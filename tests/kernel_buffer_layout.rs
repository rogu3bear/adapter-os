//! Kernel Buffer Layout Tests
//!
//! Tests to verify that Rust buffer layouts match Metal shader struct expectations.
//! These tests validate memory layout correctness without requiring Metal runtime.

use adapteros_lora_kernel_mtl::ring_buffer::{ActiveAdapter, RingBuffer};
use adapteros_core::Result;

/// Test that RingBuffer memory layout matches Metal struct definition
///
/// Metal struct (from common.metal):
/// ```metal
/// struct RingBuffer {
///     uint top_k;                 // 4 bytes at offset 0
///     uint current_pos;           // 4 bytes at offset 4
///     uint adapter_indices[8];    // 32 bytes at offset 8
///     uint16_t gates[8];         // 16 bytes at offset 40
/// };
/// ```
///
/// Total size: 56 bytes
#[test]
fn test_ring_buffer_memory_layout() -> Result<()> {
    use metal::Device;
    use std::sync::Arc;

    // Create device and ring buffer
    let device = Arc::new(Device::system_default().expect("No Metal device found"));
    let mut ring_buffer = RingBuffer::new(device.clone(), 3)?;

    // Set known values
    let adapters = vec![
        ActiveAdapter { id: 100, gate: 16384 },  // 50% strength
        ActiveAdapter { id: 200, gate: 32767 },  // 100% strength
        ActiveAdapter { id: 300, gate: 8192 },   // 25% strength
    ];

    ring_buffer.update(&adapters)?;

    // Get raw buffer contents
    let buffer = ring_buffer.get_buffer().expect("Buffer not initialized");
    let contents = buffer.contents();
    let data = unsafe {
        std::slice::from_raw_parts(contents as *const u8, buffer.length() as usize)
    };

    // Verify buffer size (should be 56 bytes minimum)
    assert!(data.len() >= 56, "Buffer too small: {} bytes", data.len());

    // Verify top_k at offset 0 (4 bytes)
    let top_k = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    assert_eq!(top_k, 3, "top_k should be 3, got {}", top_k);

    // Verify current_pos at offset 4 (4 bytes)
    let current_pos = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    // current_pos increments after each update, so just verify it's valid
    assert!(current_pos < 8, "current_pos out of range: {}", current_pos);

    // Verify adapter_indices at offset 8 (8 × 4 = 32 bytes)
    let adapter_0 = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
    let adapter_1 = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
    let adapter_2 = u32::from_le_bytes([data[16], data[17], data[18], data[19]]);

    assert_eq!(adapter_0, 100, "adapter_indices[0] should be 100, got {}", adapter_0);
    assert_eq!(adapter_1, 200, "adapter_indices[1] should be 200, got {}", adapter_1);
    assert_eq!(adapter_2, 300, "adapter_indices[2] should be 300, got {}", adapter_2);

    // Remaining adapter_indices should be 0
    for i in 3..8 {
        let offset = 8 + i * 4;
        let adapter_id = u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]);
        assert_eq!(adapter_id, 0, "adapter_indices[{}] should be 0, got {}", i, adapter_id);
    }

    // Verify gates at offset 40 (8 × 2 = 16 bytes)
    let gate_0 = u16::from_le_bytes([data[40], data[41]]);
    let gate_1 = u16::from_le_bytes([data[42], data[43]]);
    let gate_2 = u16::from_le_bytes([data[44], data[45]]);

    assert_eq!(gate_0, 16384, "gates[0] should be 16384 (50%), got {}", gate_0);
    assert_eq!(gate_1, 32767, "gates[1] should be 32767 (100%), got {}", gate_1);
    assert_eq!(gate_2, 8192, "gates[2] should be 8192 (25%), got {}", gate_2);

    // Remaining gates should be 0
    for i in 3..8 {
        let offset = 40 + i * 2;
        let gate = u16::from_le_bytes([data[offset], data[offset + 1]]);
        assert_eq!(gate, 0, "gates[{}] should be 0, got {}", i, gate);
    }

    println!("✅ RingBuffer memory layout verified:");
    println!("  - top_k: {} (offset 0)", top_k);
    println!("  - current_pos: {} (offset 4)", current_pos);
    println!("  - adapter_indices: [100, 200, 300, 0, 0, 0, 0, 0] (offset 8)");
    println!("  - gates (Q15): [16384, 32767, 8192, 0, 0, 0, 0, 0] (offset 40)");
    println!("  - Total size: {} bytes", data.len());

    Ok(())
}

