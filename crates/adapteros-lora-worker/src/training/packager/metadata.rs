//! Metadata parsing and normalization helpers

use super::types::{BranchMetadata, ScanRootMetadata};
use crate::training::{
    LORA_STRENGTH_DEFAULT_MAX, LORA_STRENGTH_DEFAULT_MICRO, LORA_STRENGTH_DEFAULT_STANDARD,
};
use adapteros_core::AosError;
use adapteros_core::Result;
use adapteros_normalization::normalize_repo_slug;
use adapteros_types::coreml::CoreMLOpKind;
use adapteros_types::training::LoraTier;
use std::collections::HashMap;

/// Codebase scope metadata extracted from the metadata HashMap.
#[derive(Debug, Clone, Default)]
pub(crate) struct ScopeMetadataExtract {
    pub scope_repo: Option<String>,
    pub scope_branch: Option<String>,
    pub scope_commit: Option<String>,
    pub scope_scan_root: Option<String>,
    pub scope_remote_url: Option<String>,
    pub repo_slug: Option<String>,
    pub session_id: Option<String>,
    pub session_name: Option<String>,
    pub session_tags: Option<Vec<String>>,
    pub scan_roots: Vec<ScanRootMetadata>,
}

/// Extracted manifest fields from metadata HashMap.
/// Centralizes parsing logic to ensure consistency across packaging methods.
#[derive(Debug, Clone)]
pub(crate) struct ManifestFieldsExtract {
    pub lora_tier: Option<LoraTier>,
    pub lora_strength: Option<f32>,
    pub category: String,
    pub tier: String,
    pub dataset_version_ids: Option<Vec<String>>,
    pub data_spec_hash: Option<String>,
    pub data_lineage_mode: Option<String>,
    pub synthetic_mode: Option<bool>,
    pub training_slice_id: Option<String>,
    pub backend_policy: Option<String>,
    pub recommended_for_moe: bool,
    pub stream_mode: Option<bool>,
    pub scope_meta: ScopeMetadataExtract,
}

pub(crate) fn default_determinism_mode() -> String {
    if cfg!(feature = "deterministic-only") {
        "deterministic-only".to_string()
    } else {
        "best-effort".to_string()
    }
}

pub(crate) fn default_category() -> String {
    "domain-adapter".to_string()
}

pub(crate) fn default_tier() -> String {
    "warm".to_string()
}

pub(crate) fn default_scope() -> String {
    "project".to_string()
}

pub(crate) fn default_recommended_for_moe() -> bool {
    true
}

pub(crate) fn normalize_optional_str(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
}

pub(crate) fn normalize_scan_roots(mut roots: Vec<ScanRootMetadata>) -> Vec<ScanRootMetadata> {
    for root in roots.iter_mut() {
        let trimmed = root.path.trim();
        if trimmed != root.path {
            root.path = trimmed.to_string();
        }
    }
    roots.retain(|root| !root.path.is_empty());
    roots
}

pub(crate) fn parse_lora_tier(metadata: &HashMap<String, String>) -> Option<LoraTier> {
    metadata.get("lora_tier").and_then(|v| match v.as_str() {
        "micro" => Some(LoraTier::Micro),
        "standard" => Some(LoraTier::Standard),
        "max" => Some(LoraTier::Max),
        _ => None,
    })
}

pub(crate) fn parse_metadata_bool(metadata: &HashMap<String, String>, key: &str) -> Option<bool> {
    metadata
        .get(key)
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "true" | "1" | "yes" | "y"))
}

pub(crate) fn default_strength_for_tier(tier: Option<LoraTier>) -> Option<f32> {
    match tier {
        Some(LoraTier::Micro) => Some(LORA_STRENGTH_DEFAULT_MICRO),
        Some(LoraTier::Standard) => Some(LORA_STRENGTH_DEFAULT_STANDARD),
        Some(LoraTier::Max) => Some(LORA_STRENGTH_DEFAULT_MAX),
        None => None,
    }
}

