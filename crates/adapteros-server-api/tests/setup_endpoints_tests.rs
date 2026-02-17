use adapteros_api_types::{
    schema_version, SetupDiscoveredModel, SetupMigrateResponse, SetupSeedModelResult,
    SetupSeedModelStatus, SetupSeedModelsResponse,
};

#[test]
fn setup_discovery_contract_exposes_path_and_registration_flag() {
    let model = SetupDiscoveredModel {
        name: "Llama-3.2".to_string(),
        path: "/var/models/Llama-3.2".to_string(),
        format: "hf".to_string(),
        backend: "mlx".to_string(),
        already_registered: true,
    };

    let value = serde_json::to_value(model).expect("serialize discovered model");
    assert_eq!(value["path"], "/var/models/Llama-3.2");
    assert_eq!(value["already_registered"], true);
}

#[test]
fn setup_seed_response_contains_buckets_and_summary_counts() {
    let seeded_item = SetupSeedModelResult {
        name: "a".to_string(),
        model_path: "/m/a".to_string(),
        status: SetupSeedModelStatus::Seeded,
        model_id: Some("mdl_a".to_string()),
        message: None,
    };
    let skipped_item = SetupSeedModelResult {
        name: "b".to_string(),
        model_path: "/m/b".to_string(),
        status: SetupSeedModelStatus::Skipped,
        model_id: None,
        message: Some("already exists".to_string()),
    };
    let failed_item = SetupSeedModelResult {
        name: "c".to_string(),
        model_path: "/m/c".to_string(),
        status: SetupSeedModelStatus::Failed,
        model_id: None,
        message: Some("invalid".to_string()),
    };

    let response = SetupSeedModelsResponse {
        schema_version: schema_version(),
        total: 3,
        seeded_count: 1,
        skipped_count: 1,
        failed_count: 1,
        seeded: vec![seeded_item.clone()],
        skipped: vec![skipped_item.clone()],
        failed: vec![failed_item.clone()],
        results: vec![seeded_item, skipped_item, failed_item],
    };

    assert_eq!(response.total, 3);
    assert_eq!(response.seeded_count, response.seeded.len());
    assert_eq!(response.skipped_count, response.skipped.len());
    assert_eq!(response.failed_count, response.failed.len());
}

#[test]
fn setup_migrate_response_contains_completion_timestamp() {
    let response = SetupMigrateResponse {
        schema_version: schema_version(),
        status: "ok".to_string(),
        completed_at: "2026-02-17T10:00:00Z".to_string(),
        message: "Migrations completed successfully".to_string(),
    };

    let parsed = chrono::DateTime::parse_from_rfc3339(&response.completed_at)
        .expect("completed_at should be RFC3339");
    assert_eq!(parsed.to_rfc3339(), "2026-02-17T10:00:00+00:00");
}
