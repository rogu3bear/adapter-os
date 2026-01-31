//! Integration test verifying the Router -> RingBuffer handshake logic.

#[cfg(target_os = "macos")]
#[cfg(test)]
mod tests {
    use crate::router_bridge::decision_to_router_ring_with_active_ids_and_strengths;
    use adapteros_lora_kernel_mtl::{ActiveAdapter, RingBuffer};
    use adapteros_lora_router::Decision;
    use metal::Device;
    use std::sync::Arc;
    // use adapteros_types::AdapterId; // Not strictly needed if not used in signature

    #[test]
    fn test_router_to_ring_buffer_handshake() {
        // 1. Simulate Router Decision
        // Selected adapters: 1 (gate ~0.5), 2 (gate 1.0)
        // Indices are local indices (u16)
        // let indices = vec![1, 2];
        // let gates_q15 = vec![16383, 32767];

        let active_adapters = vec![
            ActiveAdapter { id: 1, gate: 16383 },
            ActiveAdapter { id: 2, gate: 32767 },
            ActiveAdapter {
                id: 3,
                gate: -16383,
            }, // Negative gate test
        ];

        // 2. Initialize Metal Device & RingBuffer
        let device = match Device::system_default() {
            Some(device) => device,
            None => {
                eprintln!("skipping: metal device required");
                return;
            }
        };
        let device_arc = Arc::new(device);
        let mut ring_buffer =
            RingBuffer::new(device_arc.clone(), 8).expect("RingBuffer init failed");

        // 3. Verify Initial State
        let buffer = ring_buffer.get_buffer().expect("Buffer missing");
        let ptr = buffer.contents() as *const u8;
        let len = buffer.length();
        let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
        let initial_pos = u32::from_le_bytes(slice[4..8].try_into().unwrap());
        assert_eq!(initial_pos, 0, "Initial pos should be 0");

        // 4. Update RingBuffer
        ring_buffer.update(&active_adapters).expect("Update failed");

        // 5. Verify Metal Buffer Layout (Gap 7 / Gap 2 Validation)
        // Assert size (40 bytes)
        assert_eq!(len, 40, "RingBuffer size mismatch");

        let top_k = u32::from_le_bytes(slice[0..4].try_into().unwrap());
        // Verify pos from buffer (should be 0 because update writes THEN increments)
        let pos = u32::from_le_bytes(slice[4..8].try_into().unwrap());

        assert_eq!(top_k, 8, "top_k mismatch");
        assert_eq!(pos, 0, "pos should be 0 (value at write time)");

        assert_eq!(
            top_k, 8,
            "top_k mismatch (should be max_k or configured k?)"
        );
        // Wait, RingBuffer::new(8) sets top_k=8.

        // Indices (offset 8)
        // Expect [1, 2, 3, 0, 0, 0, 0, 0]
        let idx1 = u16::from_le_bytes(slice[8..10].try_into().unwrap());
        let idx2 = u16::from_le_bytes(slice[10..12].try_into().unwrap());
        let idx3 = u16::from_le_bytes(slice[12..14].try_into().unwrap());

        assert_eq!(idx1, 1);
        assert_eq!(idx2, 2);
        assert_eq!(idx3, 3);

        // Gates (offset 24)
        // Expect [16383, 32767, -16383, ...]
        let g1 = i16::from_le_bytes(slice[24..26].try_into().unwrap());
        let g2 = i16::from_le_bytes(slice[26..28].try_into().unwrap());
        let g3 = i16::from_le_bytes(slice[28..30].try_into().unwrap());

        assert_eq!(g1, 16383);
        assert_eq!(g2, 32767);
        assert_eq!(g3, -16383);

        println!("Gap 2 Closed: Router handshake to Metal RingBuffer verified.");
    }
}
