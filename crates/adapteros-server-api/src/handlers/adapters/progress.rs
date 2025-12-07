use tracing::info;

pub fn emit_adapter_progress(
    adapter_id: &str,
    stage: &str,
    file_name: Option<&str>,
    pct: f32,
    message: &str,
) {
    // Fire-and-forget: log only; never propagate errors.
    info!(
        adapter_id = %adapter_id,
        stage = %stage,
        file = file_name,
        pct = %pct,
        message = %message,
        "adapter_import_progress"
    );
}
