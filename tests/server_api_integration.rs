mod common;

use std::path::PathBuf;

use adapteros_api_types::inference::{RouterDecisionChainEntry, RouterModelType};
use adapteros_core::{
    version::{API_SCHEMA_VERSION, VERSION as AOS_VERSION},
    B3Hash,
};
use adapteros_db::{sqlx, workers::WorkerRegistrationParams};
use adapteros_manifest::{
    Adapter, AdapterCategory, AdapterScope, AdapterTier, AssuranceTier, Base, BundleCfg,
    ManifestV3, Policies, RouterCfg, Sampling, Seeds, TelemetryCfg,
};
use adapteros_server_api::{
    types::{
        InferenceRequestInternal, RouterSummary, TokenUsage, WorkerInferResponse, WorkerTrace,
    },
    InferenceCore,
};
use common::fixtures_consolidated::{TestAdapterFactory, TestAppStateBuilder};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    sync::oneshot,
    task::JoinHandle,
    time::Duration,
};
use uuid::Uuid;

#[tokio::test]
async fn golden_path_inference_over_uds() -> Result<(), Box<dyn std::error::Error>> {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");

    let adapter_id = "reference-adapter";
    let manifest = build_test_manifest(adapter_id);
    let manifest_json = manifest.to_json()?;
    let manifest_hash = manifest.compute_hash()?.to_hex();
    let backend = "mlx".to_string();

    std::fs::create_dir_all("var/run")?;
    let uds_path =
        PathBuf::from("var/run").join(format!("server-api-worker-{}.sock", Uuid::new_v4()));

    let stub_response = build_stub_response(adapter_id, &backend);
    let server_handle = spawn_stub_worker(uds_path.clone(), stub_response).await?;

    let mut state = TestAppStateBuilder::new().build().await?;
    state = state.with_manifest_info(manifest_hash.clone(), backend.clone());

    state
        .db
        .create_manifest("default", &manifest_hash, &manifest_json)
        .await?;
    sqlx::query(
        "INSERT OR IGNORE INTO models (id, name, hash_b3, license_hash_b3, config_hash_b3, tokenizer_hash_b3, tokenizer_cfg_hash_b3) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&manifest.base.model_id)
    .bind(&manifest.base.model_id)
    .bind(manifest.base.model_hash.to_hex())
    .bind(manifest.base.license_hash.as_ref().map(|h| h.to_hex()))
    .bind(manifest.base.config_hash.to_hex())
    .bind(manifest.base.tokenizer_hash.to_hex())
    .bind(manifest.base.tokenizer_cfg_hash.to_hex())
    .execute(state.db.pool())
    .await?;
    let plan_id = Uuid::new_v4().to_string();
    let node_id = state
        .db
        .register_node("integration-node", "http://localhost:0")
        .await?;
    sqlx::query(
        "INSERT INTO plans (id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json, layout_hash_b3) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&plan_id)
    .bind("default")
    .bind(&manifest_hash)
    .bind(&manifest_hash)
    .bind("{}")
    .bind(&manifest_hash)
    .execute(state.db.pool())
    .await?;
    state
        .db
        .update_base_model_status("default", &manifest.base.model_id, "loaded", None, Some(0))
        .await?;
    TestAdapterFactory::create_adapter(&state.db, adapter_id, "default").await?;

    let worker_id = "worker-golden";
    state
        .db
        .register_worker(WorkerRegistrationParams {
            worker_id: worker_id.to_string(),
            tenant_id: "default".to_string(),
            node_id,
            plan_id,
            uds_path: uds_path.to_string_lossy().to_string(),
            pid: std::process::id() as i32,
            manifest_hash: manifest_hash.clone(),
            backend: Some(backend.clone()),
            model_hash_b3: Some(manifest.base.model_hash.to_hex()),
            capabilities_json: None,
            schema_version: API_SCHEMA_VERSION.to_string(),
            api_version: API_SCHEMA_VERSION.to_string(),
        })
        .await?;
    state.db.update_worker_status(worker_id, "healthy").await?;
    state
        .db
        .update_worker_health_metrics(worker_id, "healthy", 1.0, 1, 0, 0)
        .await?;

    let mut request = InferenceRequestInternal::new("default".to_string(), "ping".to_string());
    request.adapters = Some(vec![adapter_id.to_string()]);
    request.model = Some(manifest.base.model_id.clone());

    let core = InferenceCore::new(&state);
    let result = core
        .route_and_infer(request, None, None, None)
        .await
        .expect("golden path inference should succeed");

    assert_eq!(result.text, "stub-response");
    assert_eq!(result.adapters_used, vec![adapter_id.to_string()]);
    assert_eq!(result.backend_used, Some(backend));

    match tokio::time::timeout(Duration::from_secs(2), server_handle).await {
        Ok(Ok(())) => {}
        Ok(Err(err)) => Err(err)?,
        Err(_) => return Err("stub worker did not shut down in time".into()),
    }

    let _ = std::fs::remove_file(&uds_path);

    Ok(())
}

fn build_test_manifest(adapter_id: &str) -> ManifestV3 {
    ManifestV3 {
        schema: "adapteros.manifest.v3".to_string(),
        base: Base {
            model_id: "test-model".to_string(),
            model_hash: B3Hash::hash(b"model"),
            arch: "qwen2".to_string(),
            vocab_size: 32000,
            hidden_dim: 2048,
            n_layers: 12,
            n_heads: 16,
            routing_bias: 1.0,
            config_hash: B3Hash::hash(b"config"),
            tokenizer_hash: B3Hash::hash(b"tokenizer"),
            tokenizer_cfg_hash: B3Hash::hash(b"tokenizer_cfg"),
            license_hash: None,
            rope_scaling_override: None,
        },
        adapters: vec![Adapter {
            id: adapter_id.to_string(),
            hash: B3Hash::hash(adapter_id.as_bytes()),
            assurance_tier: AssuranceTier::Standard,
            tier: AdapterTier::Persistent,
            rank: 8,
            alpha: 16.0,
            lora_strength: None,
            target_modules: vec!["q_proj".to_string()],
            ttl: None,
            acl: vec![],
            warmup_prompt: None,
            dependencies: None,
            determinism_seed: None,
            determinism_backend: None,
            determinism_device: None,
            drift_reference_backend: None,
            drift_metric: None,
            drift_baseline_backend: None,
            drift_test_backend: None,
            drift_tier: None,
            drift_slice_size: None,
            drift_slice_offset: None,
            drift_loss_metric: None,
            category: AdapterCategory::Code,
            scope: AdapterScope::Global,
            framework_id: None,
            framework_version: None,
            repo_id: None,
            commit_sha: None,
            intent: None,
            recommended_for_moe: true,
            auto_promote: true,
            eviction_priority: adapteros_manifest::EvictionPriority::Normal,
            free_tokens: None,
            hot_experts: None,
        }],
        router: RouterCfg {
            k_sparse: 1,
            gate_quant: "q15".to_string(),
            entropy_floor: 0.01,
            tau: 1.0,
            sample_tokens_full: 32,
            warmup: false,
            algorithm: "weighted".to_string(),
            safe_mode: false,
            orthogonal_penalty: 0.1,
            shared_downsample: false,
            compression_ratio: 0.8,
            multi_path_enabled: false,
            diversity_threshold: 0.05,
            orthogonal_constraints: false,
        },
        telemetry: TelemetryCfg {
            schema_hash: B3Hash::hash(b"telemetry"),
            sampling: Sampling {
                token: 0.0,
                router: 0.0,
                inference: 1.0,
            },
            router_full_tokens: 32,
            bundle: BundleCfg {
                max_events: 1000,
                max_bytes: 1_048_576,
            },
        },
        policies: Policies::default(),
        seeds: Seeds {
            global: B3Hash::hash(b"global-seed"),
            manifest_hash: B3Hash::hash(b"manifest-seed"),
            parent_cpid: None,
        },
        coreml: None,
        fusion: None,
    }
}

fn build_stub_router_chain(adapter_id: &str) -> Vec<RouterDecisionChainEntry> {
    vec![RouterDecisionChainEntry {
        step: 0,
        input_token_id: Some(0),
        adapter_indices: vec![0],
        adapter_ids: vec![adapter_id.to_string()],
        gates_q15: vec![32767],
        entropy: 0.0,
        decision_hash: None,
        previous_hash: None,
        entry_hash: B3Hash::hash(format!("{adapter_id}-step-0").as_bytes()).to_hex(),
        policy_mask_digest_b3: None,
        policy_overrides_applied: None,
    }]
}

fn build_stub_response(adapter_id: &str, backend: &str) -> WorkerInferResponse {
    WorkerInferResponse {
        text: Some("stub-response".to_string()),
        status: "ok".to_string(),
        trace: WorkerTrace {
            router_summary: RouterSummary {
                adapters_used: vec![adapter_id.to_string()],
            },
            token_count: 6,
            router_decisions: None,
            router_decision_chain: Some(build_stub_router_chain(adapter_id)),
            model_type: Some(RouterModelType::Dense),
        },
        run_receipt: None,
        token_usage: Some(TokenUsage {
            prompt_tokens: 3,
            completion_tokens: 6,
            billed_input_tokens: 3,
            billed_output_tokens: 6,
        }),
        backend_used: Some(backend.to_string()),
        backend_version: Some(AOS_VERSION.to_string()),
        fallback_triggered: false,
        coreml_compute_preference: None,
        coreml_compute_units: None,
        coreml_gpu_used: None,
        coreml_package_hash: None,
        coreml_expected_package_hash: None,
        coreml_hash_mismatch: None,
        fallback_backend: None,
        determinism_mode_applied: Some("deterministic".to_string()),
        unavailable_pinned_adapters: None,
        pinned_routing_fallback: None,
        placement_trace: None,
        stop_reason_code: None,
        stop_reason_token_index: None,
        stop_policy_digest_b3: None,
        tokenizer_digest_b3: None,
        backend_raw: None,
    }
}

async fn spawn_stub_worker(
    socket_path: PathBuf,
    response: WorkerInferResponse,
) -> Result<JoinHandle<()>, Box<dyn std::error::Error>> {
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let _ = std::fs::remove_file(&socket_path);
    let listener = UnixListener::bind(&socket_path)?;
    let response_json = serde_json::to_string(&response)?;
    let (ready_tx, ready_rx) = oneshot::channel();

    let handle = tokio::spawn(async move {
        let _ = ready_tx.send(());
        if let Ok((stream, _)) = listener.accept().await {
            let _ = handle_stub_connection(stream, response_json).await;
        }
    });

    let _ = ready_rx.await;
    Ok(handle)
}

async fn handle_stub_connection(stream: UnixStream, response_json: String) -> std::io::Result<()> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    let mut _request_line = String::new();
    reader.read_line(&mut _request_line).await?;

    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line).await?;
        if bytes == 0 || line.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse().unwrap_or(0);
            }
        }
    }

    if content_length > 0 {
        let mut body = vec![0u8; content_length];
        reader.read_exact(&mut body).await?;
    }

    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        response_json.len(),
        response_json
    );

    write_half.write_all(response.as_bytes()).await?;
    write_half.shutdown().await?;
    Ok(())
}