/// Parse scan-root metadata from the metadata HashMap.
///
/// Supports two formats:
/// 1. JSON array in `scan_roots` key: `[{"path": "src", "label": "main"}, ...]`
/// 2. Single scan root from canonical scan-root keys (scan_root_relative, scope_scan_root,
///    scan_root_path, repo_root_path, repo_path) with optional supporting fields
pub(crate) fn parse_scan_roots_from_metadata(
    metadata: &HashMap<String, String>,
) -> Vec<ScanRootMetadata> {
    // Try parsing JSON array first
    if let Some(raw) = metadata.get("scan_roots") {
        if let Ok(mut roots) = serde_json::from_str::<Vec<ScanRootMetadata>>(raw) {
            roots = normalize_scan_roots(roots);
            if !roots.is_empty() {
                return roots;
            }
        }
        if let Ok(paths) = serde_json::from_str::<Vec<String>>(raw) {
            let roots = normalize_scan_roots(
                paths
                    .into_iter()
                    .map(|path| ScanRootMetadata {
                        path,
                        label: None,
                        file_count: None,
                        byte_count: None,
                        content_hash: None,
                        scanned_at: None,
                    })
                    .collect(),
            );
            if !roots.is_empty() {
                return roots;
            }
        }
    }

    // Fall back to single scan root from scope_scan_root
    if let Some(path) = resolve_scan_root_from_metadata(metadata) {
        let root = ScanRootMetadata {
            path,
            label: metadata.get("scan_root_label").cloned(),
            file_count: metadata
                .get("scan_root_file_count")
                .and_then(|v| v.parse().ok()),
            byte_count: metadata
                .get("scan_root_byte_count")
                .and_then(|v| v.parse().ok()),
            content_hash: metadata.get("scan_root_content_hash").cloned(),
            scanned_at: metadata.get("scan_root_scanned_at").cloned(),
        };
        return vec![root];
    }

    Vec::new()
}

pub(crate) fn resolve_scan_root_from_metadata(
    metadata: &HashMap<String, String>,
) -> Option<String> {
    let candidates = [
        metadata.get("scan_root_relative"),
        metadata.get("scope_scan_root"),
        metadata.get("scan_root_path"),
        metadata.get("repo_root_path"),
        metadata.get("repo_path"),
    ];

    for path in candidates.into_iter().flatten() {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    None
}

pub(crate) fn parse_bool_strict(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

pub(crate) fn metadata_has_scan_root_keys(metadata: &HashMap<String, String>) -> bool {
    let keys = [
        "scan_roots",
        "scan_root_relative",
        "scope_scan_root",
        "scan_root_path",
        "repo_root_path",
        "repo_path",
    ];
    keys.iter().any(|key| {
        metadata
            .get(*key)
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false)
    })
}

pub(crate) fn metadata_indicates_codebase(metadata: &HashMap<String, String>) -> bool {
    if metadata_has_scan_root_keys(metadata) {
        return true;
    }

    let keys = [
        "codebase_scope",
        "repo_identifier",
        "scope_repo_id",
        "repo_id",
    ];
    keys.iter().any(|key| {
        metadata
            .get(*key)
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false)
    })
}

pub(crate) fn parse_scan_roots_strict(
    metadata: &HashMap<String, String>,
) -> Result<Option<Vec<ScanRootMetadata>>> {
    if let Some(raw) = metadata.get("scan_roots") {
        let value: serde_json::Value = serde_json::from_str(raw).map_err(|e| {
            AosError::InvalidManifest(format!("scan_roots metadata is invalid JSON: {}", e))
        })?;
        if let Ok(roots) = serde_json::from_value::<Vec<ScanRootMetadata>>(value.clone()) {
            let roots = normalize_scan_roots(roots);
            if roots.is_empty() {
                return Err(AosError::InvalidManifest(
                    "scan_roots metadata provided but empty".to_string(),
                ));
            }
            return Ok(Some(roots));
        }
        if let Ok(paths) = serde_json::from_value::<Vec<String>>(value) {
            let roots = normalize_scan_roots(
                paths
                    .into_iter()
                    .map(|path| ScanRootMetadata {
                        path,
                        label: None,
                        file_count: None,
                        byte_count: None,
                        content_hash: None,
                        scanned_at: None,
                    })
                    .collect(),
            );
            if roots.is_empty() {
                return Err(AosError::InvalidManifest(
                    "scan_roots metadata provided but empty".to_string(),
                ));
            }
            return Ok(Some(roots));
        }
        return Err(AosError::InvalidManifest(
            "scan_roots metadata is invalid JSON array".to_string(),
        ));
    }
    Ok(None)
}

