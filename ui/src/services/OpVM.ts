import { createActor } from '@/services/actor';
import type { StatusOperationRecord, OperationState } from '@/api/status';
import { logger } from '@/utils/logger';
import { retryWithBackoff, DEFAULT_RETRY_CONFIG, type RetryConfig, type RetryResult } from '@/utils/retry';

interface OperationRequest {
  descriptor: StatusOperationRecord;
  execute: (signal: AbortSignal) => Promise<void>;
  retryConfig?: Partial<RetryConfig>;
}

export interface OperationSnapshot {
  opId: string;
  tenantId: string;
  command: string;
  state: OperationState;
  attempts: number;
  queued: number;
  lastError?: string;
  lastUpdated: string;
}

interface OperationRuntime {
  request: OperationRequest;
  controller: AbortController;
  lockKey: string;
  handle: OperationHandleImpl;
  cancelled: boolean;
}

interface OpActorState {
  entries: Map<string, OperationSnapshot>;
}

export interface OperationHandle {
  readonly opId: string;
  readonly tenantId: string;
  readonly command: string;
  readonly result: Promise<OperationState>;
  cancel(): void;
}

class OperationHandleImpl implements OperationHandle {
  readonly result: Promise<OperationState>;
  private resolveState!: (state: OperationState) => void;
  private rejectState!: (error: Error) => void;
  private settled = false;
  private cancelHook: () => void;

  constructor(
    public readonly opId: string,
    public readonly tenantId: string,
    public readonly command: string,
    cancelHook: () => void
  ) {
    this.cancelHook = cancelHook;
    this.result = new Promise<OperationState>((resolve, reject) => {
      this.resolveState = resolve;
      this.rejectState = reject;
    });
  }

  cancel(): void {
    this.cancelHook();
  }

  settle(state: OperationState): void {
    if (!this.settled) {
      this.settled = true;
      this.resolveState(state);
    }
  }

  fail(error: Error): void {
    if (!this.settled) {
      this.settled = true;
      this.rejectState(error);
    }
  }
}

function cloneSnapshot(snapshot: OperationSnapshot): OperationSnapshot {
  return { ...snapshot };
}

export class OpVM {
  private actor = createActor<OpActorState>({ entries: new Map() });
  private runtimes = new Map<string, OperationRuntime>();
  private lockByKey = new Map<string, string>();
  private queueByKey = new Map<string, OperationRuntime[]>();

  startOperation(request: OperationRequest): OperationHandle {
    const descriptor = this.cloneDescriptor(request.descriptor);
    const handle = new OperationHandleImpl(
      descriptor.opId,
      descriptor.tenantId,
      descriptor.command,
      () => { void this.cancel(descriptor.opId); }
    );
    const runtime: OperationRuntime = {
      request: { ...request, descriptor },
      controller: new AbortController(),
      lockKey: this.makeLockKey(descriptor.tenantId, descriptor.command),
      handle,
      cancelled: false,
    };

    this.runtimes.set(descriptor.opId, runtime);

    void this.actor.send(state => {
      const entries = new Map(state.entries);
      entries.set(descriptor.opId, {
        opId: descriptor.opId,
        tenantId: descriptor.tenantId,
        command: descriptor.command,
        state: 'pending',
        attempts: descriptor.retries,
        queued: 0,
        lastUpdated: descriptor.lastUpdated,
      });
      return { entries };
    });

    const currentLock = this.lockByKey.get(runtime.lockKey);
    if (currentLock) {
      const queue = this.queueByKey.get(runtime.lockKey) ?? [];
      queue.push(runtime);
      this.queueByKey.set(runtime.lockKey, queue);
      this.updateQueueMetadata(runtime.lockKey);
    } else {
      this.lockByKey.set(runtime.lockKey, descriptor.opId);
      this.runRuntime(runtime).catch(error => {
        const err = error instanceof Error ? error : new Error(String(error));
        logger.error('Operation execution failed', {
          component: 'OpVM',
          operation: descriptor.command,
          tenantId: descriptor.tenantId,
          opId: descriptor.opId,
        }, err);
      });
    }

    return handle;
  }

