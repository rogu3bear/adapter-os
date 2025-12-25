//! Adapter utility functions
//!
//! Helper functions used across adapter handlers.

use adapteros_core::B3Hash;
use adapteros_types::training::LoraTier;
use axum::http::StatusCode;
use axum::Json;
use crate::types::*;
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) fn parse_hash_b3(hash_b3: &str) -> Result<B3Hash, String> {
    let trimmed = hash_b3.strip_prefix("b3:").unwrap_or(hash_b3);
    B3Hash::from_hex(trimmed).map_err(|e| e.to_string())
}

/// Reject adapter/stack mutations when other requests are in-flight.
pub(crate) fn guard_in_flight_requests(
    in_flight: &AtomicUsize,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    let current = in_flight.load(Ordering::SeqCst);
    // Subtract the current request (handled by middleware) to avoid self-blocking.
    let other_requests = current.saturating_sub(1);
    if other_requests > 0 {
        return Err((
            StatusCode::CONFLICT,
            Json(
                ErrorResponse::new("adapter in use, try again later")
                    .with_code("ADAPTER_IN_USE")
                    .with_string_details(format!(
                        "{} other request(s) currently in flight",
                        other_requests
                    )),
            ),
        ));
    }

    Ok(())
}

pub(crate) fn rollup_health_flag(
    has_corrupt: bool,
    trust_blocked: bool,
    drift_triggered: bool,
) -> adapteros_api_types::adapters::AdapterHealthFlag {
    if has_corrupt {
        adapteros_api_types::adapters::AdapterHealthFlag::Corrupt
    } else if trust_blocked {
        adapteros_api_types::adapters::AdapterHealthFlag::Unsafe
    } else if drift_triggered {
        adapteros_api_types::adapters::AdapterHealthFlag::Degraded
    } else {
        adapteros_api_types::adapters::AdapterHealthFlag::Healthy
    }
}

pub(crate) fn select_primary_subcode(
    overall: adapteros_api_types::adapters::AdapterHealthFlag,
    subcodes: &[adapteros_api_types::adapters::AdapterHealthSubcode],
) -> Option<adapteros_api_types::adapters::AdapterHealthSubcode> {
    let domain_priority = match overall {
        adapteros_api_types::adapters::AdapterHealthFlag::Corrupt => [
            adapteros_api_types::adapters::AdapterHealthDomain::Storage,
            adapteros_api_types::adapters::AdapterHealthDomain::Trust,
            adapteros_api_types::adapters::AdapterHealthDomain::Drift,
            adapteros_api_types::adapters::AdapterHealthDomain::Other,
        ],
        adapteros_api_types::adapters::AdapterHealthFlag::Unsafe => [
            adapteros_api_types::adapters::AdapterHealthDomain::Trust,
            adapteros_api_types::adapters::AdapterHealthDomain::Storage,
            adapteros_api_types::adapters::AdapterHealthDomain::Drift,
            adapteros_api_types::adapters::AdapterHealthDomain::Other,
        ],
        adapteros_api_types::adapters::AdapterHealthFlag::Degraded => [
            adapteros_api_types::adapters::AdapterHealthDomain::Drift,
            adapteros_api_types::adapters::AdapterHealthDomain::Trust,
            adapteros_api_types::adapters::AdapterHealthDomain::Storage,
            adapteros_api_types::adapters::AdapterHealthDomain::Other,
        ],
        adapteros_api_types::adapters::AdapterHealthFlag::Healthy => [
            adapteros_api_types::adapters::AdapterHealthDomain::Drift,
            adapteros_api_types::adapters::AdapterHealthDomain::Trust,
            adapteros_api_types::adapters::AdapterHealthDomain::Storage,
            adapteros_api_types::adapters::AdapterHealthDomain::Other,
        ],
    };

    for domain in domain_priority {
        if let Some(sub) = subcodes.iter().find(|s| s.domain == domain) {
            return Some(sub.clone());
        }
    }

    subcodes.first().cloned()
}

pub(crate) fn lora_tier_from_provenance(provenance_json: &Option<String>) -> Option<LoraTier> {
    provenance_json
        .as_ref()
        .and_then(|json| serde_json::from_str::<Value>(json).ok())
        .and_then(|v| {
            v.get("lora_tier")
                .and_then(|t| t.as_str())
                .map(str::to_string)
        })
        .and_then(|s| match s.as_str() {
            "micro" => Some(LoraTier::Micro),
            "standard" => Some(LoraTier::Standard),
            "max" => Some(LoraTier::Max),
            _ => None,
        })
}

pub(crate) fn lora_scope_from_provenance(
    provenance_json: &Option<String>,
    fallback_scope: Option<String>,
) -> Option<String> {
    provenance_json
        .as_ref()
        .and_then(|json| serde_json::from_str::<Value>(json).ok())
        .and_then(|v| {
            v.get("lora_scope")
                .or_else(|| v.get("scope"))
                .and_then(|s| s.as_str())
                .map(str::to_string)
        })
        .or(fallback_scope)
}

pub(crate) fn compute_serveable_state(release_state: &str, trust_state: &str) -> (bool, Option<String>) {
    let release_norm = release_state.trim().to_ascii_lowercase();
    if release_norm != "active" && release_norm != "ready" {
        return (
            false,
            Some(format!("release_state={} not serveable", release_state)),
        );
    }
    let trust_norm = trust_state.trim().to_ascii_lowercase();
    if matches!(
        trust_norm.as_str(),
        "blocked" | "blocked_regressed" | "needs_approval" | "unknown"
    ) {
        return (
            false,
            Some(format!("trust_state={} not serveable", trust_state)),
        );
    }
    (true, None)
}

pub(crate) async fn manifest_lineage_from_aos(
    aos_path: Option<&str>,
) -> Option<(Option<Vec<String>>, Option<String>, Option<String>)> {
    let path = aos_path?;
    let data = tokio::fs::read(path).await.ok()?;
    let file_view = adapteros_aos::open_aos(&data).ok()?;
    let manifest: serde_json::Value = serde_json::from_slice(file_view.manifest_bytes).ok()?;

    let dataset_version_ids = manifest
        .get("dataset_version_ids")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect::<Vec<String>>()
        })
        .filter(|ids| !ids.is_empty());

    let training_backend = manifest
        .get("training_backend")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            manifest
                .get("metadata")
                .and_then(|m| m.get("training_backend"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        });

    let scope_path = manifest
        .get("metadata")
        .and_then(|m| m.get("scope_path"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Some((dataset_version_ids, training_backend, scope_path))
}