pub(crate) fn expected_repo_slug_from_metadata(
    metadata: &HashMap<String, String>,
) -> Option<String> {
    metadata
        .get("repo_slug")
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
        .or_else(|| {
            metadata
                .get("scope_repo_slug")
                .map(|v| v.trim())
                .filter(|v| !v.is_empty())
                .map(|v| v.to_string())
        })
        .or_else(|| {
            let candidates = [
                metadata.get("scope_repo"),
                metadata.get("scope_repo_id"),
                metadata.get("repo_identifier"),
                metadata.get("repo_id"),
                metadata.get("repo_name"),
            ];
            for value in candidates.into_iter().flatten() {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    return Some(normalize_repo_slug(trimmed));
                }
            }
            None
        })
}

/// Extract manifest fields from metadata HashMap.
/// Ensures consistent parsing across all packaging methods.
pub(crate) fn extract_manifest_fields(metadata: &HashMap<String, String>) -> ManifestFieldsExtract {
    let lora_tier = parse_lora_tier(metadata);
    let lora_strength = metadata
        .get("lora_strength")
        .and_then(|v| v.parse::<f32>().ok())
        .or_else(|| default_strength_for_tier(lora_tier));
    let category = metadata
        .get("category")
        .cloned()
        .unwrap_or_else(default_category);
    let tier = metadata.get("tier").cloned().unwrap_or_else(default_tier);

    let dataset_version_ids = metadata.get("dataset_version_ids").and_then(|raw| {
        serde_json::from_str::<serde_json::Value>(raw)
            .ok()
            .and_then(|val| {
                let arr = val.as_array()?;
                let ids: Vec<String> = arr
                    .iter()
                    .filter_map(|v| {
                        if let Some(id) = v.get("dataset_version_id").and_then(|s| s.as_str()) {
                            Some(id.to_string())
                        } else {
                            v.as_str().map(|s| s.to_string())
                        }
                    })
                    .collect();
                if ids.is_empty() {
                    None
                } else {
                    Some(ids)
                }
            })
    });

    let data_spec_hash = metadata.get("data_spec_hash").cloned();
    let data_lineage_mode = metadata.get("data_lineage_mode").cloned();
    let synthetic_mode = metadata
        .get("synthetic_mode")
        .map(|v| v == "true" || v == "1");
    let training_slice_id = metadata.get("training_slice_id").cloned();
    let backend_policy = metadata.get("backend_policy").cloned();
    let recommended_for_moe = metadata
        .get("recommended_for_moe")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(true);
    let stream_mode = parse_metadata_bool(metadata, "stream_mode");

    let scope_meta = extract_scope_metadata(metadata);

    ManifestFieldsExtract {
        lora_tier,
        lora_strength,
        category,
        tier,
        dataset_version_ids,
        data_spec_hash,
        data_lineage_mode,
        synthetic_mode,
        training_slice_id,
        backend_policy,
        recommended_for_moe,
        stream_mode,
        scope_meta,
    }
}

pub(crate) fn apply_branch_metadata_defaults(metadata: &mut HashMap<String, String>) {
    let branch_meta = BranchMetadata::from_metadata(metadata);
    if let Some(branch) = branch_meta.branch {
        metadata.entry("scope_branch".to_string()).or_insert(branch);
    }
    if let Some(commit) = branch_meta.commit {
        metadata.entry("scope_commit".to_string()).or_insert(commit);
    }
    if let Some(repo_name) = branch_meta.repo_name {
        metadata
            .entry("scope_repo".to_string())
            .or_insert(repo_name);
    }
    if let Some(repo_slug) = branch_meta.repo_slug {
        metadata.entry("repo_slug".to_string()).or_insert(repo_slug);
    }
    if let Some(remote_url) = branch_meta.remote_url {
        metadata
            .entry("scope_remote_url".to_string())
            .or_insert(remote_url);
    }
}