  async cancel(opId: string): Promise<void> {
    const runtime = this.runtimes.get(opId);
    if (!runtime) {
      await this.updateEntry(opId, snapshot => snapshot ? { ...snapshot, state: 'cancelled', queued: 0, lastUpdated: new Date().toISOString() } : snapshot);
      return;
    }

    runtime.cancelled = true;
    runtime.controller.abort();

    const lockKey = runtime.lockKey;
    const activeOp = this.lockByKey.get(lockKey);
    if (activeOp === opId) {
      // Running operation will observe abort and handle cleanup.
      return;
    }

    const queue = this.queueByKey.get(lockKey);
    if (!queue) {
      this.runtimes.delete(opId);
      await this.updateEntry(opId, snapshot => snapshot ? { ...snapshot, state: 'cancelled', queued: 0, lastUpdated: new Date().toISOString() } : snapshot);
      return;
    }

    const index = queue.findIndex(item => item.request.descriptor.opId === opId);
    if (index >= 0) {
      queue.splice(index, 1);
      this.queueByKey.set(lockKey, queue);
      runtime.handle.settle('cancelled');
      this.runtimes.delete(opId);
      await this.updateEntry(opId, snapshot => snapshot ? { ...snapshot, state: 'cancelled', queued: 0, lastUpdated: new Date().toISOString() } : snapshot);
      this.updateQueueMetadata(lockKey);
    }
  }

  async syncOperations(operations: StatusOperationRecord[]): Promise<void> {
    const runtimeRefs = this.runtimes;
    await this.actor.send(state => {
      const entries = new Map(state.entries);
      const seen = new Set<string>();
      for (const record of operations) {
        const runtime = runtimeRefs.get(record.opId);
        if (runtime) {
          // Preserve runtime-managed snapshot to avoid overriding live state.
          seen.add(record.opId);
          continue;
        }
        entries.set(record.opId, {
          opId: record.opId,
          tenantId: record.tenantId,
          command: record.command,
          state: record.state,
          attempts: record.retries,
          queued: 0,
          lastError: entries.get(record.opId)?.lastError,
          lastUpdated: record.lastUpdated,
        });
        seen.add(record.opId);
      }

      for (const key of entries.keys()) {
        if (!seen.has(key) && !runtimeRefs.has(key)) {
          entries.delete(key);
        }
      }

      return { entries };
    });
  }

  getSnapshot(): Record<string, OperationSnapshot> {
    const snapshot = this.actor.snapshot;
    const result: Record<string, OperationSnapshot> = {};
    for (const [key, value] of snapshot.entries) {
      result[key] = cloneSnapshot(value);
    }
    return result;
  }

  subscribe(listener: (snapshot: Record<string, OperationSnapshot>) => void): () => void {
    return this.actor.subscribe(state => {
      const result: Record<string, OperationSnapshot> = {};
      for (const [key, value] of state.entries) {
        result[key] = cloneSnapshot(value);
      }
      listener(result);
    });
  }

