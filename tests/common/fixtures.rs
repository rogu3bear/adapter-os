//! Test data fixtures for integration tests
//!
//! Provides reusable test data and fixtures:
//! - Adapter fixtures with varying configurations
//! - Dataset fixtures for training tests
//! - Policy fixtures for compliance tests
//! - Request/response payload templates

#![allow(dead_code)]

use serde_json::json;

/// Adapter test fixtures
pub mod adapters {
    use serde_json::json;

    pub fn basic_adapter_payload() -> serde_json::Value {
        json!({
            "id": "test-adapter-001",
            "hash": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "tier": "persistent",
            "rank": 8
        })
    }

    pub fn with_id(id: &str) -> serde_json::Value {
        let mut payload = basic_adapter_payload();
        payload["id"] = serde_json::Value::String(id.to_string());
        payload
    }

    pub fn with_tier(tier: &str) -> serde_json::Value {
        let mut payload = basic_adapter_payload();
        payload["tier"] = serde_json::Value::String(tier.to_string());
        payload
    }

    pub fn k_sparse_routing_adapter() -> serde_json::Value {
        json!({
            "id": "k-sparse-router-001",
            "hash": "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210",
            "tier": "warm",
            "rank": 16,
            "routing_gates": [0.95, 0.87, 0.72]
        })
    }

    pub fn hot_swap_adapter() -> serde_json::Value {
        json!({
            "id": "hot-swap-candidate",
            "hash": "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
            "tier": "ephemeral",
            "rank": 4
        })
    }

    pub fn streaming_inference_adapter() -> serde_json::Value {
        json!({
            "id": "streaming-inference-001",
            "hash": "1111111111111111111111111111111111111111111111111111111111111111",
            "tier": "hot",
            "rank": 32
        })
    }

    pub fn pinned_adapter() -> serde_json::Value {
        json!({
            "id": "critical-production-adapter",
            "hash": "2222222222222222222222222222222222222222222222222222222222222222",
            "tier": "persistent",
            "rank": 8,
            "pinned": true,
            "pinned_until": "2025-12-31T23:59:59Z"
        })
    }

    pub fn expired_ttl_adapter() -> serde_json::Value {
        json!({
            "id": "temporary-adapter",
            "hash": "3333333333333333333333333333333333333333333333333333333333333333",
            "tier": "ephemeral",
            "rank": 2,
            "expires_at": "2025-01-01T00:00:00Z"
        })
    }
}

/// Dataset test fixtures
pub mod datasets {
    use serde_json::json;

    pub fn basic_dataset_payload() -> serde_json::Value {
        json!({
            "name": "test-dataset-001",
            "format": "jsonl",
            "validation_status": "pending"
        })
    }

    pub fn qa_dataset() -> serde_json::Value {
        json!({
            "name": "qa-dataset",
            "format": "jsonl",
            "description": "Question-answer dataset for training",
            "sample_records": [
                {
                    "input": "What is Rust?",
                    "target": "Rust is a systems programming language focused on safety and performance."
                },
                {
                    "input": "Explain ownership",
                    "target": "Ownership is Rust's memory management system that ensures memory safety without a garbage collector."
                }
            ]
        })
    }

    pub fn masked_lm_dataset() -> serde_json::Value {
        json!({
            "name": "masked-lm-dataset",
            "format": "jsonl",
            "strategy": "masked_lm",
            "validation_status": "valid"
        })
    }

    pub fn large_chunked_dataset() -> serde_json::Value {
        json!({
            "name": "large-dataset",
            "format": "jsonl",
            "size_bytes": 1_000_000_000,
            "chunk_count": 10,
            "requires_chunked_upload": true
        })
    }

    pub fn malformed_dataset() -> serde_json::Value {
        json!({
            "name": "invalid-dataset",
            "format": "jsonl",
            "validation_status": "invalid",
            "validation_errors": ["Missing 'input' field in record 1", "Invalid JSON in record 5"]
        })
    }
}

/// Training test fixtures
pub mod training {
    use serde_json::json;

