#![cfg(all(test, feature = "extended-tests"))]

use adapteros_cli::commands::adapter_train_from_code::{self, TrainFromCodeArgs};
use adapteros_cli::output::{OutputMode, OutputWriter};
use adapteros_db::Db;
use adapteros_lora_worker::tokenizer::QwenTokenizer;
use adapteros_single_file_adapter::SingleFileAdapterLoader;
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

    let mut index = repo.index().unwrap();
    index
        .add_all(["*"].iter(), IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
        .unwrap();
    repo
}

fn fixture_repo_path() -> PathBuf {
    PathBuf::from("crates/adapteros-cli/tests/data/train_from_code_repo")
}

fn tokenizer_path() -> PathBuf {
    PathBuf::from("models/qwen2.5-7b-mlx/tokenizer.json")
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
        let temp = TempDir::new().unwrap();
        let repo_dir = temp.path().join("repo");
        fs::create_dir_all(&repo_dir).unwrap();
        copy_fixture(&fixture_repo_path(), &repo_dir);
        init_git_repo(&repo_dir);

        let db_path = temp.path().join("registry.sqlite");
        std::env::set_var("DATABASE_URL", db_path.to_string_lossy());

        let output_dir = temp.path().join("artifacts");
        let adapter_id = "test.code.ingestion".to_string();
        let repo_arg = repo_dir.to_string_lossy().to_string();

        let mut args = TrainFromCodeArgs {
            repo: repo_arg,
            adapter_id: Some(adapter_id.clone()),
            project_name: Some("TrainFixture".to_string()),
            repo_id: Some("fixture.repo".to_string()),
            tokenizer: tokenizer_path(),
            output_dir: output_dir.clone(),
            base_model: "qwen2.5-7b".to_string(),
            rank: 2,
            alpha: 8.0,
            learning_rate: 5e-4,
            batch_size: 2,
            epochs: 1,
            hidden_dim: 64,
            max_symbols: 8,
            include_private: false,
            positive_weight: 1.0,
            negative_weight: -0.5,
            skip_register: false,
            tier: 1,
            seed: Some(42),
        };

        let writer = OutputWriter::new(OutputMode::Quiet, false);
        adapter_train_from_code::run(&args, &writer)
            .await
            .expect("first training run");

        let aos_path = output_dir.join(format!("{}.aos", adapter_id));
        assert!(aos_path.exists());
        let hash_one = aos_hash(&aos_path);

        // Running again should produce the same hash and reuse registry row.
        adapter_train_from_code::run(&args, &writer)
            .await
            .expect("second training run");
        let hash_two = aos_hash(&aos_path);
        assert_eq!(hash_one, hash_two);

        // Load adapter and inspect training data contents.
        let adapter = SingleFileAdapterLoader::load(&aos_path)
            .await
            .expect("load aos");
        let tokenizer = QwenTokenizer::from_file(tokenizer_path()).expect("tokenizer available");
        let first = adapter
            .training_data
            .first()
            .expect("at least one training example");
        let decoded = tokenizer.decode(&first.target).unwrap();
        assert!(decoded.contains("Widget size"));

        let abstain = adapter
            .training_data
            .iter()
            .find(|ex| {
                ex.metadata
                    .get("reason")
                    .map(|v| v == "missing_docstring")
                    .unwrap_or(false)
            })
            .expect("negative sample present");
        let decoded_abstain = tokenizer.decode(&abstain.target).unwrap();
        assert!(decoded_abstain.contains("I don't know"));

        // Database entry should exist with matching hash.
        let db = Db::connect(&db_path.to_string_lossy()).await.unwrap();
        let stored = db.get_adapter(&adapter_id).await.unwrap().unwrap();
        assert_eq!(stored.hash_b3, hash_one);
    });
}
