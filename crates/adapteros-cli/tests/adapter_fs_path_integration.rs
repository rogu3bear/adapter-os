use adapteros_cli::commands::train_docs::TrainDocsArgs;
use adapteros_cli::commands::training_common::{CommonTrainingArgs, TokenizerArg};
use adapteros_core::{adapter_fs_path_with_root, B3Hash};
use adapteros_db::AdapterRegistrationBuilder;
use adapteros_lora_worker::training::AdapterPackager;
use tempfile::tempdir;
use tokio::fs;

#[tokio::test]
async fn train_docs_and_worker_path_align() {
    let adapters_root = tempdir().unwrap();
    let docs_dir = tempdir().unwrap();
    let tokenizer_dir = tempdir().unwrap();

    // Create minimal tokenizer to satisfy loader
    let tokenizer_path = tokenizer_dir.path().join("tokenizer.json");
    let tokenizer = tokenizers::Tokenizer::new(tokenizers::models::wordlevel::WordLevel::new(
        [("hello".to_string(), 0u32)].into_iter().collect(),
        None,
    ));
    tokenizer
        .save(tokenizer_path.to_str().unwrap(), false)
        .expect("tokenizer saved");

    // Write a tiny markdown file for training input
    let doc_path = docs_dir.path().join("doc.md");
    fs::write(&doc_path, "# Title\n\nHello world").await.unwrap();

    // Ensure worker/env paths resolve to the temp adapters root
    std::env::set_var("AOS_ADAPTERS_ROOT", adapters_root.path());

    let tenant_id = "tenant-fs";
    let base_model_id = "base-model-test";
    let revision = "rev1";
    let adapter_id = format!("system/docs/adapteros/{revision}");
    let safe_adapter_id = adapter_id.replace('/', "_");

    // Run train-docs pipeline with small, fast settings
    let args = TrainDocsArgs {
        docs_dir: docs_dir.path().to_path_buf(),
        output: Some(adapters_root.path().to_path_buf()),
        revision: Some(revision.to_string()),
        scenario: None,
        tenant_id: Some(tenant_id.to_string()),
        base_model_id: Some(base_model_id.to_string()),
        register: false,
        auto_activate: false,
        max_seq_length: 16,
        chunk_tokens: 8,
        overlap_tokens: 2,
        dry_run: false,
        db_url: None,
        skip_training: false,
        training_strategy: "identity".to_string(),
        tokenizer_arg: TokenizerArg {
            tokenizer: Some(tokenizer_path.clone()),
        },
        common: CommonTrainingArgs {
            rank: 2,
            alpha: 4.0,
            learning_rate: 1e-3,
            batch_size: 1,
            epochs: 1,
            hidden_dim: 8,
        },
    };

    args.execute().await.expect("train_docs execution");

    // Compute canonical adapter directory and file path
    let adapter_dir = adapter_fs_path_with_root(
        adapters_root.path(),
        tenant_id,
        &safe_adapter_id,
    )
    .expect("fs path resolved");
    let expected_aos = adapter_dir.join(format!("{safe_adapter_id}.aos"));

    assert!(
        expected_aos.exists(),
        "packaged adapter should exist at {:?}",
        expected_aos
    );

    // Register adapter in in-memory DB with path derived from helper
    let db = adapteros_db::Db::connect("sqlite::memory:")
        .await
        .expect("in-memory db");
    let file_bytes = fs::read(&expected_aos).await.expect("read aos");
    let file_hash = B3Hash::hash(&file_bytes).to_hex();

    let reg_params = AdapterRegistrationBuilder::new()
        .tenant_id(tenant_id)
        .adapter_id(&safe_adapter_id)
        .name("fs-path-integration")
        .hash_b3(&file_hash)
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
    let worker_resolved = adapter_fs_path_with_root(
        adapters_root.path(),
        tenant_id,
        &safe_adapter_id,
    )
    .expect("worker path");
    let worker_aos = worker_resolved.with_extension("aos");
    assert_eq!(worker_aos, expected_aos, "path should be canonical");

    // Load bytes to ensure the artifact is readable
    assert!(
        !file_bytes.is_empty(),
        "packaged adapter bytes should be non-empty"
    );

    let _packager = AdapterPackager::new(adapters_root.path());
}