pub(crate) fn normalize_commit_metadata(metadata: &mut HashMap<String, String>) {
    let commit_full = metadata
        .get("commit_sha")
        .or_else(|| metadata.get("scope_commit_full"))
        .or_else(|| metadata.get("commit_full"))
        .or_else(|| metadata.get("repo_commit"))
        .or_else(|| metadata.get("scope_commit"))
        .or_else(|| metadata.get("commit"))
        .cloned();

    if let Some(full_sha) = commit_full {
        metadata
            .entry("commit_sha".to_string())
            .or_insert_with(|| full_sha.clone());
        let short = if full_sha.len() >= 7 {
            full_sha[..7].to_string()
        } else {
            full_sha.clone()
        };
        metadata.entry("scope_commit".to_string()).or_insert(short);
    }
}

pub(crate) fn apply_codebase_scope_defaults(metadata: &mut HashMap<String, String>) {
    let existing = metadata
        .get("codebase_scope")
        .map(|v| v.trim())
        .unwrap_or("");
    if !existing.is_empty() {
        return;
    }

    let scope_candidate = metadata
        .get("repo_identifier")
        .or_else(|| metadata.get("repo_id"))
        .or_else(|| metadata.get("scope_repo_id"))
        .map(|v| v.trim())
        .filter(|v| !v.is_empty());

    if let Some(scope) = scope_candidate {
        metadata.insert("codebase_scope".to_string(), scope.to_string());
        return;
    }

    let slug_candidate = metadata
        .get("repo_slug")
        .or_else(|| metadata.get("scope_repo_slug"))
        .map(|v| v.trim())
        .filter(|v| !v.is_empty());

    if let Some(slug) = slug_candidate {
        let normalized = normalize_repo_slug(slug);
        metadata.insert("codebase_scope".to_string(), format!("repo:{}", normalized));
    }
}

/// Extract codebase scope metadata from the metadata HashMap.
pub(crate) fn extract_scope_metadata(metadata: &HashMap<String, String>) -> ScopeMetadataExtract {
    let repo_slug = normalize_optional_str(metadata.get("repo_slug").map(String::as_str))
        .or_else(|| normalize_optional_str(metadata.get("scope_repo_slug").map(String::as_str)))
        .map(|slug| normalize_repo_slug(&slug))
        .or_else(|| {
            normalize_optional_str(
                metadata
                    .get("scope_repo")
                    .or_else(|| metadata.get("repo_identifier"))
                    .or_else(|| metadata.get("scope_repo_id"))
                    .or_else(|| metadata.get("repo_name"))
                    .map(String::as_str),
            )
            .map(|v| normalize_repo_slug(&v))
        });
    let scan_roots = parse_scan_roots_from_metadata(metadata);
    let scope_scan_root = resolve_scan_root_from_metadata(metadata)
        .or_else(|| scan_roots.first().map(|root| root.path.clone()));

    ScopeMetadataExtract {
        scope_repo: normalize_optional_str(
            metadata
                .get("scope_repo")
                .or_else(|| metadata.get("repo_name"))
                .map(String::as_str),
        ),
        scope_branch: normalize_optional_str(
            metadata
                .get("scope_branch")
                .or_else(|| metadata.get("repo_branch"))
                .map(String::as_str),
        ),
        scope_commit: normalize_optional_str(
            metadata
                .get("scope_commit")
                .or_else(|| metadata.get("repo_commit"))
                .or_else(|| metadata.get("commit_sha"))
                .or_else(|| metadata.get("commit_short_sha"))
                .or_else(|| metadata.get("commit"))
                .map(String::as_str),
        ),
        scope_scan_root,
        scope_remote_url: normalize_optional_str(
            metadata
                .get("scope_remote_url")
                .or_else(|| metadata.get("repo_remote"))
                .map(String::as_str),
        ),
        repo_slug,
        session_id: normalize_optional_str(metadata.get("session_id").map(String::as_str)),
        session_name: normalize_optional_str(metadata.get("session_name").map(String::as_str)),
        session_tags: parse_session_tags(metadata.get("session_tags")),
        scan_roots,
    }
}

