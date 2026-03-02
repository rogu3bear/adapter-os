//! Session-aware KV cache manager for multi-turn chat.
//!
//! Maps session IDs to C++ KV cache handles, with LRU eviction
//! based on time and memory pressure.

use adapteros_core::{AosError, Result};
use std::collections::HashMap;
use std::time::Instant;

/// Manages per-session KV caches for multi-turn chat performance.
///
/// Each active chat session gets its own KV cache so that subsequent
/// messages don't need to reprocess the full conversation history.
pub struct SessionCacheManager {
    caches: HashMap<String, SessionEntry>,
    max_sessions: usize,
    max_memory_bytes: usize,
}

struct SessionEntry {
    cache_ptr: *mut crate::mlx_kv_cache_t,
    last_used: Instant,
    seq_len: usize,
    estimated_bytes: usize,
}

impl SessionCacheManager {
    /// Create a new session cache manager.
    ///
    /// # Arguments
    /// * `max_sessions` - Maximum number of concurrent session caches
    /// * `max_memory_bytes` - Maximum total memory for all caches
    pub fn new(max_sessions: usize, max_memory_bytes: usize) -> Self {
        Self {
            caches: HashMap::new(),
            max_sessions,
            max_memory_bytes,
        }
    }

    /// Get or create a KV cache for a session.
    ///
    /// Returns the cache pointer for the session. Creates a new cache
    /// if one doesn't exist, potentially evicting LRU entries.
    pub fn get_or_create(
        &mut self,
        session_id: &str,
        num_layers: i32,
        num_heads: i32,
        head_dim: i32,
        max_seq_len: i32,
    ) -> Result<*mut crate::mlx_kv_cache_t> {
        // If cache exists, update last_used and return
        if let Some(entry) = self.caches.get_mut(session_id) {
            entry.last_used = Instant::now();
            return Ok(entry.cache_ptr);
        }

        // Evict if at capacity
        while self.caches.len() >= self.max_sessions || self.total_memory() > self.max_memory_bytes
        {
            if !self.evict_lru() {
                break; // Nothing left to evict
            }
        }

        // Create new cache
        let cache_ptr =
            unsafe { crate::mlx_kv_cache_new(num_layers, num_heads, head_dim, max_seq_len) };

        if cache_ptr.is_null() {
            return Err(AosError::Mlx(
                "Failed to create KV cache for session".to_string(),
            ));
        }

        // Estimate memory: 2 * num_layers * num_heads * head_dim * max_seq_len * sizeof(f32)
        // Factor of 2 for keys + values
        let estimated_bytes = 2
            * num_layers as usize
            * num_heads as usize
            * head_dim as usize
            * max_seq_len as usize
            * std::mem::size_of::<f32>();

        self.caches.insert(
            session_id.to_string(),
            SessionEntry {
                cache_ptr,
                last_used: Instant::now(),
                seq_len: 0,
                estimated_bytes,
            },
        );

        tracing::debug!(
            session_id = session_id,
            estimated_mb = estimated_bytes as f64 / (1024.0 * 1024.0),
            active_sessions = self.caches.len(),
            "Created new KV cache for session"
        );

        Ok(cache_ptr)
    }

    /// Clear the cache for a specific session.
    pub fn clear(&mut self, session_id: &str) {
        if let Some(entry) = self.caches.remove(session_id) {
            unsafe { crate::mlx_kv_cache_free(entry.cache_ptr) };
            tracing::debug!(session_id = session_id, "Cleared KV cache for session");
        }
    }

    /// Clear all session caches.
    pub fn clear_all(&mut self) {
        for (session_id, entry) in self.caches.drain() {
            unsafe { crate::mlx_kv_cache_free(entry.cache_ptr) };
            tracing::trace!(session_id = session_id, "Cleared KV cache");
        }
        tracing::debug!("Cleared all session KV caches");
    }

    /// Evict the least recently used session cache.
    /// Returns true if an entry was evicted, false if empty.
    pub fn evict_lru(&mut self) -> bool {
        let oldest = self
            .caches
            .iter()
            .min_by_key(|(_, entry)| entry.last_used)
            .map(|(k, _)| k.clone());

        if let Some(key) = oldest {
            if let Some(entry) = self.caches.remove(&key) {
                unsafe { crate::mlx_kv_cache_free(entry.cache_ptr) };
                tracing::debug!(
                    session_id = key,
                    age_secs = entry.last_used.elapsed().as_secs(),
                    "Evicted LRU session KV cache"
                );
                return true;
            }
        }
        false
    }

    /// Get total estimated memory usage across all caches.
    pub fn total_memory(&self) -> usize {
        self.caches.values().map(|e| e.estimated_bytes).sum()
    }

    /// Get the number of active sessions.
    pub fn active_sessions(&self) -> usize {
        self.caches.len()
    }

    /// Update the sequence length for a session (for memory tracking).
    pub fn update_seq_len(&mut self, session_id: &str, seq_len: usize) {
        if let Some(entry) = self.caches.get_mut(session_id) {
            entry.seq_len = seq_len;
        }
    }
}

impl Drop for SessionCacheManager {
    fn drop(&mut self) {
        self.clear_all();
    }
}

// SAFETY: The raw KV cache pointers are not thread-safe, but all access is
// serialized through the backend's &mut self requirement on run_step and the
// model's inference_lock. This matches MLXFFIModel's Send/Sync pattern.
unsafe impl Send for SessionCacheManager {}
unsafe impl Sync for SessionCacheManager {}

// SessionEntry holds a raw pointer; same safety argument applies.
unsafe impl Send for SessionEntry {}
unsafe impl Sync for SessionEntry {}