    pub fn basic_training_request(dataset_id: &str) -> serde_json::Value {
        json!({
            "dataset_id": dataset_id,
            "adapter_id": "trained-adapter-001",
            "rank": 16,
            "alpha": 32,
            "epochs": 3,
            "learning_rate": 0.001
        })
    }

    pub fn training_with_template(template: &str, dataset_id: &str) -> serde_json::Value {
        json!({
            "dataset_id": dataset_id,
            "adapter_id": "template-trained-adapter",
            "template": template,
            "epochs": 5
        })
    }

    pub fn training_job_response(job_id: &str) -> serde_json::Value {
        json!({
            "id": job_id,
            "status": "running",
            "progress_pct": 45,
            "loss": 0.234,
            "tokens_per_sec": 1024,
            "eta_seconds": 3600
        })
    }

    pub fn completed_training_job(job_id: &str) -> serde_json::Value {
        json!({
            "id": job_id,
            "status": "completed",
            "progress_pct": 100,
            "loss": 0.045,
            "tokens_per_sec": 2048,
            "duration_seconds": 7200,
            "artifact_path": "/artifacts/trained-adapter.aos"
        })
    }

    pub fn failed_training_job(job_id: &str, error: &str) -> serde_json::Value {
        json!({
            "id": job_id,
            "status": "failed",
            "progress_pct": 23,
            "error": error,
            "error_timestamp": "2025-11-23T12:34:56Z"
        })
    }
}

/// Inference test fixtures
pub mod inference {
    use serde_json::json;

    pub fn basic_inference_request(prompt: &str) -> serde_json::Value {
        json!({
            "prompt": prompt,
            "max_tokens": 100,
            "temperature": 0.7
        })
    }

    pub fn streaming_inference_request(prompt: &str) -> serde_json::Value {
        json!({
            "prompt": prompt,
            "max_tokens": 200,
            "temperature": 0.7,
            "stream": true
        })
    }

    pub fn multi_adapter_inference_request(prompt: &str, adapters: Vec<&str>) -> serde_json::Value {
        json!({
            "prompt": prompt,
            "max_tokens": 100,
            "adapters": adapters,
            "router_mode": "k-sparse"
        })
    }

    pub fn batch_inference_requests() -> serde_json::Value {
        json!({
            "requests": [
                {"prompt": "Hello world", "max_tokens": 50},
                {"prompt": "Explain AI", "max_tokens": 100},
                {"prompt": "What is Rust?", "max_tokens": 75}
            ]
        })
    }

    pub fn inference_response(text: &str) -> serde_json::Value {
        json!({
            "id": "inf-123456",
            "object": "text_completion",
            "created": 1700000000,
            "model": "qwen2.5-7b",
            "choices": [
                {
                    "text": text,
                    "index": 0,
                    "logprobs": null,
                    "finish_reason": "stop"
                }
            ],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 25,
                "total_tokens": 35
            }
        })
    }

    pub fn streaming_chunk(content: &str) -> serde_json::Value {
        json!({
            "id": "chatcmpl-123",
            "object": "chat.completion.chunk",
            "created": 1700000000,
            "model": "qwen2.5-7b",
            "choices": [
                {
                    "delta": {"content": content},
                    "index": 0,
                    "finish_reason": null
                }
            ]
        })
    }
}

/// Policy test fixtures
pub mod policies {
    use serde_json::json;

    pub fn egress_policy() -> serde_json::Value {
        json!({
            "cpid": "egress-001",
            "name": "Production Egress Block",
            "policy_type": "egress",
            "config": {
                "production_mode": true,
                "allowed_protocols": ["unix-domain-socket"],
                "block_network": true
            }
        })
    }

    pub fn determinism_policy() -> serde_json::Value {
        json!({
            "cpid": "determinism-001",
            "name": "Deterministic Execution",
            "policy_type": "determinism",
            "config": {
                "rng_seeding": "hkdf",
                "serial_execution": true
            }
        })
    }

