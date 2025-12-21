import { describe, expect, it, vi, afterEach } from 'vitest';

import { AppVM } from '@/services/AppVM';
import { OpVM } from '@/services/OpVM';
import { computeStatusDigest } from '@/api/status';
import { buildStatus, buildOperation, createDeferred, waitForCondition } from '@/test/utils';

afterEach(() => {
  vi.useRealTimers();
});

describe('AppVM', () => {
  it('rejects status when signature is corrupt', async () => {
    const status = buildStatus();
    const digest = await computeStatusDigest(status);
    status.signature.value = digest.digest;

    const appVm = new AppVM();

    const accepted = await appVm.load(status);
    expect(accepted).toBe(true);
    expect(appVm.getSnapshot().verification?.valid).toBe(true);

    const corrupt = buildStatus({
      tenants: status.tenants,
      operations: status.operations,
      signature: { ...status.signature, value: 'corrupt-signature' },
    });

    const rejected = await appVm.load(corrupt);
    expect(rejected).toBe(false);
    const snapshot = appVm.getSnapshot();
    expect(snapshot.lastError).toBe('Invalid status signature');
    expect(snapshot.verification?.valid).toBe(false);
    // Previous good status remains intact
    expect(snapshot.status?.nonce).toBe(status.nonce);
  });

  it('denies render when tenant permissions lack render allowance', async () => {
    const status = buildStatus({
      tenants: [
        {
          tenantId: 'tenant-42',
          displayName: 'Tenant Forty Two',
          isolationLevel: 'shared',
          permissions: ['role:viewer', 'deny:render'],
        },
      ],
    });
    const digest = await computeStatusDigest(status);
    status.signature.value = digest.digest;

    const appVm = new AppVM();
    await appVm.load(status);

    const tenantVm = appVm.getTenantVM('tenant-42');
    expect(tenantVm).toBeDefined();
    expect(tenantVm?.canRender()).toBe(false);
    expect(tenantVm?.getAllowedCommands()).not.toContain('render');

    const tenants = appVm.getSnapshot().tenants;
    expect(tenants['tenant-42'].canRender).toBe(false);
  });
});

describe('OpVM', () => {
  it('queues overlapping operations and retries with backoff', async () => {
    vi.useFakeTimers();

    const opVm = new OpVM();

    const firstGate = createDeferred<void>();
    let firstStarted = false;
    const firstExecution = vi.fn(async () => {
      firstStarted = true;
      await firstGate.promise;
    });

    const op1 = buildOperation('op-1');
    const handle1 = opVm.startOperation({
      descriptor: op1,
      execute: firstExecution,
    });

    await waitForCondition(() => firstStarted);
    expect(firstExecution).toHaveBeenCalledTimes(1);

    const op2 = buildOperation('op-2');
    let secondAttempts = 0;
    const secondExecution = vi.fn(async () => {
      secondAttempts += 1;
      if (secondAttempts === 1) {
        throw new Error('transient');
      }
    });

    const handle2 = opVm.startOperation({
      descriptor: op2,
      execute: secondExecution,
      retryConfig: {
        maxAttempts: 2,
        baseDelay: 5,
        maxDelay: 5,
        jitter: 0,
        retryableErrors: () => true,
      },
    });

    expect(secondExecution).not.toHaveBeenCalled();

    firstGate.resolve();
    expect(firstExecution).toHaveBeenCalledTimes(1);
    await handle1.result;

    await vi.runOnlyPendingTimersAsync();
    await waitForCondition(() => secondAttempts >= 1);

    vi.runAllTimers();
    await vi.runOnlyPendingTimersAsync();
    await waitForCondition(() => secondAttempts >= 2);
    const finalState = await handle2.result;
    expect(finalState).toBe('completed');

    const snapshot = opVm.getSnapshot()['op-2'];
    expect(snapshot).toBeDefined();
    expect(snapshot.state).toBe('completed');
    expect(snapshot.attempts).toBe(2);
    expect(snapshot.lastError).toBeUndefined();
  });
});