/// Test Q15 gate conversion
///
/// Q15 format: signed 16-bit fixed-point with 15 fractional bits
/// Range: -32768 to 32767 maps to -1.0 to 1.0
#[test]
fn test_q15_gate_conversion() {
    use adapteros_lora_kernel_mtl::ring_buffer::RingBuffer;

    // Test conversion from float to Q15
    assert_eq!(RingBuffer::float_to_q15(0.0), 0);
    assert_eq!(RingBuffer::float_to_q15(0.5), 16384);
    assert_eq!(RingBuffer::float_to_q15(1.0), 32768);

    // Test clamping
    assert_eq!(RingBuffer::float_to_q15(1.5), 32768); // Clamps to 1.0
    assert_eq!(RingBuffer::float_to_q15(-0.5), 0);    // Clamps to 0.0

    // Test edge cases
    assert_eq!(RingBuffer::float_to_q15(0.25), 8192);
    assert_eq!(RingBuffer::float_to_q15(0.75), 24576);

    println!("✅ Q15 gate conversion verified");
}

/// Test RingBuffer update with different adapter counts
#[test]
fn test_ring_buffer_update_various_counts() -> Result<()> {
    use metal::Device;
    use std::sync::Arc;

    let device = Arc::new(Device::system_default().expect("No Metal device found"));

    // Test K=1
    {
        let mut ring_buffer = RingBuffer::new(device.clone(), 1)?;
        let adapters = vec![ActiveAdapter { id: 42, gate: 32767 }];
        ring_buffer.update(&adapters)?;

        let buffer = ring_buffer.get_buffer().expect("Buffer not initialized");
        let contents = buffer.contents();
        let data = unsafe {
            std::slice::from_raw_parts(contents as *const u8, buffer.length() as usize)
        };

        let top_k = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        assert_eq!(top_k, 1);

        let adapter_0 = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
        assert_eq!(adapter_0, 42);

        println!("✅ K=1 test passed");
    }

    // Test K=8 (maximum)
    {
        let mut ring_buffer = RingBuffer::new(device.clone(), 8)?;
        let adapters: Vec<ActiveAdapter> = (0..8)
            .map(|i| ActiveAdapter {
                id: i * 10,
                gate: (i as u16 + 1) * 4096,
            })
            .collect();

        ring_buffer.update(&adapters)?;

        let buffer = ring_buffer.get_buffer().expect("Buffer not initialized");
        let contents = buffer.contents();
        let data = unsafe {
            std::slice::from_raw_parts(contents as *const u8, buffer.length() as usize)
        };

        let top_k = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        assert_eq!(top_k, 8);

        // Verify all 8 adapters
        for i in 0..8 {
            let offset = 8 + i * 4;
            let adapter_id = u32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]);
            assert_eq!(adapter_id, i as u32 * 10);

            let gate_offset = 40 + i * 2;
            let gate = u16::from_le_bytes([data[gate_offset], data[gate_offset + 1]]);
            assert_eq!(gate, (i as u16 + 1) * 4096);
        }

        println!("✅ K=8 test passed");
    }

    Ok(())
}

/// Test that RingBuffer rejects invalid inputs
#[test]
fn test_ring_buffer_validation() -> Result<()> {
    use metal::Device;
    use std::sync::Arc;

    let device = Arc::new(Device::system_default().expect("No Metal device found"));

    // Test exceeding top_k limit
    {
        let mut ring_buffer = RingBuffer::new(device.clone(), 3)?;
        let adapters = vec![
            ActiveAdapter { id: 1, gate: 10000 },
            ActiveAdapter { id: 2, gate: 20000 },
            ActiveAdapter { id: 3, gate: 30000 },
            ActiveAdapter { id: 4, gate: 40000 }, // One too many
        ];

        let result = ring_buffer.update(&adapters);
        assert!(result.is_err(), "Should reject more adapters than top_k");
        println!("✅ Correctly rejected too many adapters");
    }

    // Test exceeding maximum K=8
    {
        let result = RingBuffer::new(device.clone(), 9);
        assert!(result.is_err(), "Should reject top_k > 8");
        println!("✅ Correctly rejected K > 8");
    }

    Ok(())
}
