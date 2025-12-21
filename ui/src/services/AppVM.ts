import { createActor } from '@/services/actor';
import { sanitizeStatus, verifyStatusSignature, type SignatureVerificationResult, type StatusOperationRecord, type StatusV2 } from '@/api/status';
import { TenantVM, type TenantSnapshot } from '@/services/TenantVM';
import { OpVM, type OperationSnapshot } from '@/services/OpVM';
import type { OperationState } from '@/api/status';
import { logger } from '@/utils/logger';

export interface OperationAggregate {
  opId: string;
  tenantId: string;
  command: string;
  state: OperationState;
  attempts: number;
  queued: number;
  source: 'status' | 'runtime' | 'merged';
  lastUpdated: string;
  lastError?: string;
}

interface AppActorState {
  status?: StatusV2;
  verification?: SignatureVerificationResult;
  tenants: Record<string, TenantSnapshot>;
  aggregatedOps: Record<string, OperationAggregate[]>;
  lastError?: string;
}

export type AppSnapshot = AppActorState;

export class AppVM {
  private actor = createActor<AppActorState>({ tenants: {}, aggregatedOps: {} });
  private tenantVMs = new Map<string, TenantVM>();
  private opVM: OpVM;

  constructor(opVm?: OpVM) {
    this.opVM = opVm ?? new OpVM();
  }

  get opViewModel(): OpVM {
    return this.opVM;
  }

  async load(status: StatusV2): Promise<boolean> {
    const sanitized = sanitizeStatus(status);
    const verification = await verifyStatusSignature(sanitized);

    if (!verification.valid) {
      await this.actor.send(current => ({
        ...current,
        verification,
        lastError: 'Invalid status signature',
      }));
      logger.warn('Rejected status update due to invalid signature', {
        component: 'AppVM',
        operation: 'load',
      });
      return false;
    }

    this.reconcileTenants(sanitized);
    await this.opVM.syncOperations(sanitized.operations);

    const tenantSnapshots = this.buildTenantSnapshots();
    const aggregatedOps = this.aggregateOperations(sanitized.operations, this.opVM.getSnapshot());

    await this.actor.send(() => ({
      status: sanitized,
      verification,
      tenants: tenantSnapshots,
      aggregatedOps,
      lastError: undefined,
    }));

    logger.info('Status update applied', {
      component: 'AppVM',
      operation: 'load',
      tenantCount: sanitized.tenants.length,
      operationCount: sanitized.operations.length,
    });

    return true;
  }

  getSnapshot(): AppSnapshot {
    const snapshot = this.actor.snapshot;
    return {
      status: snapshot.status ? sanitizeStatus(snapshot.status) : undefined,
      verification: snapshot.verification,
      tenants: { ...snapshot.tenants },
      aggregatedOps: this.cloneAggregated(snapshot.aggregatedOps),
      lastError: snapshot.lastError,
    };
  }

  subscribe(listener: (snapshot: AppSnapshot) => void): () => void {
    return this.actor.subscribe(state => {
      listener({
        status: state.status ? sanitizeStatus(state.status) : undefined,
        verification: state.verification,
        tenants: { ...state.tenants },
        aggregatedOps: this.cloneAggregated(state.aggregatedOps),
        lastError: state.lastError,
      });
    });
  }

  getTenantVM(tenantId: string): TenantVM | undefined {
    return this.tenantVMs.get(tenantId);
  }

  getTenants(): TenantSnapshot[] {
    return Array.from(this.tenantVMs.values()).map(vm => vm.toSnapshot());
  }

  private reconcileTenants(status: StatusV2): void {
    const nextTenantIds = new Set(status.tenants.map(tenant => tenant.tenantId));
    for (const tenant of status.tenants) {
      const existing = this.tenantVMs.get(tenant.tenantId);
      if (existing) {
        existing.update(tenant);
      } else {
        this.tenantVMs.set(tenant.tenantId, new TenantVM(tenant));
      }
    }

    for (const tenantId of Array.from(this.tenantVMs.keys())) {
      if (!nextTenantIds.has(tenantId)) {
        this.tenantVMs.delete(tenantId);
      }
    }
  }

  private buildTenantSnapshots(): Record<string, TenantSnapshot> {
    const result: Record<string, TenantSnapshot> = {};
    for (const [tenantId, vm] of this.tenantVMs.entries()) {
      result[tenantId] = vm.toSnapshot();
    }
    return result;
  }

  private aggregateOperations(
    statusOps: StatusOperationRecord[],
    runtimeOps: Record<string, OperationSnapshot>
  ): Record<string, OperationAggregate[]> {
    const aggregated: Record<string, OperationAggregate[]> = {};
    const seenRuntime = new Set<string>();

    const push = (tenantId: string, aggregate: OperationAggregate) => {
      if (!aggregated[tenantId]) {
        aggregated[tenantId] = [];
      }
      aggregated[tenantId].push(aggregate);
    };

    for (const op of statusOps) {
      const runtime = runtimeOps[op.opId];
      if (runtime) {
        push(op.tenantId, {
          opId: runtime.opId,
          tenantId: runtime.tenantId,
          command: runtime.command,
          state: runtime.state,
          attempts: runtime.attempts,
          queued: runtime.queued,
          source: 'merged',
          lastUpdated: runtime.lastUpdated,
          lastError: runtime.lastError,
        });
        seenRuntime.add(op.opId);
      } else {
        push(op.tenantId, {
          opId: op.opId,
          tenantId: op.tenantId,
          command: op.command,
          state: op.state,
          attempts: op.retries,
          queued: 0,
          source: 'status',
          lastUpdated: op.lastUpdated,
          lastError: undefined,
        });
      }
    }

    for (const runtime of Object.values(runtimeOps)) {
      if (seenRuntime.has(runtime.opId)) continue;
      push(runtime.tenantId, {
        opId: runtime.opId,
        tenantId: runtime.tenantId,
        command: runtime.command,
        state: runtime.state,
        attempts: runtime.attempts,
        queued: runtime.queued,
        source: 'runtime',
        lastUpdated: runtime.lastUpdated,
        lastError: runtime.lastError,
      });
    }

    for (const tenantId of Object.keys(aggregated)) {
      aggregated[tenantId].sort((a, b) => b.lastUpdated.localeCompare(a.lastUpdated));
    }

    return aggregated;
  }

  private cloneAggregated(source: Record<string, OperationAggregate[]>): Record<string, OperationAggregate[]> {
    const result: Record<string, OperationAggregate[]> = {};
    for (const [key, items] of Object.entries(source)) {
      result[key] = items.map(item => ({ ...item }));
    }
    return result;
  }
}

