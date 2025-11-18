//! Ring buffer for top-K adapter management
//!
//! This module implements a ring buffer for managing active adapters
//! with Q15 quantized gates for efficient Metal kernel dispatch.
//!
//! References:
//! - Ring Buffer Data Structure: https://en.wikipedia.org/wiki/Circular_buffer
//! - Metal Buffer Management: https://developer.apple.com/documentation/metal/mtlbuffer

use adapteros_core::{AosError, Result};
use metal::*;
use std::sync::Arc;

/// Active adapter with quantized gate (Q15 signed format)
#[derive(Debug, Clone)]
pub struct ActiveAdapter {
    /// Adapter ID (u16 matches canonical RouterRing)
    pub id: u16,
    /// Gate value in Q15 signed format (i16: -32767 to +32767)
    pub gate: i16,
}

/// Ring buffer for router decisions (canonical format)
///
/// **CRITICAL**: This structure must match adapteros-lora-kernel-api::RouterRing
/// - Adapter indices: u16 (not u32, saves 2x VRAM)
/// - Gates: i16 signed Q15 (not u16 unsigned, preserves sign bit)
pub struct RingBuffer {
    /// Maximum number of active adapters (top-K)
    top_k: usize,
    /// Current position in the ring
    current_pos: usize,
    /// Adapter indices (u16 matches RouterRing canonical format)
    adapter_indices: Vec<u16>,
    /// Q15 quantized gates (i16 signed matches RouterRing)
    gates: Vec<i16>,
    /// Metal buffer for GPU access
    buffer: Option<Buffer>,
    /// Device reference
    _device: Arc<Device>,
}

impl RingBuffer {
    /// Create a new ring buffer (canonical format: u16 indices, i16 gates)
    pub fn new(device: Arc<Device>, top_k: usize) -> Result<Self> {
        if top_k > 8 {
            return Err(AosError::Kernel(
                "Ring buffer supports maximum 8 adapters".to_string(),
            ));
        }

        // Buffer layout (canonical format):
        //   4 bytes: top_k (u32)
        //   4 bytes: current_pos (u32)
        //  16 bytes: adapter_indices[8] (u16 * 8) - changed from u32 to save VRAM
        //  16 bytes: gates[8] (i16 * 8) - signed Q15 format
        // Total: 40 bytes (was 56 bytes with u32 indices)
        let buffer_size = std::mem::size_of::<u32>() * 2  // top_k + current_pos
            + std::mem::size_of::<u16>() * 8  // indices (u16)
            + std::mem::size_of::<i16>() * 8; // gates (i16 signed Q15)
        let buffer = device.new_buffer(buffer_size as u64, MTLResourceOptions::StorageModeShared);

        Ok(Self {
            top_k,
            current_pos: 0,
            adapter_indices: vec![0; 8],
            gates: vec![0; 8],
            buffer: Some(buffer),
            _device: device,
        })
    }

    /// Update the ring buffer with active adapters
    pub fn update(&mut self, adapters: &[ActiveAdapter]) -> Result<()> {
        if adapters.len() > self.top_k {
            return Err(AosError::Kernel(
                "Too many adapters for ring buffer".to_string(),
            ));
        }

        // Clear existing data
        self.adapter_indices.fill(0);
        self.gates.fill(0);

        // Set active adapters
        for (i, adapter) in adapters.iter().enumerate() {
            self.adapter_indices[i] = adapter.id;
            self.gates[i] = adapter.gate;
        }

        // Update Metal buffer
        self.update_metal_buffer()?;

        self.current_pos = (self.current_pos + 1) % self.top_k;
        Ok(())
    }