    pub fn evidence_policy() -> serde_json::Value {
        json!({
            "cpid": "evidence-001",
            "name": "Evidence Tracking",
            "policy_type": "evidence",
            "config": {
                "min_relevance_score": 0.7,
                "min_confidence_score": 0.8,
                "require_source_validation": true
            }
        })
    }

    pub fn naming_policy() -> serde_json::Value {
        json!({
            "cpid": "naming-001",
            "name": "Semantic Naming",
            "policy_type": "naming",
            "config": {
                "format": "{tenant}/{domain}/{purpose}/{revision}",
                "max_revision_gap": 5,
                "reserved_tenants": ["system", "admin", "root", "default", "test"]
            }
        })
    }
}

/// Authentication/RBAC test fixtures
pub mod auth {
    use serde_json::json;

    pub fn login_request(email: &str, password: &str) -> serde_json::Value {
        json!({
            "email": email,
            "password": password
        })
    }

    pub fn login_response(token: &str) -> serde_json::Value {
        json!({
            "access_token": token,
            "token_type": "Bearer",
            "expires_in": 28800
        })
    }

    pub fn bootstrap_request(email: &str, password: &str, display_name: &str) -> serde_json::Value {
        json!({
            "email": email,
            "password": password,
            "display_name": display_name
        })
    }

    pub fn bootstrap_response(token: &str) -> serde_json::Value {
        json!({
            "access_token": token,
            "token_type": "Bearer",
            "admin_created": true,
            "expires_in": 28800
        })
    }

    pub fn user_info_response() -> serde_json::Value {
        json!({
            "id": "user-001",
            "email": "testadmin@example.com",
            "display_name": "Test Admin",
            "role": "admin",
            "created_at": "2025-01-01T00:00:00Z"
        })
    }
}

/// Utility functions for fixture composition
pub mod utils {
    /// Create a fixture with custom fields merged in
    pub fn merge_fixture(
        base: serde_json::Value,
        overrides: serde_json::Value,
    ) -> serde_json::Value {
        let mut merged = base;
        if let (
            serde_json::Value::Object(ref mut base_obj),
            serde_json::Value::Object(override_obj),
        ) = (&mut merged, overrides)
        {
            for (key, value) in override_obj {
                base_obj.insert(key, value);
            }
        }
        merged
    }

    /// Create multiple fixtures with IDs
    pub fn create_multiple_fixtures(
        template_fn: fn(&str) -> serde_json::Value,
        count: usize,
    ) -> Vec<serde_json::Value> {
        (0..count)
            .map(|i| template_fn(&format!("fixture-{:03}", i)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adapter_fixtures() {
        let basic = adapters::basic_adapter_payload();
        assert_eq!(basic["tier"], "persistent");

        let custom = adapters::with_id("custom-adapter");
        assert_eq!(custom["id"], "custom-adapter");
    }

    #[test]
    fn test_dataset_fixtures() {
        let qa = datasets::qa_dataset();
        assert_eq!(qa["format"], "jsonl");
        assert!(qa["sample_records"].is_array());
    }

    #[test]
    fn test_inference_fixtures() {
        let req = inference::basic_inference_request("Hello");
        assert_eq!(req["prompt"], "Hello");
        assert_eq!(req["max_tokens"], 100);
    }

    #[test]
    fn test_policy_fixtures() {
        let policy = policies::egress_policy();
        assert_eq!(policy["policy_type"], "egress");
    }

    #[test]
    fn test_auth_fixtures() {
        let login = auth::login_request("test@example.com", "password");
        assert_eq!(login["email"], "test@example.com");
    }

    #[test]
    fn test_fixture_merge() {
        let base = adapters::basic_adapter_payload();
        let overrides = json!({"tier": "ephemeral"});
        let merged = utils::merge_fixture(base, overrides);
        assert_eq!(merged["tier"], "ephemeral");
    }

    #[test]
    fn test_multiple_fixtures() {
        let fixtures = utils::create_multiple_fixtures(adapters::with_id, 5);
        assert_eq!(fixtures.len(), 5);
        assert_eq!(fixtures[0]["id"], "fixture-000");
        assert_eq!(fixtures[4]["id"], "fixture-004");
    }
}