pub(crate) fn persist_scope_metadata(
    metadata: &mut HashMap<String, String>,
    scope_meta: &ScopeMetadataExtract,
) {
    fn insert_if_missing(
        metadata: &mut HashMap<String, String>,
        key: &str,
        value: Option<&String>,
    ) {
        let Some(value) = value else {
            return;
        };
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return;
        }
        let should_insert = metadata
            .get(key)
            .map(|existing| existing.trim().is_empty())
            .unwrap_or(true);
        if should_insert {
            metadata.insert(key.to_string(), trimmed.to_string());
        }
    }

    insert_if_missing(metadata, "scope_repo", scope_meta.scope_repo.as_ref());
    insert_if_missing(metadata, "scope_branch", scope_meta.scope_branch.as_ref());
    insert_if_missing(metadata, "scope_commit", scope_meta.scope_commit.as_ref());
    insert_if_missing(
        metadata,
        "scope_scan_root",
        scope_meta.scope_scan_root.as_ref(),
    );
    insert_if_missing(
        metadata,
        "scope_remote_url",
        scope_meta.scope_remote_url.as_ref(),
    );
    insert_if_missing(metadata, "repo_slug", scope_meta.repo_slug.as_ref());
    insert_if_missing(metadata, "session_id", scope_meta.session_id.as_ref());
    insert_if_missing(metadata, "session_name", scope_meta.session_name.as_ref());

    if let Some(tags) = scope_meta.session_tags.as_ref().filter(|t| !t.is_empty()) {
        let should_insert = metadata
            .get("session_tags")
            .map(|existing| existing.trim().is_empty())
            .unwrap_or(true);
        if should_insert {
            let value = serde_json::to_string(tags).unwrap_or_else(|_| tags.join(","));
            metadata.insert("session_tags".to_string(), value);
        }
    }

    if !scope_meta.scan_roots.is_empty() {
        let should_insert = metadata
            .get("scan_roots")
            .map(|existing| existing.trim().is_empty())
            .unwrap_or(true);
        if should_insert {
            if let Ok(serialized) = serde_json::to_string(&scope_meta.scan_roots) {
                metadata.insert("scan_roots".to_string(), serialized);
            }
        }
    }
}

pub(crate) fn parse_session_tags(raw: Option<&String>) -> Option<Vec<String>> {
    let raw = raw?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed.starts_with('[') {
        if let Ok(mut tags) = serde_json::from_str::<Vec<String>>(trimmed) {
            normalize_session_tags(&mut tags);
            return if tags.is_empty() { None } else { Some(tags) };
        }
    }

    let mut tags: Vec<String> = trimmed
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();
    normalize_session_tags(&mut tags);

    if tags.is_empty() {
        None
    } else {
        Some(tags)
    }
}

pub(crate) fn normalize_session_tags(tags: &mut Vec<String>) {
    if tags.is_empty() {
        return;
    }
    for tag in tags.iter_mut() {
        let trimmed = tag.trim();
        if trimmed != tag.as_str() {
            *tag = trimmed.to_string();
        }
    }
    tags.retain(|t| !t.is_empty());
    if tags.len() > 1 {
        tags.sort();
        tags.dedup();
    }
}

pub(crate) fn canonicalize_backend_label(raw: &str) -> String {
    let lower = raw.trim().to_ascii_lowercase();
    if lower.contains("coreml") {
        "coreml".to_string()
    } else if lower.contains("mlx") {
        "mlx".to_string()
    } else if lower.contains("metal") {
        "metal".to_string()
    } else if lower.contains("cpu") {
        "cpu".to_string()
    } else {
        lower
    }
}

pub(crate) fn is_valid_graph_target(target: &str) -> bool {
    !target.trim().is_empty()
        && target.len() <= 256
        && target
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/'))
}

pub(crate) fn infer_op_kind_from_target(target: &str) -> CoreMLOpKind {
    let lower = target.to_lowercase();
    if lower.contains("q_proj") || lower.contains(".q_proj") || lower.contains("query") {
        CoreMLOpKind::AttentionQ
    } else if lower.contains("k_proj") || lower.contains(".k_proj") || lower.contains("key") {
        CoreMLOpKind::AttentionK
    } else if lower.contains("v_proj") || lower.contains(".v_proj") || lower.contains("value") {
        CoreMLOpKind::AttentionV
    } else if lower.contains("o_proj") || lower.contains(".o_proj") || lower.contains("out_proj") {
        CoreMLOpKind::AttentionO
    } else if lower.contains("gate") {
        CoreMLOpKind::MlpGate
    } else if lower.contains("up_proj") {
        CoreMLOpKind::MlpUp
    } else if lower.contains("down_proj") {
        CoreMLOpKind::MlpDown
    } else {
        CoreMLOpKind::AttentionO
    }
}
