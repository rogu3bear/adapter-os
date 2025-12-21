import React from 'react';
import { Badge } from '@/components/ui/badge';
import type { EvidenceStatus } from '@/api/document-types';

interface EvidenceStatusBadgeProps {
  status?: EvidenceStatus | null;
}

const STATUS_STYLES: Record<
  EvidenceStatus | 'unknown',
  { label: string; className: string; variant?: React.ComponentProps<typeof Badge>['variant'] }
> = {
  queued: {
    label: 'Queued',
    className: 'text-amber-700 border-amber-200 bg-amber-50',
    variant: 'outline',
  },
  building: {
    label: 'Building',
    className: 'text-blue-700 border-blue-200 bg-blue-50',
    variant: 'outline',
  },
  ready: {
    label: 'Ready',
    className: 'text-green-700 border-green-200 bg-green-50',
    variant: 'secondary',
  },
  failed: {
    label: 'Failed',
    className: 'text-red-700 border-red-200 bg-red-50',
    variant: 'destructive',
  },
  unknown: {
    label: 'Unknown',
    className: 'text-slate-700 border-slate-200 bg-slate-50',
    variant: 'outline',
  },
};

export function EvidenceStatusBadge({ status }: EvidenceStatusBadgeProps) {
  const style = status ? STATUS_STYLES[status] : STATUS_STYLES.unknown;
  return (
    <Badge variant={style.variant ?? 'outline'} className={style.className}>
      {style.label}
    </Badge>
  );
}

