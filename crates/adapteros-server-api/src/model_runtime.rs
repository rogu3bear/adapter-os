//! Minimal model runtime stub used by the API to track base model load/unload.
//!
//! This is a placeholder to satisfy handler wiring in environments where
//! an actual backend (e.g., MLX FFI) is not linked.

#[derive(Default)]
pub struct ModelRuntime;

impl ModelRuntime {
    pub fn new() -> Self {
        Self
    }

    pub fn load_model(
        &mut self,
        _tenant_id: &str,
        _model_id: &str,
        _model_path: &str,
    ) -> Result<(), String> {
        // no-op stub
        Ok(())
    }

    pub fn unload_model(&mut self, _tenant_id: &str, _model_id: &str) -> Result<(), String> {
        // no-op stub
        Ok(())
    }

    pub fn snapshot_all_models(
        &self,
        _tenant_id: &str,
    ) -> (Vec<crate::types::BaseModelStatusResponse>, i32, i32) {
        (
            vec![], // no models loaded in stub
            0,      // total memory
            0,      // active count
        )
    }
}
