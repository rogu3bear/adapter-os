use adapteros_config::resolve_manifest_cache_dir;
use adapteros_core::{AosError, B3Hash, Result};
use adapteros_model_hub::manifest::ManifestV3;
use std::fs;
use tracing::{info, warn};

/// Parse manifest content from YAML or JSON
pub fn parse_manifest(content: &str) -> Result<ManifestV3> {
    serde_yaml::from_str(content).or_else(|yaml_err| {
        serde_json::from_str(content).map_err(|json_err| {
            AosError::Validation(format!(
                "Failed to parse manifest as YAML ({}) or JSON ({})",
                yaml_err, json_err
            ))
        })
    })
}

/// Fetch manifest from control plane by hash
pub fn fetch_manifest_from_cp(
    cp_url: &str,
    tenant_id: &str,
    manifest_hash: &B3Hash,
) -> Result<String> {
    let url = format!(
        "{}/v1/tenants/{}/manifests/{}",
        cp_url,
        tenant_id,
        manifest_hash.to_hex()
    );

    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(10)))
        .build()
        .new_agent();

    let response = agent
        .get(&url)
        .call()
        .map_err(|e| AosError::Worker(format!("Failed to fetch manifest: {}", e)))?;

    let body = response
        .into_body()
        .read_to_string()
        .map_err(|e| AosError::Worker(format!("Failed to read manifest response: {}", e)))?;

    let parsed: adapteros_api_types::workers::WorkerManifestFetchResponse =
        serde_json::from_str(&body).map_err(|e| {
            AosError::Worker(format!("Failed to parse manifest response JSON: {}", e))
        })?;

    if parsed.manifest_hash != manifest_hash.to_hex() {
        return Err(AosError::Validation(format!(
            "Manifest hash mismatch from CP: expected {}, got {}",
            manifest_hash.to_hex(),
            parsed.manifest_hash
        )));
    }

    let computed = B3Hash::hash(parsed.manifest_json.as_bytes());
    if computed != *manifest_hash {
        return Err(AosError::Validation(format!(
            "Manifest content hash mismatch: expected {}, computed {}",
            manifest_hash.to_hex(),
            computed.to_hex()
        )));
    }

    Ok(parsed.manifest_json)
}

/// Cache manifest locally for reuse
pub fn cache_manifest(manifest_hash: &B3Hash, manifest_json: &str) {
    let resolved_cache = match resolve_manifest_cache_dir() {
        Ok(path) => path,
        Err(err) => {
            warn!(error = %err, "Skipping manifest cache write because cache path is invalid");
            return;
        }
    };
    let cache_dir = resolved_cache.path;
    if fs::create_dir_all(&cache_dir).is_ok() {
        let cache_path = cache_dir.join(format!("{}.json", manifest_hash.to_hex()));
        info!(
            path = %cache_path.display(),
            source = %resolved_cache.source,
            "Writing manifest cache entry"
        );
        let mut tmp_file = match tempfile::Builder::new()
            .prefix(&format!("{}_", manifest_hash.to_hex()))
            .suffix(".json.tmp")
            .tempfile_in(&cache_dir)
        {
            Ok(f) => f,
            Err(e) => {
                warn!(error = %e, dir = %cache_dir.display(), "Failed to create manifest cache temp file");
                return;
            }
        };

        if let Err(e) = std::io::Write::write_all(&mut tmp_file, manifest_json.as_bytes()) {
            warn!(error = %e, "Failed to write manifest cache temp file");
            return;
        }

        if let Err(e) = tmp_file.persist(&cache_path) {
            warn!(error = %e.error, path = %cache_path.display(), "Failed to persist manifest cache temp file to final destination");
        }
    } else {
        warn!(
            path = %cache_dir.display(),
            source = %resolved_cache.source,
            "Failed to create manifest cache directory"
        );
    }
}

pub struct LoadedManifest {
    pub manifest: ManifestV3,
    pub _canonical_json: String,
    pub hash: B3Hash,
}
