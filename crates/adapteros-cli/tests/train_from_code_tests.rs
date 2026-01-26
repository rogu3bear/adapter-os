#![cfg(all(test, feature = "extended-tests", feature = "orchestrator"))]

use adapteros_cli::commands::adapter_codebase::CodebaseScopeOverrides;
use adapteros_cli::commands::adapter_train_from_code::{self, TrainFromCodeArgs};
use adapteros_cli::commands::training_common::{CommonTrainingArgs, TokenizerArg};
use adapteros_cli::output::{OutputMode, OutputWriter};
use adapteros_config::{DEFAULT_BASE_MODEL_ID, DEFAULT_MODEL_CACHE_ROOT};
use adapteros_db::Db;
use adapteros_storage::platform::common::PlatformUtils;
use blake3::Hasher;
use git2::{IndexAddOption, Repository, Signature};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use tokio::runtime::Runtime;
use walkdir::WalkDir;

fn copy_fixture(src: &Path, dst: &Path) {
    for entry in WalkDir::new(src).into_iter().filter_map(Result::ok) {
        let relative = entry.path().strip_prefix(src).unwrap();
        let target = dst.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target).unwrap();
        } else {
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::copy(entry.path(), &target).unwrap();
        }
    }
}

fn init_git_repo(path: &Path) -> Repository {
    let repo = Repository::init(path).expect("git init");
    let sig = Signature::now("Codex", "codex@example.com").unwrap();

    {
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"].iter(), IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
            .unwrap();
    }
    repo
}

fn fixture_repo_path() -> PathBuf {
    PathBuf::from("crates/adapteros-cli/tests/data/train_from_code_repo")
}

fn tokenizer_path() -> PathBuf {
    std::env::var("AOS_TOKENIZER_PATH")
        .map(PathBuf::from)
        .or_else(|_| {
            std::env::var("AOS_MODEL_PATH").map(|p| PathBuf::from(p).join("tokenizer.json"))
        })
        .unwrap_or_else(|_| {
            PathBuf::from(DEFAULT_MODEL_CACHE_ROOT)
                .join(DEFAULT_BASE_MODEL_ID)
                .join("tokenizer.json")
        })
}

fn aos_hash(path: &Path) -> String {
    let mut hasher = Hasher::new();
    let data = fs::read(path).unwrap();
    hasher.update(&data);
    hasher.finalize().to_hex().to_string()
}

#[test]
fn train_from_code_pipeline_is_deterministic() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let root = PlatformUtils::temp_dir();
        std::fs::create_dir_all(&root).expect("create var/tmp");
        let temp = TempDir::new_in(&root).unwrap();
        let repo_dir = temp.path().join("repo");
        fs::create_dir_all(&repo_dir).unwrap();
        copy_fixture(&fixture_repo_path(), &repo_dir);
        init_git_repo(&repo_dir);

        let db_path = temp.path().join("registry.sqlite");
        std::env::set_var("DATABASE_URL", &db_path);

        let output_dir = temp.path().join("artifacts");
        let adapter_id = "test.code.ingestion".to_string();
        let repo_arg = repo_dir.to_string_lossy().to_string();

        let args = TrainFromCodeArgs {
            repo: repo_arg,
            adapter_id: Some(adapter_id.clone()),
            project_name: Some("TrainFixture".to_string()),
            repo_id: Some("fixture.repo".to_string()),
            output_dir: output_dir.clone(),
            base_model: "qwen2.5-7b".to_string(),
            max_symbols: 8,
            include_private: false,
            positive_weight: 1.0,
            negative_weight: -0.5,
            skip_register: false,
            tier: 1,
            deterministic: true,
            seed: Some(42),
            tokenizer_arg: TokenizerArg {
                tokenizer: Some(tokenizer_path()),
            },
            common: CommonTrainingArgs {
                rank: 2,
                alpha: 8.0,
                learning_rate: 5e-4,
                batch_size: 2,
                epochs: 1,
                hidden_dim: 64,
            },
            scope_overrides: CodebaseScopeOverrides::default(),
        };

        let writer = OutputWriter::new(OutputMode::Quiet, false);
        adapter_train_from_code::run(&args, &writer)
            .await
            .expect("first training run");

        // The code ingestion pipeline writes to default/<adapter_id>.aos
        let aos_path = output_dir
            .join("default")
            .join(format!("{}.aos", adapter_id));
        assert!(aos_path.exists(), "AOS file should exist at {:?}", aos_path);
        let hash_one = aos_hash(&aos_path);

        // Running again should produce the same hash and reuse registry row.
        adapter_train_from_code::run(&args, &writer)
            .await
            .expect("second training run");
        let hash_two = aos_hash(&aos_path);
        assert_eq!(
            hash_one, hash_two,
            "Deterministic training should produce identical hashes"
        );

        // Database entry should exist with matching hash.
        let db = Db::connect(&db_path.to_string_lossy()).await.unwrap();
        #[allow(deprecated)]
        let stored = db.get_adapter(&adapter_id).await.unwrap().unwrap();
        assert_eq!(stored.hash_b3, hash_one);
    });
}
