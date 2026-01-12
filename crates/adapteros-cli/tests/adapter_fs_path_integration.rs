use adapteros_cli::commands::train_docs::TrainDocsArgs;
use adapteros_core::{adapter_fs_path_with_root, B3Hash};
use adapteros_db::AdapterRegistrationBuilder;
use clap::Parser;
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::PathBuf;
use tempfile::tempdir;
use tokio::fs;

#[tokio::test]
async fn train_docs_and_worker_path_align() {
    let adapters_root = tempdir().unwrap();
    let docs_dir = tempdir().unwrap();
    let tokenizer_dir = tempdir().unwrap();

    // Create minimal tokenizer to satisfy loader
    let tokenizer_path = tokenizer_dir.path().join("tokenizer.json");
    let mut vocab = HashMap::new();
    vocab.insert("hello".to_string(), 0u32);
    vocab.insert("[UNK]".to_string(), 1u32);
    vocab.insert("[PAD]".to_string(), 2u32);
    let model = tokenizers::models::wordlevel::WordLevel::builder()
        .vocab(vocab)
        .unk_token("[UNK]".to_string())
        .build()
        .unwrap();
    let tokenizer = tokenizers::Tokenizer::new(model);
    tokenizer
        .save(tokenizer_path.to_str().unwrap(), false)
        .expect("tokenizer saved");

    // Write a tiny markdown file for training input
    let doc_path = docs_dir.path().join("doc.md");
    fs::write(&doc_path, "# Title\n\nHello world")
        .await
        .unwrap();

    // Ensure worker/env paths resolve to the temp adapters root and restore after test
    let prior_root: Option<OsString> = std::env::var_os("AOS_ADAPTERS_ROOT");
    std::env::set_var("AOS_ADAPTERS_ROOT", adapters_root.path());
    struct EnvGuard {
        key: &'static str,
        prev: Option<OsString>,
    }
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(val) = &self.prev {
                std::env::set_var(self.key, val);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }
    let _root_guard = EnvGuard {
        key: "AOS_ADAPTERS_ROOT",
        prev: prior_root,
    };

    let tenant_id = "tenant-fs";
    let base_model_path = match std::env::var("AOS_TEST_MODEL_PATH")
        .or_else(|_| std::env::var("AOS_MODEL_PATH"))
    {
        Ok(path) => PathBuf::from(path),
        Err(_) => {
            eprintln!("skipping: set AOS_TEST_MODEL_PATH or AOS_MODEL_PATH to run train_docs_and_worker_path_align");
            return;
        }
    };
    if !base_model_path.exists() {
        eprintln!(
            "skipping: base model path not found at {}",
            base_model_path.display()
        );
        return;
    }
    if !base_model_path.join("config.json").exists() {
        eprintln!(
            "skipping: config.json not found at {}",
            base_model_path.display()
        );
        return;
    }
    let weight_candidates = [
        "model.safetensors",
        "pytorch_model.bin.safetensors",
        "model.safetensors.index.json",
    ];
    if !weight_candidates
        .iter()
        .any(|name| base_model_path.join(name).exists())
    {
        eprintln!(
            "skipping: base model weights not found under {}",
            base_model_path.display()
        );
        return;
    }
    let Some(base_model_id) = base_model_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
    else {
        eprintln!(
            "skipping: base model path missing directory name at {}",
            base_model_path.display()
        );
        return;
    };
    let Some(model_cache_root) = base_model_path.parent() else {
        eprintln!(
            "skipping: base model path has no parent directory at {}",
            base_model_path.display()
        );
        return;
    };
    let prior_model_cache: Option<OsString> = std::env::var_os("AOS_MODEL_CACHE_DIR");
    std::env::set_var("AOS_MODEL_CACHE_DIR", model_cache_root);
    let _model_cache_guard = EnvGuard {
        key: "AOS_MODEL_CACHE_DIR",
        prev: prior_model_cache,
    };
    let revision = "rev1";
    let adapter_id = format!("system/docs/adapteros/{revision}");
    let safe_adapter_id = adapter_id.replace('/', "_");

    // Skip migration signature verification for temp DBs
    let prior_skip_signatures: Option<OsString> = std::env::var_os("AOS_SKIP_MIGRATION_SIGNATURES");
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let _skip_guard = EnvGuard {
        key: "AOS_SKIP_MIGRATION_SIGNATURES",
        prev: prior_skip_signatures,
    };

    #[derive(Debug, Parser)]
    struct TrainDocsTestCli {
        #[command(flatten)]
        args: TrainDocsArgs,
    }

    // Run train-docs pipeline with small, fast settings
    let args = TrainDocsTestCli::try_parse_from(vec![
        "train-docs-test".to_string(),
        "--docs-dir".to_string(),
        docs_dir.path().to_string_lossy().into_owned(),
        "--output".to_string(),
        adapters_root.path().to_string_lossy().into_owned(),
        "--revision".to_string(),
        revision.to_string(),
        "--tenant-id".to_string(),
        tenant_id.to_string(),
        "--base-model-id".to_string(),
        base_model_id.to_string(),
        "--max-seq-length".to_string(),
        "16".to_string(),
        "--chunk-tokens".to_string(),
        "8".to_string(),
        "--overlap-tokens".to_string(),
        "2".to_string(),
        "--training-strategy".to_string(),
        "identity".to_string(),
        "--tokenizer".to_string(),
        tokenizer_path.to_string_lossy().into_owned(),
        "--rank".to_string(),
        "2".to_string(),
        "--alpha".to_string(),
        "4.0".to_string(),
        "--learning-rate".to_string(),
        "0.001".to_string(),
        "--batch-size".to_string(),
        "1".to_string(),
        "--epochs".to_string(),
        "1".to_string(),
        "--hidden-dim".to_string(),
        "8".to_string(),
    ])
    .expect("train-docs args parse")
    .args;

    args.execute().await.expect("train_docs execution");

    // Compute canonical adapter directory and file path
    let adapter_dir = adapter_fs_path_with_root(adapters_root.path(), tenant_id, &safe_adapter_id)
        .expect("fs path resolved");
    let expected_aos = adapter_dir.join(format!("{safe_adapter_id}.aos"));

    assert!(
        expected_aos.exists(),
        "packaged adapter should exist at {:?}",
        expected_aos
    );
    let default_aos = adapter_fs_path_with_root(adapters_root.path(), "default", &safe_adapter_id)
        .expect("default path resolved")
        .join(format!("{safe_adapter_id}.aos"));
    assert!(
        !default_aos.exists(),
        "artifact should not be stored under default tenant path"
    );

    // Register adapter in a file-backed temp DB so pooled connections share state
    let db_dir = tempdir().unwrap();
    let db_url = format!(
        "sqlite:{}",
        db_dir.path().join("cp.sqlite3").to_string_lossy()
    );
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let db = adapteros_db::Db::connect(&db_url).await.expect("temp db");
    db.migrate().await.expect("migrate temp db");
    sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES (?1, ?2, 0)")
        .bind(tenant_id)
        .bind(tenant_id)
        .execute(db.pool())
        .await
        .expect("seed tenant");
    let tenant_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tenants WHERE id = ?1")
        .bind(tenant_id)
        .fetch_one(db.pool())
        .await
        .expect("count tenant");
    assert_eq!(tenant_count, 1, "tenant persisted in temp db");
    let file_bytes = fs::read(&expected_aos).await.expect("read aos");
    let file_view = adapteros_aos::open_aos(&file_bytes).expect("parse aos");
    let canonical_segment = file_view
        .segments
        .iter()
        .find(|seg| seg.backend_tag == adapteros_aos::BackendTag::Canonical)
        .expect("canonical segment present");

    let weights_data = canonical_segment.payload;
    let weights_hash = B3Hash::hash(weights_data).to_hex();
    let file_hash = B3Hash::hash(&file_bytes).to_hex();

    let reg_params = AdapterRegistrationBuilder::new()
        .tenant_id(tenant_id)
        .adapter_id(&safe_adapter_id)
        .name("fs-path-integration")
        .hash_b3(&weights_hash)
        .rank(2)
        .tier("warm")
        .aos_file_path(Some(expected_aos.to_string_lossy()))
        .aos_file_hash(Some(file_hash.clone()))
        .build()
        .expect("registration params");

    db.register_adapter(reg_params)
        .await
        .expect("registration succeeds");

    // Worker-side resolution should match canonical helper output
    let worker_resolved =
        adapter_fs_path_with_root(adapters_root.path(), tenant_id, &safe_adapter_id)
            .expect("worker path");
    let worker_aos = worker_resolved.join(format!("{safe_adapter_id}.aos"));
    assert_eq!(worker_aos, expected_aos, "path should be canonical");

    // Load bytes to ensure the artifact is readable
    assert!(
        !file_bytes.is_empty(),
        "packaged adapter bytes should be non-empty"
    );
}
