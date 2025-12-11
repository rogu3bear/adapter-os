import React from 'react';
import { Badge } from '@/components/ui/badge';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import type { TrustState } from '@/api/training-types';
import type { AdapterHealthFlag } from '@/api/adapter-types';

type PillSize = 'sm' | 'md';

const TRUST_LABELS: Record<TrustState | 'unknown', string> = {
  allowed: 'Allowed',
  allowed_with_warning: 'Allowed w/ warning',
  blocked: 'Blocked',
  needs_approval: 'Needs approval',
  unknown: 'Unknown',
};

const TRUST_COPY: Record<TrustState | 'unknown', string> = {
  allowed: 'Cleared for training and evaluation.',
  allowed_with_warning: 'Training allowed but flagged; review warnings before proceeding.',
  blocked: 'Training blocked; resolve safety/validation issues or override.',
  needs_approval: 'Training blocked until approval/validation is completed.',
  unknown: 'Trust not yet evaluated; training is blocked until reviewed.',
};

const TRUST_CLASSES: Record<TrustState | 'unknown', string> = {
  allowed: 'bg-emerald-100 text-emerald-800 border-emerald-200',
  allowed_with_warning: 'bg-amber-100 text-amber-900 border-amber-200',
  blocked: 'bg-red-100 text-red-800 border-red-200',
  needs_approval: 'bg-blue-100 text-blue-800 border-blue-200',
  unknown: 'bg-slate-100 text-slate-700 border-slate-200',
};

const HEALTH_LABELS: Record<AdapterHealthFlag | 'unknown', string> = {
  healthy: 'Healthy',
  degraded: 'Degraded',
  unsafe: 'Unsafe',
  corrupt: 'Corrupt',
  unknown: 'Unknown',
};

const HEALTH_COPY: Record<AdapterHealthFlag | 'unknown', string> = {
  healthy: 'Adapter passed health checks and validations.',
  degraded: 'Adapter serving with degraded signals; investigate.',
  unsafe: 'Unsafe for serving; policy or safety violations detected.',
  corrupt: 'Artifacts or signatures corrupt; do not serve.',
  unknown: 'Health not reported for this adapter.',
};

const HEALTH_CLASSES: Record<AdapterHealthFlag | 'unknown', string> = {
  healthy: 'bg-emerald-100 text-emerald-800 border-emerald-200',
  degraded: 'bg-amber-100 text-amber-900 border-amber-200',
  unsafe: 'bg-red-100 text-red-800 border-red-200',
  corrupt: 'bg-rose-100 text-rose-800 border-rose-200',
  unknown: 'bg-slate-100 text-slate-700 border-slate-200',
};

const SIZE_CLASSES: Record<PillSize, string> = {
  sm: 'text-[11px] px-2 py-0.5',
  md: 'text-xs px-2.5 py-0.5',
};

export interface TrustBadgeProps {
  state?: TrustState | null;
  reason?: string;
  size?: PillSize;
}

export function TrustBadge({ state, reason, size = 'md' }: TrustBadgeProps) {
  const effectiveState = (state ?? 'unknown') as TrustState | 'unknown';
  const label = TRUST_LABELS[effectiveState];
  const tooltip = reason ? `${TRUST_COPY[effectiveState]} • ${reason}` : TRUST_COPY[effectiveState];

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Badge variant="outline" className={`${TRUST_CLASSES[effectiveState]} ${SIZE_CLASSES[size]}`}>
          {label}
        </Badge>
      </TooltipTrigger>
      <TooltipContent side="top" className="max-w-xs text-xs">
        {tooltip}
      </TooltipContent>
    </Tooltip>
  );
}

export interface HealthBadgeProps {
  state?: AdapterHealthFlag | null;
  reason?: string;
  size?: PillSize;
}

export function HealthBadge({ state, reason, size = 'md' }: HealthBadgeProps) {
  const effectiveState = (state ?? 'unknown') as AdapterHealthFlag | 'unknown';
  const label = HEALTH_LABELS[effectiveState];
  const tooltip = reason ? `${HEALTH_COPY[effectiveState]} • ${reason}` : HEALTH_COPY[effectiveState];

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Badge variant="outline" className={`${HEALTH_CLASSES[effectiveState]} ${SIZE_CLASSES[size]}`}>
          {label}
        </Badge>
      </TooltipTrigger>
      <TooltipContent side="top" className="max-w-xs text-xs">
        {tooltip}
      </TooltipContent>
    </Tooltip>
  );
}

export function getHealthCopy(state?: AdapterHealthFlag | null) {
  return HEALTH_COPY[(state ?? 'unknown') as AdapterHealthFlag | 'unknown'];
}

export function getTrustCopy(state?: TrustState | null) {
  return TRUST_COPY[(state ?? 'unknown') as TrustState | 'unknown'];
}
