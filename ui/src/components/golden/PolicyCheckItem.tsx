import React from 'react';
import { Badge } from '@/components/ui/badge';
import { PolicyCheck, PolicyStatus } from './PolicyCheckDisplay';

export interface PolicyCheckItemProps {
  policy: PolicyCheck;
  icon?: React.ReactNode;
}

function PolicyCheckItemComponent({ policy, icon }: PolicyCheckItemProps) {
  const getSeverityColor = (severity: string) => {
    switch (severity) {
      case 'critical':
        return 'bg-red-100 text-red-900 border-red-300';
      case 'high':
        return 'bg-orange-100 text-orange-900 border-orange-300';
      case 'medium':
        return 'bg-yellow-100 text-yellow-900 border-yellow-300';
      case 'low':
        return 'bg-blue-100 text-blue-900 border-blue-300';
      default:
        return 'bg-gray-100 text-gray-900 border-gray-300';
    }
  };

  const getStatusBadgeVariant = (status: PolicyStatus) => {
    switch (status) {
      case 'passed':
        return 'success';
      case 'failed':
        return 'error';
      case 'warning':
        return 'warning';
      case 'pending':
        return 'info';
      default:
        return 'neutral';
    }
  };

  const statusLabel = {
    passed: 'Passed',
    failed: 'Failed',
    warning: 'Warning',
    pending: 'Pending',
  }[policy.status];

  return (
    <div className="flex items-center justify-between w-full gap-2">
      <div className="flex items-center gap-3 flex-1 min-w-0">
        {icon && <div className="flex-shrink-0">{icon}</div>}
        <div className="flex-1 min-w-0">
          <p className="font-medium text-sm truncate">{policy.name}</p>
          <p className="text-xs text-muted-foreground truncate">{policy.description}</p>
        </div>
      </div>

      <div className="flex items-center gap-2 flex-shrink-0">
        {policy.message && (
          <span
            className="text-xs text-muted-foreground bg-muted px-2 py-1 rounded max-w-[200px] truncate"
            title={policy.message}
          >
            {policy.message}
          </span>
        )}

        <Badge variant={getStatusBadgeVariant(policy.status)} className="flex-shrink-0">
          {statusLabel}
        </Badge>

        <div
          className={`px-2 py-1 rounded text-xs font-medium border ${getSeverityColor(
            policy.severity,
          )}`}
        >
          {policy.severity}
        </div>
      </div>
    </div>
  );
}

export const PolicyCheckItem = React.memo(PolicyCheckItemComponent);
