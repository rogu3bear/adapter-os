//! Unit tests for training API handlers
//!
//! Tests placeholder handlers that return empty/unimplemented responses.
//! These tests verify the contract of each handler while they await migration
//! from adapteros-server-api.

use axum::{extract::Path, http::StatusCode, Json};
use serde_json::json;
use uuid::Uuid;

use adapteros_server_api_training::handlers::{
    cancel_training, get_checkpoints, get_training_job, list_training_jobs, start_training,
    StartTrainingRequest, StartTrainingResponse, TrainingJobStatus,
};

#[tokio::test]
async fn test_list_training_jobs_returns_empty_vec() {
    let Json(jobs) = list_training_jobs().await;
    assert_eq!(jobs.len(), 0, "Expected empty list of training jobs");
}

#[tokio::test]
async fn test_get_training_job_returns_not_found() {
    let job_id = Uuid::new_v4();
    let result = get_training_job(Path(job_id)).await;
    assert!(result.is_err(), "Expected Err result");
    assert_eq!(
        result.unwrap_err(),
        StatusCode::NOT_FOUND,
        "Expected NOT_FOUND status code"
    );
}

#[tokio::test]
async fn test_start_training_returns_not_implemented() {
    let request = StartTrainingRequest {
        adapter_id: Some(Uuid::new_v4()),
        config: json!({"learning_rate": 0.001, "epochs": 10}),
    };
    let result = start_training(Json(request)).await;
    assert!(result.is_err(), "Expected Err result");
    assert_eq!(
        result.unwrap_err(),
        StatusCode::NOT_IMPLEMENTED,
        "Expected NOT_IMPLEMENTED status code"
    );
}

#[tokio::test]
async fn test_cancel_training_returns_not_implemented() {
    let job_id = Uuid::new_v4();
    let status = cancel_training(Path(job_id)).await;
    assert_eq!(
        status,
        StatusCode::NOT_IMPLEMENTED,
        "Expected NOT_IMPLEMENTED status code"
    );
}

#[tokio::test]
async fn test_get_checkpoints_returns_empty_vec() {
    let job_id = Uuid::new_v4();
    let Json(checkpoints) = get_checkpoints(Path(job_id)).await;
    assert_eq!(checkpoints.len(), 0, "Expected empty list of checkpoints");
}

// Serde serialization/deserialization tests

#[test]
fn test_training_job_status_serialize() {
    let status = TrainingJobStatus {
        job_id: Uuid::new_v4(),
        status: "running".to_string(),
        progress: Some(0.42),
        message: Some("Epoch 3/10".to_string()),
    };

    let json = serde_json::to_value(&status).expect("Failed to serialize");
    assert_eq!(json["status"], "running");
    assert_eq!(json["progress"], 0.42);
    assert_eq!(json["message"], "Epoch 3/10");
}

#[test]
fn test_training_job_status_deserialize() {
    let job_id = Uuid::new_v4();
    let json = json!({
        "job_id": job_id,
        "status": "completed",
        "progress": 1.0,
        "message": "Training finished"
    });

    let status: TrainingJobStatus = serde_json::from_value(json).expect("Failed to deserialize");
    assert_eq!(status.job_id, job_id);
    assert_eq!(status.status, "completed");
    assert_eq!(status.progress, Some(1.0));
    assert_eq!(status.message, Some("Training finished".to_string()));
}

#[test]
fn test_training_job_status_optional_fields() {
    let job_id = Uuid::new_v4();
    let json = json!({
        "job_id": job_id,
        "status": "pending"
    });

    let status: TrainingJobStatus = serde_json::from_value(json).expect("Failed to deserialize");
    assert_eq!(status.job_id, job_id);
    assert_eq!(status.status, "pending");
    assert_eq!(status.progress, None);
    assert_eq!(status.message, None);
}

#[test]
fn test_start_training_request_serialize() {
    let adapter_id = Uuid::new_v4();
    let request = StartTrainingRequest {
        adapter_id: Some(adapter_id),
        config: json!({"learning_rate": 0.001, "batch_size": 32}),
    };

    let json = serde_json::to_value(&request).expect("Failed to serialize");
    assert_eq!(json["adapter_id"], adapter_id.to_string());
    assert_eq!(json["config"]["learning_rate"], 0.001);
    assert_eq!(json["config"]["batch_size"], 32);
}

#[test]
fn test_start_training_request_deserialize() {
    let adapter_id = Uuid::new_v4();
    let json = json!({
        "adapter_id": adapter_id,
        "config": {"epochs": 5, "optimizer": "adam"}
    });

    let request: StartTrainingRequest =
        serde_json::from_value(json).expect("Failed to deserialize");
    assert_eq!(request.adapter_id, Some(adapter_id));
    assert_eq!(request.config["epochs"], 5);
    assert_eq!(request.config["optimizer"], "adam");
}

#[test]
fn test_start_training_request_no_adapter_id() {
    let json = json!({
        "adapter_id": null,
        "config": {"model": "base-7b"}
    });

    let request: StartTrainingRequest =
        serde_json::from_value(json).expect("Failed to deserialize");
    assert_eq!(request.adapter_id, None);
    assert_eq!(request.config["model"], "base-7b");
}

#[test]
fn test_start_training_response_serialize() {
    let job_id = Uuid::new_v4();
    let response = StartTrainingResponse {
        job_id,
        status: "queued".to_string(),
    };

    let json = serde_json::to_value(&response).expect("Failed to serialize");
    assert_eq!(json["job_id"], job_id.to_string());
    assert_eq!(json["status"], "queued");
}

#[test]
fn test_start_training_response_deserialize() {
    let job_id = Uuid::new_v4();
    let json = json!({
        "job_id": job_id,
        "status": "starting"
    });

    let response: StartTrainingResponse =
        serde_json::from_value(json).expect("Failed to deserialize");
    assert_eq!(response.job_id, job_id);
    assert_eq!(response.status, "starting");
}

#[test]
fn test_training_job_status_round_trip() {
    let original = TrainingJobStatus {
        job_id: Uuid::new_v4(),
        status: "failed".to_string(),
        progress: Some(0.75),
        message: Some("Out of memory".to_string()),
    };

    let json = serde_json::to_value(&original).expect("Failed to serialize");
    let deserialized: TrainingJobStatus =
        serde_json::from_value(json).expect("Failed to deserialize");

    assert_eq!(deserialized.job_id, original.job_id);
    assert_eq!(deserialized.status, original.status);
    assert_eq!(deserialized.progress, original.progress);
    assert_eq!(deserialized.message, original.message);
}

#[test]
fn test_start_training_request_round_trip() {
    let original = StartTrainingRequest {
        adapter_id: Some(Uuid::new_v4()),
        config: json!({
            "learning_rate": 0.0003,
            "epochs": 20,
            "batch_size": 16,
            "gradient_accumulation_steps": 4
        }),
    };

    let json = serde_json::to_value(&original).expect("Failed to serialize");
    let deserialized: StartTrainingRequest =
        serde_json::from_value(json).expect("Failed to deserialize");

    assert_eq!(deserialized.adapter_id, original.adapter_id);
    assert_eq!(deserialized.config, original.config);
}

#[test]
fn test_start_training_response_round_trip() {
    let original = StartTrainingResponse {
        job_id: Uuid::new_v4(),
        status: "preparing".to_string(),
    };

    let json = serde_json::to_value(&original).expect("Failed to serialize");
    let deserialized: StartTrainingResponse =
        serde_json::from_value(json).expect("Failed to deserialize");

    assert_eq!(deserialized.job_id, original.job_id);
    assert_eq!(deserialized.status, original.status);
}
