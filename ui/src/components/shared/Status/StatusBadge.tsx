/**
 * StatusBadge Component
 *
 * Unified status badge component that replaces duplicated badge implementations.
 * Supports multiple contexts: adapter, training, policy, document, federation.
 */

import React from 'react';
import { Badge } from '../../ui/badge';
import { cn } from '@/lib/utils';
import {
  CheckCircle,
  XCircle,
  Clock,
  AlertTriangle,
  Loader2,
  Pause,
  Play,
  type LucideIcon,
} from 'lucide-react';

export type StatusContext = 'adapter' | 'training' | 'policy' | 'document' | 'federation' | 'generic';

interface StatusConfig {
  variant: 'default' | 'secondary' | 'destructive' | 'outline';
  className: string;
  icon?: LucideIcon;
}

/**
 * Status configurations per context
 */
const STATUS_CONFIGS: Record<StatusContext, Record<string, StatusConfig>> = {
  adapter: {
    unloaded: { variant: 'outline', className: 'bg-gray-100 text-gray-800' },
    cold: { variant: 'secondary', className: 'bg-blue-100 text-blue-800' },
    warm: { variant: 'secondary', className: 'bg-orange-100 text-orange-800' },
    hot: { variant: 'default', className: 'bg-red-100 text-red-800' },
    resident: { variant: 'default', className: 'bg-purple-100 text-purple-800' },
  },
  training: {
    pending: { variant: 'outline', className: '', icon: Clock },
    queued: { variant: 'outline', className: '', icon: Clock },
    running: { variant: 'secondary', className: 'bg-blue-100 text-blue-800', icon: Loader2 },
    paused: { variant: 'secondary', className: 'bg-yellow-100 text-yellow-800', icon: Pause },
    completed: { variant: 'default', className: 'bg-green-100 text-green-800', icon: CheckCircle },
    failed: { variant: 'destructive', className: '', icon: XCircle },
    cancelled: { variant: 'outline', className: 'bg-gray-100 text-gray-800', icon: AlertTriangle },
  },
  policy: {
    passed: { variant: 'default', className: 'bg-green-100 text-green-800', icon: CheckCircle },
    failed: { variant: 'destructive', className: '', icon: XCircle },
    warning: { variant: 'secondary', className: 'bg-yellow-100 text-yellow-800', icon: AlertTriangle },
    pending: { variant: 'outline', className: '', icon: Clock },
    skipped: { variant: 'outline', className: 'bg-gray-100 text-gray-800' },
  },
  document: {
    processing: { variant: 'secondary', className: 'bg-blue-100 text-blue-800', icon: Loader2 },
    ready: { variant: 'default', className: 'bg-green-100 text-green-800', icon: CheckCircle },
    error: { variant: 'destructive', className: '', icon: XCircle },
    pending: { variant: 'outline', className: '', icon: Clock },
  },
  federation: {
    online: { variant: 'default', className: 'bg-green-100 text-green-800', icon: CheckCircle },
    offline: { variant: 'destructive', className: '', icon: XCircle },
    syncing: { variant: 'secondary', className: 'bg-blue-100 text-blue-800', icon: Loader2 },
    degraded: { variant: 'secondary', className: 'bg-yellow-100 text-yellow-800', icon: AlertTriangle },
  },
  generic: {
    success: { variant: 'default', className: 'bg-green-100 text-green-800', icon: CheckCircle },
    error: { variant: 'destructive', className: '', icon: XCircle },
    warning: { variant: 'secondary', className: 'bg-yellow-100 text-yellow-800', icon: AlertTriangle },
    info: { variant: 'secondary', className: 'bg-blue-100 text-blue-800' },
    pending: { variant: 'outline', className: '', icon: Clock },
    loading: { variant: 'secondary', className: '', icon: Loader2 },
  },
};

export interface StatusBadgeProps {
  /** The status value to display */
  status: string;
  /** Context determines the color scheme */
  context?: StatusContext;
  /** Show status icon */
  showIcon?: boolean;
  /** Additional CSS classes */
  className?: string;
  /** Override the displayed label */
  label?: string;
  /** Size variant */
  size?: 'sm' | 'md' | 'lg';
}

/**
 * StatusBadge - Unified component for displaying status across the application
 */
export function StatusBadge({
  status,
  context = 'generic',
  showIcon = false,
  className,
  label,
  size = 'md',
}: StatusBadgeProps) {
  const normalizedStatus = status.toLowerCase();
  const contextConfigs = STATUS_CONFIGS[context] ?? STATUS_CONFIGS.generic;
  const config = contextConfigs[normalizedStatus] ?? {
    variant: 'outline' as const,
    className: 'bg-gray-100 text-gray-800',
  };

  const Icon = config.icon;
  const displayLabel = label ?? status.charAt(0).toUpperCase() + status.slice(1);

  const sizeClasses = {
    sm: 'text-xs px-1.5 py-0.5',
    md: 'text-xs px-2.5 py-0.5',
    lg: 'text-sm px-3 py-1',
  };

  return (
    <Badge
      variant={config.variant}
      className={cn(config.className, sizeClasses[size], className)}
    >
      {showIcon && Icon && (
        <Icon
          className={cn(
            'mr-1',
            size === 'sm' ? 'h-3 w-3' : 'h-4 w-4',
            normalizedStatus === 'running' || normalizedStatus === 'loading' || normalizedStatus === 'syncing' || normalizedStatus === 'processing'
              ? 'animate-spin'
              : ''
          )}
        />
      )}
      {displayLabel}
    </Badge>
  );
}

/**
 * Get status configuration for programmatic access
 */
export function getStatusConfig(
  status: string,
  context: StatusContext = 'generic'
): StatusConfig | null {
  const normalizedStatus = status.toLowerCase();
  const contextConfigs = STATUS_CONFIGS[context] ?? STATUS_CONFIGS.generic;
  return contextConfigs[normalizedStatus] ?? null;
}

export default StatusBadge;
