use adapteros_lora_kernel_mtl::noise_tracker::{
    create_reference_data, extract_buffer_data, NoiseTracker, NoiseTrackingConfig,
};

fn make_tracker(config: NoiseTrackingConfig) -> NoiseTracker {
    NoiseTracker::new(config, None)
}

#[test]
fn tracker_behaviour_matrix() {
    let mut tracker = make_tracker(NoiseTrackingConfig::default());
    tracker
        .track_layer_error("layer", &[1.0, 2.0, 3.0], Some(&[1.01, 1.99, 3.01]))
        .unwrap();
    assert!(tracker.get_layer_stats("layer").is_some());
    tracker.track_step().unwrap();
    assert_eq!(tracker.step_count(), 1);
    assert!(tracker.get_layer_stats("layer").is_none());

    let mut strict = make_tracker(NoiseTrackingConfig {
        strict_mode: true,
        error_threshold: 1e-10,
        ..NoiseTrackingConfig::default()
    });
    let mut warn = make_tracker(NoiseTrackingConfig {
        strict_mode: false,
        error_threshold: 1e-10,
        ..NoiseTrackingConfig::default()
    });
    let quant = [1.0, 2.0, 3.0];
    let reference = [2.0, 3.0, 4.0];
    assert!(strict
        .track_layer_error("bad", &quant, Some(&reference))
        .is_err());
    assert!(warn
        .track_layer_error("bad", &quant, Some(&reference))
        .is_ok());

    let mut disabled = make_tracker(NoiseTrackingConfig {
        enabled: false,
        ..NoiseTrackingConfig::default()
    });
    disabled
        .track_layer_error("off", &quant, Some(&reference))
        .unwrap();
    assert!(disabled.get_layer_stats("off").is_none());
}

#[test]
fn reference_generation_reduces_spikes() {
    let spike = vec![0.0, 0.0, 10.0, 0.0, 0.0];
    let reference = create_reference_data(&spike);
    assert_eq!(reference.len(), spike.len());
    assert!(reference[2] < spike[2]);
    assert!(create_reference_data(&vec![5.0; 16])
        .iter()
        .all(|&v| (v - 5.0).abs() < 1e-6));
}

#[cfg(target_os = "macos")]
#[test]
fn metal_buffer_extraction_handles_formats() {
    use half::f16;
    use metal::{Device, MTLResourceOptions};

    let device = Device::system_default().expect("Metal device required for test");
    let f32_values = vec![0.1_f32, 1.2, -3.4, 9.0];
    let f32_bytes = bytemuck::cast_slice(&f32_values);
    let buffer32 = device.new_buffer(
        f32_bytes.len() as u64,
        MTLResourceOptions::StorageModeShared,
    );
    unsafe {
        std::ptr::copy_nonoverlapping(
            f32_bytes.as_ptr(),
            buffer32.contents() as *mut u8,
            f32_bytes.len(),
        );
    }
    assert_eq!(
        extract_buffer_data(&buffer32, f32_values.len()).unwrap(),
        f32_values
    );

    let half_values: Vec<f16> = f32_values.iter().map(|&v| f16::from_f32(v)).collect();
    let half_bytes: Vec<u8> = half_values
        .iter()
        .flat_map(|&v| v.to_bits().to_le_bytes())
        .collect();
    let buffer16 = device.new_buffer(
        half_bytes.len() as u64,
        MTLResourceOptions::StorageModeShared,
    );
    unsafe {
        std::ptr::copy_nonoverlapping(
            half_bytes.as_ptr(),
            buffer16.contents() as *mut u8,
            half_bytes.len(),
        );
    }
    let extracted = extract_buffer_data(&buffer16, half_values.len()).unwrap();
    for (expected, actual) in f32_values.iter().zip(extracted.iter()) {
        assert!((expected - actual).abs() < 1e-3);
    }
}

#[test]
fn adapter_prefixed_keys_and_events() {
    use adapteros_telemetry::{event::KernelNoiseEvent, TelemetryWriter};
    use std::{fs, thread, time::Duration};

    // Prepare telemetry directory
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "aos_noise_tracker_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();

    // Writer with max_events=1 to force immediate rotation/flush
    let writer = TelemetryWriter::new(&dir, 1, 1024 * 1024).unwrap();
    let telemetry = std::sync::Arc::new(writer);

    let mut tracker = NoiseTracker::new(NoiseTrackingConfig::default(), Some(telemetry));
    tracker.set_adapter_prefix("adp-1");

    // Track small error for a layer
    let quant = [1.0f32, 2.0, 3.0, 4.0];
    let reference = [1.0f32, 2.0, 3.000001, 3.999999];
    tracker
        .track_layer_error("ff.gate", &quant, Some(&reference))
        .unwrap();

    // Verify the prefixed key is stored in layer_stats
    assert!(tracker.get_layer_stats("adapter:adp-1/ff.gate").is_some());

    // Give some time for async writer thread to flush the event
    thread::sleep(Duration::from_millis(400));

    // Read the first bundle file
    let bundle_path = dir.join("bundle_000000.ndjson");
    assert!(
        bundle_path.exists(),
        "bundle file not created: {:?}",
        bundle_path
    );
    let content = fs::read_to_string(&bundle_path).unwrap();

    // Parse unified telemetry event and extract KernelNoiseEvent payload
    let mut found = false;
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let ev: adapteros_telemetry::unified_events::TelemetryEvent =
            serde_json::from_str(line).unwrap();
        if ev.event_type == "kernel.noise" {
            if let Some(meta) = ev.metadata {
                let noise: KernelNoiseEvent = serde_json::from_value(meta).unwrap();
                assert_eq!(noise.layer_id, "adapter:adp-1/ff.gate");
                found = true;
                break;
            }
        }
    }

    assert!(
        found,
        "kernel.noise event with adapter-prefixed key not found"
    );
}
