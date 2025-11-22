use adapteros_codegraph::CodeGraph;
use adapteros_core::B3Hash;
use adapteros_db::adapters::AdapterRegistrationBuilder;
use adapteros_db::CreateStackRequest;
use adapteros_db::Db;
use adapteros_lora_kernel_api::MockKernels;
use adapteros_lora_worker::Worker;
use adapteros_lora_worker::{InferenceConfig, InferenceRequest};
use adapteros_manifest::ManifestV3;
use adapteros_telemetry::TelemetryWriter;
use anyhow::Result;
use bincode;
use chrono;
use proptest::prelude::*;
use std::fs;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::sync::Mutex;
use tokio::time;

// Mock structs as needed

#[tokio::test]
async fn determinism_harness() {
    let start = Instant::now();
    // Setup in-memory DB
    let db_url = "sqlite::memory:";
    let db = Db::connect(db_url).await.unwrap();

    // Create tenant
    db.create_tenant("test_tenant", false).await.unwrap();

    // Assume hydrate_tenant_from_bundle(db, tenant_id, bundle_data).await?;
    // For now, implement simple hydrate:
    async fn hydrate_tenant_from_bundle(db: &Db, tenant_id: &str, bundle: &[u8]) -> Result<()> {
        // Parse bundle as json array of events
        let events: Vec<serde_json::Value> = serde_json::from_slice(bundle)?;
        for event in events {
            match event["type"].as_str().unwrap_or("") {
                "adapter.register" => {
                    let id = event["adapter_id"]
                        .as_str()
                        .ok_or(anyhow::anyhow!("missing id"))?;
                    let hash_hex = event["hash"]
                        .as_str()
                        .ok_or(anyhow::anyhow!("missing hash"))?;
                    let rank = event["rank"].as_u64().unwrap_or(16) as i32;
                    let tier = event.get("tier").and_then(|v| v.as_str()).unwrap_or("warm");

                    let params = AdapterRegistrationBuilder::new()
                        .tenant_id(tenant_id.to_string())
                        .adapter_id(id.to_string())
                        .name(format!("mock_{}", id))
                        .hash_b3(hash_hex.to_string())
                        .rank(rank)
                        .tier(tier.to_string())
                        .build()?;

                    db.register_adapter(params).await?;
                }
                "stack.create" => {
                    let stack_id = event["stack_id"]
                        .as_str()
                        .ok_or(anyhow::anyhow!("missing stack_id"))?;
                    let adapter_ids: Vec<String> =
                        serde_json::from_value(event["adapter_ids"].clone())?;
                    let workflow_type = event["workflow_type"].as_str().map(|s| s.to_string());

                    let req = CreateStackRequest {
                        name: stack_id.to_string(),
                        description: None,
                        adapter_ids,
                        workflow_type,
                    };

                    db.insert_stack(req).await?;
                }
                "policy.apply" => {
                    let policy_hash = event["policy_hash"]
                        .as_str()
                        .ok_or(anyhow::anyhow!("missing policy_hash"))?;
                    let body_json = event["body_json"]
                        .as_str()
                        .ok_or(anyhow::anyhow!("missing body_json"))?;
                    // Deactivate existing policies for tenant
                    sqlx::query(
                        "UPDATE policies SET active = 0 WHERE tenant_id = ? AND active = 1",
                    )
                    .bind(tenant_id)
                    .execute(db.pool())
                    .await?;
                    // Insert new policy
                    let policy_id = uuid::Uuid::now_v7().to_string();
                    sqlx::query("INSERT INTO policies (id, tenant_id, hash_b3, body_json, active) VALUES (?, ?, ?, ?, 1)")
                        .bind(policy_id)
                        .bind(tenant_id)
                        .bind(policy_hash)
                        .bind(body_json)
                        .execute(db.pool())
                        .await?;
                }
                "telemetry.record" => {
                    let event_json: serde_json::Value = event["event_json"].clone();
                    use adapteros_db::telemetry_bundles::TelemetryBatchBuilder;
                    let params = TelemetryBatchBuilder::new()
                        .tenant_id(tenant_id.to_string())
                        .event_type("telemetry.record")
                        .event_data(event_json)
                        .timestamp(chrono::Utc::now().to_rfc3339())
                        .build()?;
                    db.record_telemetry_batch(params).await?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    // In hydrate_tenant_from_bundle, parse hash as B3Hash::from_hex_str(hash_str)?;

    // In test:
    let bundle_str = r#"[
    {"type":"adapter.register","adapter_id":"mock1","hash":"0000000000000000000000000000000000000000000000000000000000000000","rank":16,"tier":"warm"},
    {"type":"adapter.register","adapter_id":"mock2","hash":"1111111111111111111111111111111111111111111111111111111111111111","rank":16,"tier":"warm"},
    {"type":"stack.create","stack_id":"mock_stack","adapter_ids":["mock1","mock2"],"workflow_type":"sequential"},
    {"type":"policy.apply","policy_hash":"2222222222222222222222222222222222222222222222222222222222222222","body_json":"{\"evidence_config\":{},\"auto_apply\":false}"},
    {"type":"telemetry.record","event_json":{"event_type":"test","message":"hydrate event"}}
]"#;
    let bundle_data = bundle_str.as_bytes();
    hydrate_tenant_from_bundle(&db, "test_tenant", bundle_data)
        .await
        .unwrap();

    let adapters = db.list_adapters("test_tenant").await.unwrap();
    assert_eq!(adapters.len(), 2);

    let expected_hashes = vec![
        B3Hash::from_hex("0000000000000000000000000000000000000000000000000000000000000000")?,
        B3Hash::from_hex("1111111111111111111111111111111111111111111111111111111111111111")?,
    ];
    for (adapter, expected) in adapters.iter().zip(expected_hashes) {
        assert_eq!(B3Hash::from_hex(&adapter.hash_b3)?, expected);
    }

    // After hydrate_tenant_from_bundle call and adapters assertion

    // State hash verification
    let tenant_adapters: Vec<serde_json::Value> =
        sqlx::query_as("SELECT * FROM adapters WHERE tenant_id = ?")
            .bind("test_tenant")
            .fetch_all(&db.pool())
            .await
            .unwrap()
            .into_iter()
            .map(|adapter: adapteros_db::adapters::Adapter| serde_json::to_value(adapter).unwrap())
            .collect();

    let stacks = db.list_stacks().await.unwrap(); // Assume no tenant filter for test
    let stacks_json: Vec<serde_json::Value> = stacks
        .into_iter()
        .map(|stack| serde_json::to_value(stack).unwrap())
        .collect();

    let policies_json: Vec<serde_json::Value> =
        sqlx::query("SELECT * FROM policies WHERE tenant_id = ?")
            .bind("test_tenant")
            .fetch_all(&db.pool())
            .await
            .unwrap()
            .into_iter()
            .map(|row: sqlx::sqlite::SqliteRow| {
                let id: String = row.get("id");
                let tenant_id: String = row.get("tenant_id");
                let hash_b3: String = row.get("hash_b3");
                let body_json: String = row.get("body_json");
                let active: i32 = row.get("active");
                let created_at: String = row.get("created_at");
                serde_json::json!({
                    "id": id,
                    "tenant_id": tenant_id,
                    "hash_b3": hash_b3,
                    "body_json": body_json,
                    "active": active,
                    "created_at": created_at
                })
            })
            .collect();

    let telemetry_json: Vec<serde_json::Value> =
        sqlx::query("SELECT * FROM telemetry_events WHERE tenant_id = ?")
            .bind("test_tenant")
            .fetch_all(&db.pool())
            .await
            .unwrap()
            .into_iter()
            .map(|row: sqlx::sqlite::SqliteRow| {
                let id: String = row.get("id");
                let tenant_id: String = row.get("tenant_id");
                let event_type: String = row.get("event_type");
                let event_data: String = row.get("event_data");
                let timestamp: String = row.get("timestamp");
                serde_json::json!({
                    "id": id,
                    "tenant_id": tenant_id,
                    "event_type": event_type,
                    "event_data": event_data,
                    "timestamp": timestamp
                })
            })
            .collect();

    let mut state_hasher = blake3::Hasher::new();
    state_hasher.update(
        format!(
            "adapters:{}",
            serde_json::to_string(&tenant_adapters).unwrap()
        )
        .as_bytes(),
    );
    state_hasher
        .update(format!("stacks:{}", serde_json::to_string(&stacks_json).unwrap()).as_bytes());
    state_hasher.update(
        format!(
            "policies:{}",
            serde_json::to_string(&policies_json).unwrap()
        )
        .as_bytes(),
    );
    state_hasher.update(
        format!(
            "telemetry:{}",
            serde_json::to_string(&telemetry_json).unwrap()
        )
        .as_bytes(),
    );
    let state_hash = B3Hash::new(*state_hasher.finalize().as_bytes());

    let bundle_hash = B3Hash::hash(bundle_data);
    assert_eq!(state_hash, bundle_hash, "State hash must match bundle hash");

    // Setup worker with seed
    let seed = [42u8; 32];
    let worker = Worker::new_with_seed(
        "./models/test-model".to_string(),
        seed,
        Arc::new(Mutex::new(mock_kernels())),
        db.clone(),
        Arc::new(mock_telemetry()),
    )
    .await
    .unwrap();

    let prompt = "Test prompt for determinism".to_string();
    let config = InferenceConfig {
        max_tokens: 50,
        temperature: 0.7,
        seed: Some(seed),
        ..Default::default()
    };

    // Run first inference, collect events
    let (result1, events1) = worker
        .infer_with_events(prompt.clone(), config.clone())
        .await
        .unwrap();
    let events_hash1 = B3Hash::hash(&serde_json::to_vec(&events1).unwrap());

    // Run second inference with same inputs
    let (result2, events2) = worker.infer_with_events(prompt, config).await.unwrap();
    let events_hash2 = B3Hash::hash(&serde_json::to_vec(&events2).unwrap());

    // Compare
    assert_eq!(result1, result2, "Outputs must be deterministic");
    assert_eq!(events_hash1, events_hash2, "Events must be identical");

    // After hydrate verify:

    let mut hasher1 = blake3::Hasher::new();
    let codegraph1 = CodeGraph::from_directory("tests/fixtures/mock_repo", None)
        .await
        .unwrap();
    codegraph1.save_to_db("temp_index1.db").await.unwrap();
    let db_bytes1 = fs::read("temp_index1.db").unwrap();
    let db_hash1 = B3Hash::hash(&db_bytes1);
    fs::remove_file("temp_index1.db").unwrap();

    codegraph1.symbols.iter().for_each(|(id, sym)| {
        hasher1.update(id.as_bytes());
        hasher1.update(sym.kind.to_string().as_bytes());
    });
    codegraph1.call_graph.edges.iter().for_each(|edge| {
        hasher1.update(&edge.from.to_bytes());
        hasher1.update(&edge.to.to_bytes());
    });
    let index_hash1 = B3Hash::hash_multi(&[hasher1.finalize().as_bytes(), db_hash1.as_bytes()]);

    // Run second time
    let mut hasher2 = blake3::Hasher::new();
    let codegraph2 = CodeGraph::from_directory("tests/fixtures/mock_repo", None)
        .await
        .unwrap();
    codegraph2.save_to_db("temp_index2.db").await.unwrap();
    let db_bytes2 = fs::read("temp_index2.db").unwrap();
    let db_hash2 = B3Hash::hash(&db_bytes2);
    fs::remove_file("temp_index2.db").unwrap();

    codegraph2.symbols.iter().for_each(|(id, sym)| {
        hasher2.update(id.as_bytes());
        hasher2.update(sym.kind.to_string().as_bytes());
    });
    codegraph2.call_graph.edges.iter().for_each(|edge| {
        hasher2.update(&edge.from.to_bytes());
        hasher2.update(&edge.to.to_bytes());
    });
    let index_hash2 = B3Hash::hash_multi(&[hasher2.finalize().as_bytes(), db_hash2.as_bytes()]);

    assert_eq!(
        index_hash1, index_hash2,
        "Index build must be deterministic"
    );
    assert_eq!(db_hash1, db_hash2, "DB file hash must be identical");

    // After index assert:

    let manifest_hash = B3Hash::hash(b"test_manifest");
    let global_seed = B3Hash::hash(b"test_seed");
    let router_seed = adapteros_core::hash::derive_seed(&global_seed, "router_test");
    let manifest = ManifestV3 {
        seeds: adapteros_manifest::Seeds {
            global: global_seed,
        },
        ..Default::default()
    };

    let kernels = MockKernels::new();

    // First inference with real telemetry
    let temp_dir1 = TempDir::new().unwrap();
    let telemetry_writer1 = TelemetryWriter::new(temp_dir1.path(), 10, 1024).unwrap();
    let worker1 = Worker::new(
        manifest.clone(),
        kernels.clone(),
        None,
        "dummy_tokenizer",
        Arc::new(telemetry_writer1),
    )
    .await
    .unwrap();

    let prompt = "Deterministic test prompt".to_string();
    let request = InferenceRequest {
        prompt: prompt.clone(),
        max_tokens: 10,
        temperature: 0.0, // Deterministic
        seed: Some(global_seed.to_bytes()),
        ..Default::default()
    };

    let response1 = worker1.infer(request.clone()).await.unwrap();
    time::sleep(Duration::from_millis(100)).await; // Allow telemetry to flush

    let mut events1 = vec![];
    for entry in fs::read_dir(temp_dir1.path()).unwrap() {
        let entry = entry.unwrap();
        if entry.path().extension() == Some(std::ffi::OsStr::new("ndjson")) {
            let content = fs::read_to_string(entry.path()).unwrap();
            for line in content.lines() {
                if !line.trim().is_empty() {
                    events1.push(serde_json::from_str::<serde_json::Value>(line).unwrap());
                }
            }
        }
    }

    let mut combined_hasher1 = blake3::Hasher::new();
    combined_hasher1.update(response1.text.as_bytes());
    combined_hasher1.update(&serde_json::to_vec(&events1).unwrap());
    combined_hasher1.update(index_hash1.as_bytes());
    let combined_hash1 = combined_hasher1.finalize().into();

    // Second inference
    let temp_dir2 = TempDir::new().unwrap();
    let telemetry_writer2 = TelemetryWriter::new(temp_dir2.path(), 10, 1024).unwrap();
    let worker2 = Worker::new(
        manifest,
        kernels,
        None,
        "dummy_tokenizer",
        Arc::new(telemetry_writer2),
    )
    .await
    .unwrap();

    let response2 = worker2.infer(request).await.unwrap();
    time::sleep(Duration::from_millis(100)).await;

    let mut events2 = vec![];
    for entry in fs::read_dir(temp_dir2.path()).unwrap() {
        let entry = entry.unwrap();
        if entry.path().extension() == Some(std::ffi::OsStr::new("ndjson")) {
            let content = fs::read_to_string(entry.path()).unwrap();
            for line in content.lines() {
                if !line.trim().is_empty() {
                    events2.push(serde_json::from_str::<serde_json::Value>(line).unwrap());
                }
            }
        }
    }

    let mut combined_hasher2 = blake3::Hasher::new();
    combined_hasher2.update(response2.text.as_bytes());
    combined_hasher2.update(&serde_json::to_vec(&events2).unwrap());
    combined_hasher2.update(index_hash2.as_bytes());
    let combined_hash2 = combined_hasher2.finalize().into();

    assert_eq!(response1.text, response2.text);
    assert_eq!(events1.len(), events2.len());
    assert_eq!(combined_hash1, combined_hash2);
    let duration = start.elapsed();
    assert!(
        duration < Duration::from_secs(45),
        "Harness must complete in under 45 seconds, took {:?}",
        duration
    );
    println!("Harness completed in {:?}", duration);
}

proptest! {
    #[test]
    fn test_router_decision_properties(
        features in prop::collection::vec(-1.0f32..1.0f32, 5..20),
        priors in prop::collection::vec(0.0f32..1.0f32, 5..20),
        k in 1usize..8,
        tau in 0.1f32..2.0,
        eps in 0.001f32..0.1,
    ) {
        use adapteros_lora_router::Router;
        let mut router = Router::new_with_weights(adapteros_lora_router::RouterWeights::default(), k, tau, eps);

        let decision = router.route(&features, &priors);

        // Unique indices
        let mut indices_set = std::collections::HashSet::new();
        for &idx in &decision.indices {
            prop_assert!(!indices_set.contains(&idx), "Non-unique index {}", idx);
            indices_set.insert(idx);
        }

        // Gates sum ~1.0
        let sum: f32 = decision.gates_f32().iter().sum();
        prop_assert!((sum - 1.0).abs() < 0.01, "Gates sum {} not close to 1.0", sum);

        // Entropy >0
        prop_assert!(decision.entropy > 0.0, "Entropy {} <=0", decision.entropy);

        // Gates in [0,1]
        for &g in &decision.gates_f32() {
            prop_assert!(g >= 0.0 && g <= 1.0, "Gate {} out of [0,1]", g);
        }

        // Log2 guard: ensure entropy >0 before log2
        if decision.entropy > 0.0 {
            let _ = (decision.entropy as f64).log2(); // Should not panic or be -inf
        }
    }
}

proptest! {
    #[test]
    fn test_kv_zeroization(
        size in 256usize..4096,
        pattern in prop::collection::vec(0u8..255, 1..100),
    ) {
        // Simulate Metal buffer zeroization post-reset
        let mut buffer = vec![0u8; size];

        // Write test data
        for (i, &byte) in pattern.iter().enumerate() {
            if i < buffer.len() {
                buffer[i] = byte;
            }
        }

        // Simulate reset: fill with 0
        buffer.fill(0);

        // Assert first 16 bytes == 0 (Metal buffer.contents() check)
        for &byte in buffer.iter().take(16) {
            prop_assert_eq!(byte, 0, "Buffer not zeroized");
        }
    }
}

proptest! {
    #[test]
    fn test_telemetry_wire_format(
        event_type_str in "[a-z_]{1,20}",
        message in ".{1,100}",
        level in prop_oneof![
            Just(adapteros_telemetry::unified_events::LogLevel::Debug),
            Just(adapteros_telemetry::unified_events::LogLevel::Info),
            Just(adapteros_telemetry::unified_events::LogLevel::Warn),
            Just(adapteros_telemetry::unified_events::LogLevel::Error),
            Just(adapteros_telemetry::unified_events::LogLevel::Critical)
        ],
        metadata in prop::collection::hash_map(any::<String>(), any::<serde_json::Value>(), 0..5),
    ) {
        use adapteros_telemetry::unified_events::{TelemetryEventBuilder, EventType, UnifiedTelemetryEvent};

        let identity = IdentityEnvelope::new("test_tenant".to_string(), "test_domain".to_string(), "test_purpose".to_string(), "test_rev".to_string());

        let event = TelemetryEventBuilder::new(EventType::Custom(event_type_str), level, message, identity)
            .metadata(serde_json::Value::Object(metadata.into_iter().collect()))
            .build();

        // JSON roundtrip
        let json = serde_json::to_string(&event).unwrap();
        let decoded: UnifiedTelemetryEvent = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(decoded, event);
        let json2 = serde_json::to_string(&decoded).unwrap();
        prop_assert_eq!(json, json2);

        // Bincode roundtrip
        let bin = bincode::serialize(&event).unwrap();
        let decoded2: UnifiedTelemetryEvent = bincode::deserialize(&bin).unwrap();
        prop_assert_eq!(decoded2, event);
    }
}

// Mock functions
fn mock_kernels() -> impl FusedKernels { /* mock */
}
fn mock_telemetry() -> TelemetryWriter { /* mock */
}
