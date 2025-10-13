//! Kernel profiling tests
//!
//! Validates that performance counter profiling emits correct event structures
//! with available=false when counters aren't supported.

#[cfg(target_os = "macos")]
use metal::Device;

#[cfg(target_os = "macos")]
use mplora_kernel_prof::MetalProfiler;

#[test]
#[cfg(target_os = "macos")]
fn test_profiler_creation() {
    let device = Device::system_default().unwrap();
    let profiler = MetalProfiler::new(&device);
    
    assert!(!profiler.device_name().is_empty());
    println!("Created profiler for device: {}", profiler.device_name());
}

#[test]
#[cfg(target_os = "macos")]
fn test_profile_event_structure() {
    let device = Device::system_default().unwrap();
    let profiler = MetalProfiler::new(&device);
    
    let queue = device.new_command_queue();
    let command_buffer = queue.new_command_buffer();
    
    let profile = profiler.profile_dispatch("test_kernel", &command_buffer).unwrap();
    
    // Verify event structure
    assert_eq!(profile.kernel, "test_kernel");
    assert_eq!(profile.device, profiler.device_name());
    
    // Counters should be present (even if zero)
    assert_eq!(profile.counters.threads, 0);
    assert_eq!(profile.counters.occupancy, 0);
    assert_eq!(profile.counters.mem_read, 0);
    assert_eq!(profile.counters.mem_write, 0);
    
    println!("Profile available: {}", profile.available);
}

#[test]
#[cfg(target_os = "macos")]
fn test_profile_serialization() {
    let device = Device::system_default().unwrap();
    let profiler = MetalProfiler::new(&device);
    
    let queue = device.new_command_queue();
    let command_buffer = queue.new_command_buffer();
    
    let profile = profiler.profile_dispatch("fused_mlp", &command_buffer).unwrap();
    
    // Serialize to JSON
    let json = serde_json::to_string(&profile).unwrap();
    
    // Verify JSON contains required fields
    assert!(json.contains("\"device\""));
    assert!(json.contains("\"kernel\":\"fused_mlp\""));
    assert!(json.contains("\"available\""));
    assert!(json.contains("\"counters\""));
    
    println!("Serialized profile: {}", json);
}

#[test]
#[cfg(target_os = "macos")]
fn test_unavailable_counters_emit_zeros() {
    let device = Device::system_default().unwrap();
    let profiler = MetalProfiler::new(&device);
    
    let queue = device.new_command_queue();
    let command_buffer = queue.new_command_buffer();
    
    let profile = profiler.profile_dispatch("test", &command_buffer).unwrap();
    
    // If counters unavailable, should still have valid structure
    if !profile.available {
        assert_eq!(profile.counters.threads, 0);
        assert_eq!(profile.counters.occupancy, 0);
        assert_eq!(profile.counters.mem_read, 0);
        assert_eq!(profile.counters.mem_write, 0);
        println!("✓ Unavailable counters correctly return zeros");
    } else {
        println!("✓ Counters available on this device");
    }
}

#[test]
#[cfg(target_os = "macos")]
fn test_multiple_kernel_profiles() {
    let device = Device::system_default().unwrap();
    let profiler = MetalProfiler::new(&device);
    
    let queue = device.new_command_queue();
    
    let kernels = vec!["fused_mlp", "fused_qkv", "flash_attention"];
    
    for kernel_name in kernels {
        let command_buffer = queue.new_command_buffer();
        let profile = profiler.profile_dispatch(kernel_name, &command_buffer).unwrap();
        
        assert_eq!(profile.kernel, kernel_name);
        println!("✓ Profiled {}: available={}", kernel_name, profile.available);
    }
}

#[test]
#[cfg(not(target_os = "macos"))]
fn test_profiler_not_available() {
    // On non-macOS platforms, profiling is not available
    println!("Kernel profiling tests require macOS with Metal support");
}