  private async runRuntime(runtime: OperationRuntime): Promise<void> {
    const { descriptor } = runtime.request;
    await this.updateEntry(descriptor.opId, snapshot => snapshot ? { ...snapshot, state: 'running', queued: this.getQueueLength(runtime.lockKey) } : snapshot);

    const retryConfig: Partial<RetryConfig> = {
      ...DEFAULT_RETRY_CONFIG,
      ...(runtime.request.retryConfig ?? {}),
    };

    const operationName = `${descriptor.command}:${descriptor.tenantId}`;

    const result: RetryResult<void> = await retryWithBackoff(
      async () => {
        if (runtime.controller.signal.aborted) {
          throw new Error('Operation aborted');
        }
        await runtime.request.execute(runtime.controller.signal);
      },
      retryConfig,
      (attempt, error, delay) => {
        const message = error instanceof Error ? error.message : String(error);
        void this.updateEntry(descriptor.opId, snapshot => snapshot ? {
          ...snapshot,
          attempts: attempt,
          lastError: message,
          queued: this.getQueueLength(runtime.lockKey),
          lastUpdated: new Date().toISOString(),
        } : snapshot);
        logger.warn('Retrying operation', {
          component: 'OpVM',
          operation: descriptor.command,
          tenantId: descriptor.tenantId,
          opId: descriptor.opId,
          attempt,
          delay,
        });
      },
      operationName
    );

    if (runtime.cancelled || runtime.controller.signal.aborted) {
      runtime.handle.settle('cancelled');
      await this.updateEntry(descriptor.opId, snapshot => snapshot ? {
        ...snapshot,
        state: 'cancelled',
        queued: this.getQueueLength(runtime.lockKey),
        lastUpdated: new Date().toISOString(),
      } : snapshot);
    } else if (result.success) {
      runtime.handle.settle('completed');
      await this.updateEntry(descriptor.opId, snapshot => snapshot ? {
        ...snapshot,
        state: 'completed',
        attempts: result.attempts,
        queued: this.getQueueLength(runtime.lockKey),
        lastError: undefined,
        lastUpdated: new Date().toISOString(),
      } : snapshot);
    } else {
      const failureError = 'error' in result ? result.error : new Error('Operation failed');
      const err = failureError instanceof Error ? failureError : new Error(String(failureError));
      runtime.handle.fail(err);
      await this.updateEntry(descriptor.opId, snapshot => snapshot ? {
        ...snapshot,
        state: 'failed',
        attempts: result.attempts,
        queued: this.getQueueLength(runtime.lockKey),
        lastError: err.message,
        lastUpdated: new Date().toISOString(),
      } : snapshot);
    }

    this.runtimes.delete(descriptor.opId);
    this.releaseLock(runtime.lockKey);
  }

  private releaseLock(lockKey: string): void {
    const queue = this.queueByKey.get(lockKey) ?? [];
    if (queue.length === 0) {
      this.lockByKey.delete(lockKey);
      this.queueByKey.delete(lockKey);
      return;
    }

    const next = queue.shift()!;
    this.queueByKey.set(lockKey, queue);
    this.lockByKey.set(lockKey, next.request.descriptor.opId);
    this.updateQueueMetadata(lockKey);
    this.runRuntime(next).catch(error => {
      const err = error instanceof Error ? error : new Error(String(error));
      logger.error('Queued operation failed', {
        component: 'OpVM',
        operation: next.request.descriptor.command,
        tenantId: next.request.descriptor.tenantId,
        opId: next.request.descriptor.opId,
      }, err);
    });
  }

  private updateQueueMetadata(lockKey: string): void {
    const queue = this.queueByKey.get(lockKey) ?? [];
    queue.forEach((runtime, index) => {
      void this.updateEntry(runtime.request.descriptor.opId, snapshot => snapshot ? {
        ...snapshot,
        queued: index + 1,
      } : snapshot);
    });

    const activeOpId = this.lockByKey.get(lockKey);
    if (activeOpId) {
      void this.updateEntry(activeOpId, snapshot => snapshot ? {
        ...snapshot,
        queued: queue.length,
      } : snapshot);
    }
  }

  private async updateEntry(opId: string, mutator: (snapshot: OperationSnapshot | undefined) => OperationSnapshot | undefined): Promise<void> {
    await this.actor.send(state => {
      const entries = new Map(state.entries);
      const current = entries.get(opId);
      const next = mutator(current ? { ...current } : undefined);
      if (next) {
        entries.set(opId, next);
      } else if (entries.has(opId)) {
        entries.delete(opId);
      }
      return { entries };
    });
  }

  private cloneDescriptor(descriptor: StatusOperationRecord): StatusOperationRecord {
    return {
      ...descriptor,
      metadata: descriptor.metadata ? { ...descriptor.metadata } : undefined,
      lastUpdated: descriptor.lastUpdated || new Date().toISOString(),
    };
  }

  private makeLockKey(tenantId: string, command: string): string {
    return `${tenantId}::${command}`;
  }

  private getQueueLength(lockKey: string): number {
    const queue = this.queueByKey.get(lockKey);
    return queue ? queue.length : 0;
  }
}

