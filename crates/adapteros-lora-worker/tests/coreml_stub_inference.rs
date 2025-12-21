#![cfg(all(target_os = "macos", feature = "coreml-backend"))]

use adapteros_core::Result;
use adapteros_core::{backend::BackendKind, ExecutionProfile, SeedMode};
use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};
use adapteros_lora_worker::backend_factory::{
    select_backend_from_execution_profile, BackendCapabilities, BackendChoice, SelectionContext,
};
use std::io::Write;
use std::sync::{Arc, Mutex};
use tracing::Level;

#[derive(Clone)]
struct BufferWriter {
    inner: Arc<Mutex<Vec<u8>>>,
}

impl Write for BufferWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.inner.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn capture_subscriber(buffer: Arc<Mutex<Vec<u8>>>) -> tracing::subscriber::DefaultGuard {
    let make_writer = move || BufferWriter {
        inner: buffer.clone(),
    };

    let subscriber = tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_writer(make_writer)
        .without_time()
        .finish();

    tracing::subscriber::set_default(subscriber)
}

#[test]
fn coreml_stub_inference_captures_logs() -> Result<()> {
    use adapteros_lora_kernel_coreml::{ComputeUnits, CoreMLBackend};

    let buffer = Arc::new(Mutex::new(Vec::new()));
    let guard = capture_subscriber(buffer.clone());

    let profile = ExecutionProfile {
        seed_mode: SeedMode::BestEffort,
        backend_profile: BackendKind::CoreML,
    };
    let capabilities = BackendCapabilities {
        has_metal: true,
        metal_device_name: Some("Test Metal".to_string()),
        has_ane: true,
        has_coreml: true,
        has_mlx: false,
        gpu_memory_bytes: None,
    };
    let ctx = SelectionContext::new(profile, capabilities);
    let selection = select_backend_from_execution_profile(&ctx).expect("selects coreml");
    assert_eq!(selection.selected, BackendChoice::CoreML);
    assert!(!selection.overridden);

    let mut backend = CoreMLBackend::new_stub(ComputeUnits::CpuAndNeuralEngine)?;
    let mut io = IoBuffers::new(16);
    io.input_ids = vec![1, 2];
    let ring = RouterRing::new(0);

    backend.run_step(&ring, &mut io)?;
    assert_eq!(io.position, 1);
    let logits_sum: f32 = io.output_logits.iter().sum();
    assert!((logits_sum - 1.0).abs() < 1e-3);

    drop(guard);
    let captured = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
    let lowered = captured.to_lowercase();
    assert!(
        lowered.contains("coreml"),
        "expected CoreML logs, got: {captured}"
    );
    assert!(
        lowered.contains("stub mode") || lowered.contains("creating coreml kernel backend"),
        "expected stub or backend creation log, got: {captured}"
    );

    Ok(())
}
