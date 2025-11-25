// Memory estimation utilities for adapters
// 【2025-01-20†rectification†memory_estimation】

import type { Adapter } from '@/api/adapter-types';

/**
 * Estimate memory usage for an adapter based on rank and tier.
 * 
 * **IMPORTANT:** This is an approximation when `memory_bytes` is not available.
 * The estimation is based on typical LoRA adapter memory usage patterns:
 * - Base memory: ~4MB per rank unit (includes weights, activations, gradients)
 * - Tier multipliers account for different lifecycle states and caching strategies
 * 
 * **Accuracy:** This estimation may vary by ±30% depending on:
 * - Actual model architecture
 * - Quantization settings
 * - Backend implementation (CoreML vs MLX vs Metal)
 * - Batch size and sequence length
 * 
 * **Recommendation:** Always prefer `memory_bytes` from the adapter manifest when available.
 * This estimation should only be used as a fallback for capacity planning.
 * 
 * @param adapter - Adapter object with rank, tier, and optionally memory_bytes
 * @returns Estimated memory usage in bytes
 */
export function estimateAdapterMemory(adapter: Adapter): number {
  // If memory_bytes is available, use it (most accurate)
  if (adapter.memory_bytes && adapter.memory_bytes > 0) {
    return adapter.memory_bytes;
  }

  // Approximation based on rank and tier
  // Base memory per rank unit (in bytes)
  // Note: This is based on typical LoRA patterns. Actual usage may vary.
  const BASE_MEMORY_PER_RANK = 4 * 1024 * 1024; // 4MB per rank unit
  
  // Tier multipliers
  const tierMultipliers: Record<string, number> = {
    'persistent': 1.0,
    'warm': 0.8,
    'ephemeral': 0.6,
  };

  const rank = adapter.rank || 16; // Default rank
  const tier = adapter.tier || 'warm';
  const multiplier = tierMultipliers[tier] || 1.0;

  // Estimate: rank * base * multiplier
  const estimatedBytes = Math.round(rank * BASE_MEMORY_PER_RANK * multiplier);

  return estimatedBytes;
}

/**
 * Calculate total memory for a list of adapter IDs
 */
export function calculateTotalMemory(
  adapterIds: string[],
  adapters: Adapter[]
): { totalBytes: number; estimated: boolean; missing: string[] } {
  let totalBytes = 0;
  const missing: string[] = [];
  let estimated = false;

  adapterIds.forEach(adapterId => {
    const adapter = adapters.find(a => a.id === adapterId || a.adapter_id === adapterId);
    if (!adapter) {
      missing.push(adapterId);
      return;
    }

    const memoryBytes = estimateAdapterMemory(adapter);
    if (!adapter.memory_bytes || adapter.memory_bytes === 0) {
      estimated = true;
    }
    totalBytes += memoryBytes;
  });

  return { totalBytes, estimated, missing };
}

