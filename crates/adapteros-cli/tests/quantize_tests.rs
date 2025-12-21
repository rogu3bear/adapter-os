//! Quantization command tests
//!
//! NOTE: These tests are ignored pending CLI refactoring.
//! The commands::quantize_qwen and output modules are not currently exported from adapteros_cli.
//! Also requires safetensors crate in dev-dependencies.

#[tokio::test]
#[ignore = "Pending CLI refactoring - commands module not exported"]
async fn quantize_qwen_processes_safetensors_file() {
    // TODO: Update imports once commands module is exported and safetensors added to dev-deps
}
