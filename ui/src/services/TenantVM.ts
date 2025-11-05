import type { StatusTenantRecord } from '@/api/status';
import { hasRoleLevel, type User, type UserRole } from '@/lib/rbac';

const ROLE_SEMAPHORE: Array<{ role: UserRole; commands: string[] }> = [
  { role: 'viewer', commands: ['render'] },
  { role: 'compliance', commands: ['render', 'view-compliance'] },
  { role: 'operator', commands: ['render', 'view-compliance', 'operate'] },
  { role: 'admin', commands: ['render', 'view-compliance', 'operate', 'admin'] },
];

const ROLE_PRIORITY: UserRole[] = ['viewer', 'compliance', 'operator', 'admin'];

export interface TenantSnapshot {
  tenantId: string;
  displayName: string;
  isolationLevel: string;
  permissions: string[];
  allowedCommands: string[];
  canRender: boolean;
}

function resolveRole(permissions: string[]): UserRole {
  let resolved: UserRole = 'viewer';
  for (const permission of permissions) {
    if (permission.startsWith('role:')) {
      const roleCandidate = permission.substring(5) as UserRole;
      if (ROLE_PRIORITY.includes(roleCandidate) && ROLE_PRIORITY.indexOf(roleCandidate) > ROLE_PRIORITY.indexOf(resolved)) {
        resolved = roleCandidate;
      }
    }
  }
  return resolved;
}

function normalizeCommand(command: string): string {
  if (command.startsWith('command:')) {
    return command.substring('command:'.length);
  }
  if (command.startsWith('allow:')) {
    return command.substring('allow:'.length);
  }
  return command;
}

export class TenantVM {
  private record: StatusTenantRecord;
  private allowedCommands: Set<string>;

  constructor(record: StatusTenantRecord) {
    this.record = { ...record, permissions: [...record.permissions] };
    this.allowedCommands = this.computeAllowed(record.permissions);
  }

  update(record: StatusTenantRecord): void {
    this.record = { ...record, permissions: [...record.permissions] };
    this.allowedCommands = this.computeAllowed(record.permissions);
  }

  get tenantId(): string {
    return this.record.tenantId;
  }

  getAllowedCommands(): string[] {
    return Array.from(this.allowedCommands.values()).sort();
  }

  canRender(): boolean {
    return this.allowedCommands.has('render');
  }

  toSnapshot(): TenantSnapshot {
    return {
      tenantId: this.record.tenantId,
      displayName: this.record.displayName,
      isolationLevel: this.record.isolationLevel,
      permissions: [...this.record.permissions],
      allowedCommands: this.getAllowedCommands(),
      canRender: this.canRender(),
    };
  }

  private computeAllowed(permissions: string[]): Set<string> {
    const role = resolveRole(permissions);
    const user: User = {
      id: `${this.record.tenantId}-synthetic`,
      email: `${this.record.tenantId}@tenant.local`,
      role,
    };

    const allowed = new Set<string>();

    for (const { role: roleGate, commands } of ROLE_SEMAPHORE) {
      if (hasRoleLevel(user, roleGate)) {
        commands.forEach(command => allowed.add(command));
      }
    }

    for (const permission of permissions) {
      if (permission.startsWith('command:') || permission.startsWith('allow:')) {
        allowed.add(normalizeCommand(permission));
      } else if (permission.startsWith('deny:')) {
        const denied = permission.substring('deny:'.length);
        allowed.delete(denied);
      }
    }

    return allowed;
  }
}

