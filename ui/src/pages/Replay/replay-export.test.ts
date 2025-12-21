import { describe, expect, it } from 'vitest';
import type { ReplaySession } from '@/api/types';
import replayExportFixture from './__fixtures__/replay-export.json';

function toStableExport(session: ReplaySession) {
  const { id, cpid, plan_id, manifest_hash_b3, policy_hash_b3, kernel_hash_b3, telemetry_bundle_ids, config } = session;
  return {
    id,
    cpid,
    plan_id,
    manifest_hash_b3,
    policy_hash_b3,
    kernel_hash_b3: kernel_hash_b3 ?? null,
    telemetry_bundle_ids,
    config: config ?? {},
  };
}

describe('Replay export', () => {
  it('matches golden fixture for stable fields', () => {
    const session: ReplaySession = {
      id: 'replay-123',
      cpid: 'cp-abc',
      plan_id: 'plan-xyz',
      snapshot_at: '2025-01-01T00:00:00Z',
      telemetry_bundle_ids: ['bundle-a', 'bundle-b'],
      manifest_hash_b3: 'b3-manifest',
      policy_hash_b3: 'b3-policy',
      kernel_hash_b3: 'b3-kernel',
      config: {
        max_tokens: 256,
        temperature: 0.7,
        top_k: 50,
        top_p: 0.95,
        seed: 12345,
        require_evidence: true,
      },
    };

    expect(toStableExport(session)).toEqual(replayExportFixture);
  });
});


