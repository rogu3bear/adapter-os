use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;

use crate::cmd_replay::{
    compute_context_digest, compute_receipt, load_json, ContextManifest, InputTokens,
    ReplayExpectation, TokenTrace,
};
use crate::output::OutputWriter;

fn default_fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test_data")
        .join("replay_fixtures")
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let data = serde_json::to_string_pretty(value)
        .with_context(|| format!("Failed to serialize {}", path.display()))?;
    fs::write(path, data).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

pub fn run(
    request_id: &str,
    out_dir: &Path,
    fixtures: Option<&Path>,
    output: &OutputWriter,
) -> Result<ReplayExpectation> {
    let fixture_root: PathBuf = fixtures
        .map(PathBuf::from)
        .unwrap_or_else(default_fixture_root);
    let source_dir = fixture_root.join(request_id);

    if !source_dir.exists() {
        return Err(adapteros_core::AosError::NotFound(format!(
            "Fixture {} not found at {}",
            request_id,
            source_dir.display()
        ))
        .into());
    }

    let manifest_path = source_dir.join("context_manifest.json");
    let trace_path = source_dir.join("token_trace.json");
    let tokens_path = source_dir.join("input_tokens.json");

    let manifest: ContextManifest = load_json(&manifest_path)?;
    let trace: TokenTrace = load_json(&trace_path)?;
    let input_tokens: InputTokens = load_json(&tokens_path)?;

    fs::create_dir_all(out_dir)
        .with_context(|| format!("Failed to create {}", out_dir.display()))?;

    // Normalize and write copies to output directory to keep exports deterministic
    write_json(&out_dir.join("context_manifest.json"), &manifest)?;
    write_json(&out_dir.join("token_trace.json"), &trace)?;
    write_json(&out_dir.join("input_tokens.json"), &input_tokens)?;

    let context_digest = compute_context_digest(&manifest)?;
    let receipt = compute_receipt(&context_digest, &input_tokens.tokens, &trace)?;
    let expected_output_tokens: Vec<u32> = trace.steps.iter().map(|s| s.output_id).collect();

    let expectation = ReplayExpectation {
        request_id: manifest.request_id.clone(),
        cpid: manifest.cpid.clone(),
        plan_id: manifest.plan_id.clone(),
        worker_id: manifest.worker_id.clone(),
        allow_cross_worker: manifest.allow_cross_worker,
        expected_context_digest: context_digest.to_hex(),
        expected_receipt: receipt.to_hex(),
        expected_output_tokens,
    };

    write_json(&out_dir.join("expected_report.json"), &expectation)?;

    if output.is_verbose() {
        output.progress(format!(
            "Exported replay artifacts for {} -> {}",
            request_id,
            out_dir.display()
        ));
    }

    Ok(expectation)
}
