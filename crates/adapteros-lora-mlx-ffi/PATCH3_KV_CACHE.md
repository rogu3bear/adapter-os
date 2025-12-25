# Patch 3: KV Cache Integration for model.rs

This patch adds KV caching support to enable efficient autoregressive generation in the mlx-rs-backend.

## Changes Required

### 1. Add KV Cache Structure (after RoPECache, before MlxRsModel)

```rust
/// KV Cache for storing key-value pairs across generation steps
#[cfg(feature = "mlx-rs-backend")]
pub struct MlxRsKVCache {
    keys: Vec<Option<MlxArray>>,    // Per-layer
    values: Vec<Option<MlxArray>>,  // Per-layer
    seq_len: usize,
}

#[cfg(feature = "mlx-rs-backend")]
impl MlxRsKVCache {
    pub fn new(num_layers: usize) -> Self {
        Self {
            keys: vec![None; num_layers],
            values: vec![None; num_layers],
            seq_len: 0,
        }
    }

    pub fn update(&mut self, layer_idx: usize, new_k: &MlxArray, new_v: &MlxArray) -> Result<(MlxArray, MlxArray)> {
        let k = if let Some(cached_k) = &self.keys[layer_idx] {
            MlxArray::concat_axis(&[cached_k, new_k], 1)?  // Concat along seq dim
        } else {
            new_k.clone()
        };

        let v = if let Some(cached_v) = &self.values[layer_idx] {
            MlxArray::concat_axis(&[cached_v, new_v], 1)?
        } else {
            new_v.clone()
        };

        self.keys[layer_idx] = Some(k.clone());
        self.values[layer_idx] = Some(v.clone());

        Ok((k, v))
    }

    pub fn clear(&mut self) {
        for k in &mut self.keys { *k = None; }
        for v in &mut self.values { *v = None; }
        self.seq_len = 0;
    }

    pub fn seq_len(&self) -> usize {
        self.seq_len
    }
}
```

### 2. Add kv_cache field to MlxRsModel

In `pub struct MlxRsModel`:
```rust
    /// KV cache for efficient generation
    kv_cache: Option<MlxRsKVCache>,
```

### 3. Initialize kv_cache in MlxRsModel::load()

In the `Ok(Self { ... })` constructor:
```rust
        Ok(Self {
            weights,
            config,
            embed_tokens,
            lm_head,
            rope_cache,
            kv_cache: None,  // Add this line
        })
```

### 4. Update forward() signature

Change from:
```rust
pub fn forward(&self, token_ids: &[u32], _position: usize) -> Result<Vec<f32>>
```

To:
```rust
pub fn forward(&mut self, token_ids: &[u32], use_cache: bool) -> Result<Vec<f32>>
```

Update docstring to replace `position` with `use_cache`:
```rust
    /// * `use_cache` - Whether to use KV caching for efficient generation
```

### 5. Update transformer_layer_forward() calls in forward()

Change:
```rust
x = self.transformer_layer_forward(layer_idx, x)?;
```

To:
```rust
x = self.transformer_layer_forward(layer_idx, x, use_cache)?;
```

### 6. Update transformer_layer_forward() signature

Change from:
```rust
fn transformer_layer_forward(&self, layer_idx: usize, x: MlxArray) -> Result<MlxArray>
```

To:
```rust
fn transformer_layer_forward(&mut self, layer_idx: usize, x: MlxArray, use_cache: bool) -> Result<MlxArray>
```

Update the call to self_attention:
```rust
let attn_out = self.self_attention(layer_idx, &normed, use_cache)?;
```

### 7. Update self_attention() to integrate KV cache

Change signature from:
```rust
fn self_attention(&self, layer_idx: usize, x: &MlxArray) -> Result<MlxArray>
```

To:
```rust
fn self_attention(&mut self, layer_idx: usize, x: &MlxArray, use_cache: bool) -> Result<MlxArray>
```

Change K and V from immutable to mutable:
```rust
let mut k = x.matmul(&k_proj.transpose()?)?;
let mut v = x.matmul(&v_proj.transpose()?)?;
```

Add KV cache integration BEFORE reshaping for multi-head attention:
```rust
// Integrate KV cache if enabled
if use_cache {
    // Initialize cache if needed
    if self.kv_cache.is_none() {
        self.kv_cache = Some(MlxRsKVCache::new(self.config.num_hidden_layers));
    }

    // Update cache and get full K, V tensors
    let cache = self.kv_cache.as_mut().unwrap();
    let (cached_k, cached_v) = cache.update(layer_idx, &k, &v)?;
    k = cached_k;
    v = cached_v;
}
```

Update reshaping section to handle different sequence lengths:
```rust
// Reshape for multi-head attention
let shape = x.shape();
let batch_size = shape[0];
let input_seq_len = shape[1];  // Changed from seq_len
let num_heads = self.config.num_attention_heads as i32;
let head_dim = (self.config.hidden_size / self.config.num_attention_heads) as i32;

// Get actual sequence length from K/V (may be different from input when using cache)
let kv_shape = k.shape();
let kv_seq_len = kv_shape[1];  // New line

// [batch, seq, hidden] -> [batch, seq, num_heads, head_dim]
let mut q = q.reshape(&[batch_size, input_seq_len, num_heads, head_dim])?;  // Changed
let mut k = k.reshape(&[batch_size, kv_seq_len, num_heads, head_dim])?;  // Changed
let v = v.reshape(&[batch_size, kv_seq_len, num_heads, head_dim])?;  // Changed
```

Update final reshape to use `input_seq_len`:
```rust
let attn_out = attn_out.reshape(&[batch_size, input_seq_len, hidden_size])?;  // Changed from seq_len
```

### 8. Add cache management methods to MlxRsModel

Add these methods before the closing brace of `impl MlxRsModel`:
```rust
    /// Clear the KV cache
    pub fn clear_cache(&mut self) {
        if let Some(cache) = &mut self.kv_cache {
            cache.clear();
        }
    }

    /// Check if KV cache is enabled
    pub fn has_cache(&self) -> bool {
        self.kv_cache.is_some()
    }
```

## Summary

This patch enables KV caching for efficient autoregressive text generation by:
1. Storing key and value tensors across generation steps
2. Concatenating new K/V with cached K/V during subsequent forward passes
3. Handling variable sequence lengths when cache is active
4. Providing cache management methods (clear, has_cache)

The cache is automatically initialized on first use when `use_cache=true` is passed to `forward()`.
