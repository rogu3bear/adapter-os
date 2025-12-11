import { describe, expect, it } from 'vitest';

import type { AdapterHealthFlag, AdapterHealthResponse, AdapterHealthSubcode } from '@/api/adapter-types';
import { describeSubcode, pickPrimarySubcode } from '../health';

function makeHealth(
  health: AdapterHealthFlag,
  subcodes: AdapterHealthSubcode[],
  primary_subcode?: AdapterHealthSubcode
): AdapterHealthResponse {
  return {
    schema_version: 'v1',
    adapter_id: 'adapter-1',
    health,
    primary_subcode,
    subcodes,
    drift_summary: undefined,
    datasets: [],
    storage: undefined,
    backend: undefined,
    recent_activations: [],
    total_activations: 0,
    selected_count: 0,
    avg_gate_value: 0,
    memory_usage_mb: 0,
    policy_violations: [],
  };
}

describe('health utils', () => {
  const trustBlocked: AdapterHealthSubcode = {
    domain: 'trust',
    code: 'trust_blocked',
    message: undefined,
    data: undefined,
  };

  const hashMismatch: AdapterHealthSubcode = {
    domain: 'storage',
    code: 'hash_mismatch',
    message: undefined,
    data: undefined,
  };

  it('selects trust_blocked as primary for unsafe', () => {
    const primary = pickPrimarySubcode(makeHealth('unsafe', [trustBlocked]));
    expect(primary?.code).toBe('trust_blocked');
    expect(primary?.domain).toBe('trust');
  });

  it('prefers storage subcode for corrupt even when trust exists', () => {
    const primary = pickPrimarySubcode(makeHealth('corrupt', [trustBlocked, hashMismatch]));
    expect(primary?.domain).toBe('storage');
    expect(primary?.code).toBe('hash_mismatch');
  });

  it('describes storage corruption with friendly labels', () => {
    const detail = describeSubcode(hashMismatch);
    expect(detail.label).toBe('Artifact hash mismatch');
    expect(detail.detail).toContain('hash');
  });
});