    /// Update the Metal buffer with current data
    fn update_metal_buffer(&self) -> Result<()> {
        let buffer = self
            .buffer
            .as_ref()
            .ok_or_else(|| AosError::Kernel("Buffer not initialized".to_string()))?;

        let contents = buffer.contents();
        let slice = unsafe {
            std::slice::from_raw_parts_mut(contents as *mut u8, buffer.length() as usize)
        };

        let mut offset = 0;

        // IMPORTANT: This layout must match the Metal struct RingBuffer in common.metal:
        // struct RingBuffer {
        //     uint top_k;          // 4 bytes (u32)
        //     uint current_pos;    // 4 bytes (u32)
        //     ushort adapter_indices[8];  // 8 * 2 bytes = 16 bytes (u16)
        //     short gates[8];      // 8 * 2 bytes = 16 bytes (i16 signed Q15)
        // };
        //
        // **NOTE**: Canonical format uses u16 indices (not uint) and i16 gates (not ushort)
        // This saves 16 bytes VRAM per ring and preserves signed Q15 gates

        // Write top_k (4 bytes)
        slice[offset..offset + 4].copy_from_slice(&(self.top_k as u32).to_le_bytes());
        offset += 4;

        // Write current_pos (4 bytes)
        slice[offset..offset + 4].copy_from_slice(&(self.current_pos as u32).to_le_bytes());
        offset += 4;

        // Write adapter indices (8 * 2 bytes = 16 bytes, u16 canonical format)
        for &idx in &self.adapter_indices {
            slice[offset..offset + 2].copy_from_slice(&idx.to_le_bytes());
            offset += 2;
        }

        // Write gates (8 * 2 bytes = 16 bytes, i16 signed Q15)
        for &gate in &self.gates {
            slice[offset..offset + 2].copy_from_slice(&gate.to_le_bytes());
            offset += 2;
        }

        Ok(())
    }

    /// Get the Metal buffer for kernel dispatch
    pub fn get_buffer(&self) -> Option<&Buffer> {
        self.buffer.as_ref()
    }

    /// Get active adapters
    pub fn get_active_adapters(&self) -> Vec<ActiveAdapter> {
        let mut adapters = Vec::new();

        for i in 0..self.top_k {
            if self.adapter_indices[i] != 0 {
                adapters.push(ActiveAdapter {
                    id: self.adapter_indices[i],
                    gate: self.gates[i],
                });
            }
        }

        adapters
    }

    /// Convert float gate to signed Q15 format (matches router canonical format)
    ///
    /// **CRITICAL**: Uses denominator 32767.0 (not 32768.0) to match router Decision
    /// Reference: adapteros-lora-router/src/lib.rs:1030 (Decision::gates_f32)
    pub fn float_to_q15(gate: f32) -> i16 {
        (gate.clamp(-1.0, 1.0) * 32767.0) as i16
    }

    /// Convert signed Q15 gate to float (matches router canonical format)
    pub fn q15_to_float(gate: i16) -> f32 {
        gate as f32 / 32767.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_buffer_creation() {
        let device = Device::system_default().expect("Metal device should be available for test");
        let ring_buffer =
            RingBuffer::new(Arc::new(device), 3).expect("Ring buffer creation should succeed");
        assert_eq!(ring_buffer.top_k, 3);
    }

    #[test]
    fn test_ring_buffer_update() {
        let device = Device::system_default().expect("Metal device should be available for test");
        let mut ring_buffer =
            RingBuffer::new(Arc::new(device), 3).expect("Ring buffer creation should succeed");

        let adapters = vec![
            ActiveAdapter { id: 1, gate: 16383 }, // ~0.5 in signed Q15
            ActiveAdapter { id: 2, gate: 32767 }, // 1.0 in signed Q15
        ];

        ring_buffer
            .update(&adapters)
            .expect("Ring buffer update should succeed");
        let active = ring_buffer.get_active_adapters();
        assert_eq!(active.len(), 2);
        assert_eq!(active[0].id, 1);
        assert_eq!(active[1].id, 2);
    }

    #[test]
    fn test_q15_conversion() {
        // Signed Q15 format: i16 range -32767 to +32767, denominator 32767.0
        assert_eq!(RingBuffer::float_to_q15(0.0), 0);
        assert_eq!(RingBuffer::float_to_q15(0.5), 16383);  // 0.5 * 32767 = 16383
        assert_eq!(RingBuffer::float_to_q15(1.0), 32767);  // 1.0 * 32767 = 32767
        assert_eq!(RingBuffer::float_to_q15(-1.0), -32767); // Negative gates supported

        assert_eq!(RingBuffer::q15_to_float(0), 0.0);
        assert_eq!(RingBuffer::q15_to_float(16383), 16383.0 / 32767.0);
        assert_eq!(RingBuffer::q15_to_float(32767), 1.0);
        assert_eq!(RingBuffer::q15_to_float(-32767), -1.0);
    }
}
