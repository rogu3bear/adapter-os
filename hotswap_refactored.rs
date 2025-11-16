    // ============================================================================
    // Helper methods for safetensors metadata parsing and validation
    // ============================================================================

    /// Extract metadata from safetensors
    fn parse_safetensors_metadata(tensors: &SafeTensors) -> Result<(u32, f32)> {
        // Try to parse metadata JSON if present
        if let Ok(metadata_bytes) = tensors.metadata() {
            if let Ok(metadata_str) = std::str::from_utf8(metadata_bytes) {
                if let Ok(metadata) = serde_json::from_str::<serde_json::Value>(metadata_str) {
                    let rank = metadata
                        .get("rank")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse::<u32>().ok())
                        .or_else(|| {
                            metadata
                                .get("r")
                                .and_then(|v| v.as_str())
                                .and_then(|s| s.parse::<u32>().ok())
                        });

                    let alpha = metadata
                        .get("lora_alpha")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse::<f32>().ok())
                        .or_else(|| {
                            metadata
                                .get("alpha")
                                .and_then(|v| v.as_str())
                                .and_then(|s| s.parse::<f32>().ok())
                        });

                    if let (Some(r), Some(a)) = (rank, alpha) {
                        return Ok((r, a));
                    }
                }
            }
        }

        // Fallback: infer from tensor shapes
        Err(AosError::Kernel(
            "No metadata found, will infer from tensor shapes".to_string(),
        ))
    }

    /// Validate tensor shape compatibility for LoRA
    fn validate_lora_shapes(
        a_shape: &[usize],
        b_shape: &[usize],
        rank: usize,
        module: &str,
    ) -> Result<()> {
        if a_shape.len() != 2 || b_shape.len() != 2 {
            return Err(AosError::Kernel(format!(
                "Module {} has invalid tensor ranks: lora_a {:?}, lora_b {:?}",
                module, a_shape, b_shape
            )));
        }

        let a_rank = a_shape[0];
        let b_rank = b_shape[1];

        // Both should match the declared rank (with some tolerance for padding)
        if a_rank != rank && a_rank != rank.div_ceil(16) * 16 {
            return Err(AosError::Kernel(format!(
                "Module {} lora_a has rank {} but expected {} (or padded)",
                module, a_rank, rank
            )));
        }

        if b_rank != rank && b_rank != rank.div_ceil(16) * 16 {
            return Err(AosError::Kernel(format!(
                "Module {} lora_b has rank {} but expected {} (or padded)",
                module, b_rank, rank
            )));
        }

        Ok(())
    }

    /// Load adapter at runtime (hot-swap)
    ///
    /// This method enables deterministic hot-swapping of LoRA adapter weights
    /// at runtime without requiring a process restart. The weights are provided
    /// as safetensors format bytes and are loaded into Metal GPU buffers.
    ///
    /// # Thread Safety
    ///
    /// This operation is synchronized with the Metal command queue to ensure
    /// no inference operations are using the old weights during the swap.
    /// The deterministic executor guarantees serialization of operations.
    ///
    /// # Arguments
    ///
    /// * `id` - Adapter slot ID (0-based index into adapter_index_map)
    /// * `weights` - Safetensors-formatted weight data
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Weights cannot be parsed as safetensors
    /// - Required tensors are missing
    /// - Tensor dimensions don't match expected shapes
    /// - Metal buffer allocation fails
    /// - Adapter ID exceeds maximum slots (256)
    fn load_adapter(&mut self, id: u16, weights: &[u8]) -> Result<()> {
        tracing::info!(
            adapter_id = id,
            weight_bytes = weights.len(),
            "Loading adapter at runtime"
        );

        // Bounds checking
        const MAX_ADAPTERS: u16 = 256;
        if id >= MAX_ADAPTERS {
            return Err(AosError::Kernel(format!(
                "Adapter ID {} exceeds maximum {} slots",
                id, MAX_ADAPTERS
            )));
        }

        let adapter_index = id as usize;
        let adapter_id = format!("runtime_adapter_{}", id);

        // Check for duplicate load
        if adapter_index < self.adapter_index_map.len()
            && !self.adapter_index_map[adapter_index].is_empty()
        {
            tracing::warn!(
                adapter_id = id,
                existing = %self.adapter_index_map[adapter_index],
                "Overwriting existing adapter at slot"
            );
        }

        // Parse safetensors data
        let tensors = SafeTensors::deserialize(weights).map_err(|err| {
            AosError::Kernel(format!(
                "Failed to deserialize safetensors for adapter {}: {}",
                id, err
            ))
        })?;

        if tensors.names().next().is_none() {
            return Err(AosError::Kernel(format!(
                "No tensors found in adapter {}",
                id
            )));
        }

        // Extract metadata (rank and alpha)
        let (rank, alpha) = Self::parse_safetensors_metadata(&tensors).unwrap_or_else(|_| {
            // Fallback: infer rank from first tensor
            let first_name = tensors.names().next().unwrap();
            let first_tensor = tensors.tensor(first_name).unwrap();
            let shape = first_tensor.shape();

            let inferred_rank = if first_name.contains("lora_a") {
                shape[0] as u32
            } else if first_name.contains("lora_b") {
                shape[1] as u32
            } else {
                8 // Conservative default
            };

            tracing::warn!(
                adapter_id = id,
                inferred_rank,
                "No metadata found, inferred rank from tensor shapes"
            );

            (inferred_rank, 16.0)
        });

        let rank_usize = rank as usize;
        let rank_padded = rank_usize.div_ceil(16) * 16;

        // Extract target modules from tensor names
        let mut target_modules = HashSet::new();
        for name in tensors.names() {
            if let Some(module_name) = name.strip_prefix("lora_a.") {
                target_modules.insert(module_name.to_string());
            } else if let Some(module_name) = name.strip_prefix("lora_b.") {
                target_modules.insert(module_name.to_string());
            }
        }

        if target_modules.is_empty() {
            return Err(AosError::Kernel(format!(
                "No target modules found in adapter {} tensors",
                id
            )));
        }

        // Validate all modules before creating any buffers (atomic check)
        for module in &target_modules {
            let a_key = format!("lora_a.{}", module);
            let b_key = format!("lora_b.{}", module);

            let a_tensor = tensors.tensor(&a_key).map_err(|err| {
                AosError::Kernel(format!("Adapter {} missing tensor {}: {}", id, a_key, err))
            })?;

            let b_tensor = tensors.tensor(&b_key).map_err(|err| {
                AosError::Kernel(format!("Adapter {} missing tensor {}: {}", id, b_key, err))
            })?;

            Self::validate_lora_shapes(a_tensor.shape(), b_tensor.shape(), rank_usize, module)?;
        }

        // Create AdapterWeights with Metal buffers (atomic operation)
        let mut lora_a_buffers = HashMap::new();
        let mut lora_b_buffers = HashMap::new();
        let mut module_shapes = HashMap::new();
        let mut total_bytes = 0u64;

        // Store cleanup data in case of failure
        let mut allocated_buffers: Vec<(String, Buffer, Buffer)> = Vec::new();

        for module in &target_modules {
            let a_key = format!("lora_a.{}", module);
            let b_key = format!("lora_b.{}", module);

            let a_tensor = tensors.tensor(&a_key).unwrap();
            let b_tensor = tensors.tensor(&b_key).unwrap();

            let a_shape = a_tensor.shape();
            let b_shape = b_tensor.shape();

            let a_rows = a_shape[0];
            let a_cols = a_shape[1];
            let b_rows = b_shape[0];
            let b_cols = b_shape[1];

            // Convert tensors to f32 vectors
            let a_values = match Self::tensor_to_f32_vec(&a_tensor, &a_key) {
                Ok(v) => v,
                Err(e) => {
                    // Cleanup already allocated buffers
                    drop(allocated_buffers);
                    return Err(e);
                }
            };

            let b_values = match Self::tensor_to_f32_vec(&b_tensor, &b_key) {
                Ok(v) => v,
                Err(e) => {
                    drop(allocated_buffers);
                    return Err(e);
                }
            };

            // Pad to align with 16-byte boundaries
            let mut padded_a = vec![0f32; rank_padded * a_cols];
            let copy_rows = usize::min(rank_usize, a_rows);
            for r in 0..copy_rows {
                let src = r * a_cols;
                let dst = r * a_cols;
                padded_a[dst..dst + a_cols].copy_from_slice(&a_values[src..src + a_cols]);
            }

            let mut padded_b = vec![0f32; b_rows * rank_padded];
            let copy_cols = usize::min(rank_usize, b_cols);
            for row in 0..b_rows {
                let src = row * b_cols;
                let dst = row * rank_padded;
                padded_b[dst..dst + copy_cols].copy_from_slice(&b_values[src..src + copy_cols]);
            }

            // Create Metal buffers
            let a_buffer = self.device.new_buffer_with_data(
                padded_a.as_ptr() as *const c_void,
                (padded_a.len() * std::mem::size_of::<f32>()) as u64,
                MTLResourceOptions::StorageModeShared,
            );

            let b_buffer = self.device.new_buffer_with_data(
                padded_b.as_ptr() as *const c_void,
                (padded_b.len() * std::mem::size_of::<f32>()) as u64,
                MTLResourceOptions::StorageModeShared,
            );

            total_bytes += a_buffer.length();
            total_bytes += b_buffer.length();

            // Store for cleanup in case of later failure
            allocated_buffers.push((module.clone(), a_buffer.clone(), b_buffer.clone()));

            lora_a_buffers.insert(module.clone(), a_buffer);
            lora_b_buffers.insert(module.clone(), b_buffer);
            module_shapes.insert(
                module.clone(),
                ModuleShape {
                    out_dim: b_rows,
                    in_dim: a_cols,
                },
            );
        }

        let adapter_weights = AdapterWeights {
            adapter_id: adapter_id.clone(),
            rank,
            rank_padded: rank_padded as u32,
            alpha,
            lora_a_buffers,
            lora_b_buffers,
            module_shapes,
            total_bytes,
        };

        // Wait for any in-flight GPU operations to complete before modifying adapter state
        // This ensures thread-safety by synchronizing with the Metal command queue
        let sync_buffer = self._queue.new_command_buffer();
        sync_buffer.commit();
        sync_buffer.wait_until_completed();

        // Update adapter index map if needed
        if adapter_index >= self.adapter_index_map.len() {
            self.adapter_index_map
                .resize(adapter_index + 1, String::new());
        }
        self.adapter_index_map[adapter_index] = adapter_id.clone();

        // Store adapter weights
        self.adapter_weights
            .insert(adapter_id.clone(), adapter_weights);

        // Track VRAM usage
        self.vram_tracker.track_adapter_load(
            &adapter_id,
            total_bytes / (1024 * 1024), // Convert to MB
        );

        // If LoRA buffers are already allocated, copy weights immediately
        if self.lora_buffers.is_some() && self.embedding_dimensions.is_some() {
            let hidden_size = self.embedding_dimensions.as_ref().unwrap().hidden_size;

            // Calculate dimensions from transformer weights if available, otherwise error
            let (intermediate_size, kv_width) =
                if let Some(ref transformer_weights) = self.transformer_weights {
                    let gate_elements = (transformer_weights.gate_weight.length() as usize)
                        / std::mem::size_of::<f32>();
                    let intermediate_size = gate_elements / hidden_size;

                    let kv_width = self
                        .qkv_kernel
                        .as_ref()
                        .map(|k| k.gqa_config().kv_width as usize)
                        .unwrap_or(hidden_size / 8);

                    (intermediate_size, kv_width)
                } else {
                    return Err(AosError::Kernel(
                        "Transformer weights not loaded, cannot determine dimensions".to_string(),
                    ));
                };

            // Copy adapter weights to GPU buffers
            if let Some(weights) = self.adapter_weights.get(&adapter_id) {
                if let Some(buffers) = self.lora_buffers.as_ref() {
                    let copy_params = LoraCopyParamsBuilder::new()
                        .adapter_index(adapter_index)
                        .weights(weights)
                        .buffers(buffers)
                        .hidden_size(hidden_size)
                        .intermediate_size(intermediate_size)
                        .kv_width(kv_width)
                        .rank(rank_usize)
                        .build()?;

                    self.copy_lora_from_weights(copy_params)?;
                    self.populated_lora_adapters.insert(id as u32);
                }
            }
        }

        tracing::info!(
            adapter_id = id,
            adapter_name = %adapter_id,
            rank,
            alpha,
            modules = target_modules.len(),
            bytes = total_bytes,
            "Adapter loaded successfully"
        );

        Ok(())
    }

    /// Unload adapter at runtime (hot-swap)
    ///
    /// This method removes a LoRA adapter from the active set, zeroing out its
    /// GPU buffer regions and freeing associated Metal resources. This ensures
    /// clean removal without affecting other active adapters.
    ///
    /// # Thread Safety
    ///
    /// Similar to load_adapter, this operation is synchronized with the Metal
    /// command queue to prevent race conditions during weight updates.
    ///
    /// # Arguments
    ///
    /// * `id` - Adapter slot ID to unload
    ///
    /// # Errors
    ///
    /// Returns error if the adapter ID is out of range or not currently loaded.
    fn unload_adapter(&mut self, id: u16) -> Result<()> {
        tracing::info!(adapter_id = id, "Unloading adapter at runtime");

        let adapter_index = id as usize;

        // Wait for any in-flight GPU operations before modifying state
        let sync_buffer = self._queue.new_command_buffer();
        sync_buffer.commit();
        sync_buffer.wait_until_completed();

        // Get adapter name from index map
        let adapter_name = if adapter_index < self.adapter_index_map.len() {
            self.adapter_index_map[adapter_index].clone()
        } else {
            return Err(AosError::Kernel(format!(
                "Adapter ID {} out of range (max {})",
                id,
                self.adapter_index_map.len()
            )));
        };

        if adapter_name.is_empty() {
            return Err(AosError::Kernel(format!("Adapter ID {} not loaded", id)));
        }

        // Get adapter metadata before removal for VRAM tracking
        let vram_mb = self
            .adapter_weights
            .get(&adapter_name)
            .map(|w| w.total_bytes / (1024 * 1024))
            .unwrap_or(0);

        // Get actual rank from the adapter being unloaded
        let rank = self
            .adapter_weights
            .get(&adapter_name)
            .map(|w| w.rank as usize)
            .unwrap_or_else(|| {
                tracing::warn!(
                    adapter_id = id,
                    "Adapter not found in weights map, using default rank for cleanup"
                );
                8 // Fallback to conservative default
            });

        // Zero out GPU buffer regions if LoRA buffers are allocated
        if self.lora_buffers.is_some() && self.embedding_dimensions.is_some() {
            let hidden_size = self.embedding_dimensions.as_ref().unwrap().hidden_size;

            let (intermediate_size, kv_width) =
                if let Some(ref transformer_weights) = self.transformer_weights {
                    let gate_elements = (transformer_weights.gate_weight.length() as usize)
                        / std::mem::size_of::<f32>();
                    let intermediate_size = gate_elements / hidden_size;

                    let kv_width = self
                        .qkv_kernel
                        .as_ref()
                        .map(|k| k.gqa_config().kv_width as usize)
                        .unwrap_or(hidden_size / 8);

                    (intermediate_size, kv_width)
                } else {
                    return Err(AosError::Kernel(
                        "Transformer weights not loaded, cannot determine dimensions".to_string(),
                    ));
                };

            if let Some(buffers) = self.lora_buffers.as_ref() {
                let adapter_offset_hidden = adapter_index * hidden_size * rank;
                let adapter_offset_intermediate = adapter_index * intermediate_size * rank;
                let adapter_offset_hidden_rank = adapter_index * rank * hidden_size;
                let adapter_offset_intermediate_rank = adapter_index * rank * intermediate_size;
                let adapter_offset_kv_rank = adapter_index * rank * kv_width;

                // Zero out all LoRA buffer regions for this adapter
                self.zero_lora_region(
                    &buffers.gate_lora_a,
                    adapter_offset_hidden,
                    hidden_size * rank,
                );
                self.zero_lora_region(
                    &buffers.gate_lora_b,
                    adapter_offset_intermediate_rank,
                    rank * intermediate_size,
                );
                self.zero_lora_region(&buffers.up_lora_a, adapter_offset_hidden, hidden_size * rank);
                self.zero_lora_region(
                    &buffers.up_lora_b,
                    adapter_offset_intermediate_rank,
                    rank * intermediate_size,
                );
                self.zero_lora_region(
                    &buffers.down_lora_a,
                    adapter_offset_intermediate,
                    intermediate_size * rank,
                );
                self.zero_lora_region(
                    &buffers.down_lora_b,
                    adapter_offset_hidden_rank,
                    rank * hidden_size,
                );
                self.zero_lora_region(&buffers.q_lora_a, adapter_offset_hidden, hidden_size * rank);
                self.zero_lora_region(
                    &buffers.q_lora_b,
                    adapter_offset_hidden_rank,
                    rank * hidden_size,
                );
                self.zero_lora_region(&buffers.k_lora_a, adapter_offset_hidden, hidden_size * rank);
                self.zero_lora_region(&buffers.k_lora_b, adapter_offset_kv_rank, rank * kv_width);
                self.zero_lora_region(&buffers.v_lora_a, adapter_offset_hidden, hidden_size * rank);
                self.zero_lora_region(&buffers.v_lora_b, adapter_offset_kv_rank, rank * kv_width);
            }

            self.populated_lora_adapters.remove(&(id as u32));
        }

        // Remove adapter weights and clear index map entry
        self.adapter_weights.remove(&adapter_name);
        if adapter_index < self.adapter_index_map.len() {
            self.adapter_index_map[adapter_index] = String::new();
        }

        // Track VRAM release
        self.vram_tracker
            .track_adapter_unload(&adapter_name, vram_mb);

        tracing::info!(
            adapter_id = id,
            adapter_name = %adapter_name,
            rank,
            vram_mb,
            "Adapter unloaded successfully"
        );

        Ok(())
    }
