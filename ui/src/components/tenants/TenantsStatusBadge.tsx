import React from 'react';
import { Tenant as ApiTenant } from '@/api/types';
import {
  CheckCircle,
  AlertTriangle,
  Settings,
  Lock,
  Database,
} from 'lucide-react';

export interface TenantsStatusBadgeProps {
  status?: ApiTenant['status'];
}

export function TenantsStatusBadge({ status }: TenantsStatusBadgeProps) {
  const currentStatus = status || 'active';

  switch (currentStatus) {
    case 'active':
      return (
        <div className="status-indicator status-success">
          <CheckCircle className="h-3 w-3" />
          Active
        </div>
      );
    case 'suspended':
      return (
        <div className="status-indicator status-error">
          <AlertTriangle className="h-3 w-3" />
          Suspended
        </div>
      );
    case 'maintenance':
      return (
        <div className="status-indicator status-warning">
          <Settings className="h-3 w-3" />
          Maintenance
        </div>
      );
    case 'paused':
      return (
        <div className="status-indicator status-neutral">
          <Lock className="h-3 w-3" />
          Inactive
        </div>
      );
    case 'archived':
      return (
        <div className="status-indicator status-neutral">
          <Database className="h-3 w-3" />
          Archived
        </div>
      );
    default:
      return <div className="status-indicator status-neutral">Unknown</div>;
  }
}

export interface ClassificationBadgeProps {
  classification?: ApiTenant['data_classification'];
}

export function ClassificationBadge({ classification }: ClassificationBadgeProps) {
  const current = classification || 'internal';
  const colors: Record<string, string> = {
    public: 'status-info',
    internal: 'status-neutral',
    confidential: 'status-warning',
    restricted: 'status-error',
  };

  return (
    <div className={`status-indicator ${colors[current] || 'status-neutral'}`}>
      <Lock className="h-3 w-3" />
      {current.toUpperCase()}
    </div>
  );
}
