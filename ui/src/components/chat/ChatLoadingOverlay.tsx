/**
 * ChatLoadingOverlay - Full-page loading display for adapter initialization
 *
 * Provides a prominent overlay with progress tracking, status cards,
 * and action buttons when adapters need to be loaded before chat.
 */

import * as React from 'react';
import { Loader2, CheckCircle, XCircle, AlertTriangle, Server, Cpu, MemoryStick, Activity, CircleOff } from 'lucide-react';
import { cn } from '@/lib/utils';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Progress } from '@/components/ui/progress';
import type { AdapterLoadingItem } from './AdapterLoadingProgress';

// ============================================================================
// Types
// ============================================================================

export interface ChatLoadingOverlayProps {
  /** Loading state with adapter details */
  loadingState: {
    adapters: AdapterLoadingItem[];
    overallProgress: number;
    estimatedTimeRemaining?: number;
  };

  /** Called when user clicks "Load and Chat" button */
  onLoadAll: () => void;

  /** Called when user cancels loading */
  onCancel: () => void;

  /** Optional kernel + backend snapshot to surface during boot */
  kernelInfo?: {
    workerName?: string | null;
    workerStatus?: string | null;
    backend?: string | null;
    backendMode?: string | null;
    baseModelName?: string | null;
    vramUsedMb?: number | null;
    vramTotalMb?: number | null;
    bootProgress?: number | null;
  };

  /** Additional class names */
  className?: string;
}

// ============================================================================
// Lifecycle State Icons
// ============================================================================

const STATE_ICONS: Record<string, React.ElementType> = {
  ready: CheckCircle,
  loading: Loader2,
  failed: XCircle,
  pending: CircleOff,
};

const STATE_COLORS: Record<string, string> = {
  ready: 'text-green-600',
  loading: 'text-blue-600',
  failed: 'text-red-600',
  pending: 'text-gray-400',
};

const STATE_BG_COLORS: Record<string, string> = {
  ready: 'bg-green-50 border-green-200',
  loading: 'bg-blue-50 border-blue-200',
  failed: 'bg-red-50 border-red-200',
  pending: 'bg-gray-50 border-gray-200',
};

// ============================================================================
// Helper Functions
// ============================================================================

function formatTime(seconds: number): string {
  if (seconds < 60) {
    return `${seconds}s`;
  }
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = seconds % 60;
  return `${minutes}m ${remainingSeconds}s`;
}

// ============================================================================
// Component
// ============================================================================

export function ChatLoadingOverlay({
  loadingState,
  onLoadAll,
  onCancel,
  kernelInfo,
  className,
}: ChatLoadingOverlayProps) {
  const { adapters, overallProgress, estimatedTimeRemaining } = loadingState;

  // Calculate status counts
  const readyCount = adapters.filter((a) => a.status === 'ready').length;
  const failedCount = adapters.filter((a) => a.status === 'failed').length;
  const loadingCount = adapters.filter((a) => a.status === 'loading').length;
  const totalCount = adapters.length;
  const allReady = readyCount === totalCount;
  const hasFailed = failedCount > 0;

  // Detect reduced motion preference
  const prefersReducedMotion = React.useMemo(() => {
    if (typeof window === 'undefined') return false;
    return window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  }, []);

  // Calculate circumference for circular progress (radius = 54)
  const radius = 54;
  const circumference = 2 * Math.PI * radius;
  const strokeDashoffset = circumference - (overallProgress / 100) * circumference;

  const vramPercent = kernelInfo?.vramUsedMb !== undefined && kernelInfo?.vramUsedMb !== null
    && kernelInfo?.vramTotalMb
    ? Math.min(100, (kernelInfo.vramUsedMb / kernelInfo.vramTotalMb) * 100)
    : null;

  const formatVram = (mb?: number | null) => {
    if (mb === undefined || mb === null) return '–';
    if (mb >= 1024) {
      return `${(mb / 1024).toFixed(1)} GB`;
    }
    return `${mb.toFixed(0)} MB`;
  };

  return (
    <div
      className={cn(
        'fixed inset-0 z-50 flex items-center justify-center',
        'bg-black/50 backdrop-blur-sm',
        'animate-in fade-in-0 duration-200',
        className
      )}
      role="dialog"
      aria-modal="true"
      aria-labelledby="loading-overlay-title"
      aria-describedby="loading-overlay-description"
    >
      <div className="bg-background/95 backdrop-blur-md rounded-lg border shadow-lg p-8 max-w-2xl w-full mx-4 max-h-[90vh] overflow-auto">
        {kernelInfo && (
          <div className="mb-6 space-y-3">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2 text-sm font-semibold uppercase tracking-wide text-muted-foreground">
                <Activity className="h-4 w-4 text-green-600" />
                Kernel Boot
              </div>
              {kernelInfo.bootProgress !== undefined && kernelInfo.bootProgress !== null && (
                <Badge variant="secondary" className="text-[11px]">
                  {(kernelInfo.bootProgress).toFixed(0)}% synced
                </Badge>
              )}
            </div>
            <div className="grid gap-3 md:grid-cols-3">
              <div className="rounded-md border bg-muted/40 p-3">
                <div className="flex items-center justify-between gap-2">
                  <div className="flex items-center gap-2 text-xs font-medium text-muted-foreground">
                    <Server className="h-4 w-4 text-foreground" />
                    adapteros-lora-worker
                  </div>
                  {kernelInfo.workerStatus && (
                    <Badge variant="outline" className="text-[11px]">
                      {kernelInfo.workerStatus}
                    </Badge>
                  )}
                </div>
                <div className="mt-2 text-sm font-mono">
                  {kernelInfo.workerName || 'Detecting worker'}
                </div>
              </div>

              <div className="rounded-md border bg-muted/40 p-3">
                <div className="flex items-center justify-between gap-2">
                  <div className="flex items-center gap-2 text-xs font-medium text-muted-foreground">
                    <Cpu className="h-4 w-4 text-foreground" />
                    Backend
                  </div>
                  {kernelInfo.backendMode && (
                    <Badge variant="outline" className="text-[11px]">
                      {kernelInfo.backendMode}
                    </Badge>
                  )}
                </div>
                <div className="mt-2 text-sm font-mono">
                  {kernelInfo.backend || 'Auto-select'}
                </div>
                <div className="text-xs text-muted-foreground">
                  Base model: {kernelInfo.baseModelName || 'Loading…'}
                </div>
              </div>

              <div className="rounded-md border bg-muted/40 p-3">
                <div className="flex items-center justify-between gap-2">
                  <div className="flex items-center gap-2 text-xs font-medium text-muted-foreground">
                    <MemoryStick className="h-4 w-4 text-foreground" />
                    VRAM
                  </div>
                  {vramPercent !== null && (
                    <Badge variant="outline" className="text-[11px]">
                      {vramPercent.toFixed(0)}%
                    </Badge>
                  )}
                </div>
                <div className="mt-2 flex items-center gap-2">
                  <div className="text-sm font-mono">
                    {formatVram(kernelInfo.vramUsedMb)}
                    <span className="text-muted-foreground"> / {formatVram(kernelInfo.vramTotalMb)}</span>
                  </div>
                </div>
                {vramPercent !== null && (
                  <Progress value={vramPercent} className="mt-2 h-2" />
                )}
              </div>
            </div>
          </div>
        )}

        {/* Header with circular progress */}
        <div className="flex flex-col items-center mb-6">
          {/* Circular progress ring */}
          <div className="relative mb-4">
            <svg
              className="transform -rotate-90"
              width="120"
              height="120"
              aria-hidden="true"
            >
              {/* Background circle */}
              <circle
                cx="60"
                cy="60"
                r={radius}
                stroke="currentColor"
                strokeWidth="8"
                fill="none"
                className="text-gray-200"
              />
              {/* Progress circle */}
              <circle
                cx="60"
                cy="60"
                r={radius}
                stroke="currentColor"
                strokeWidth="8"
                fill="none"
                strokeDasharray={circumference}
                strokeDashoffset={strokeDashoffset}
                strokeLinecap="round"
                className={cn(
                  allReady ? 'text-green-600' : hasFailed ? 'text-amber-600' : 'text-blue-600',
                  !prefersReducedMotion && 'transition-all duration-300 ease-in-out'
                )}
              />
            </svg>
            {/* Center icon/percentage */}
            <div className="absolute inset-0 flex items-center justify-center">
              {allReady ? (
                <CheckCircle className="h-12 w-12 text-green-600" aria-hidden="true" />
              ) : hasFailed ? (
                <AlertTriangle className="h-12 w-12 text-amber-600" aria-hidden="true" />
              ) : (
                <div className="flex flex-col items-center">
                  <Loader2
                    className={cn(
                      'h-8 w-8 text-blue-600',
                      !prefersReducedMotion && 'animate-spin'
                    )}
                    aria-hidden="true"
                  />
                  <span className="text-2xl font-bold text-foreground mt-1">
                    {overallProgress}%
                  </span>
                </div>
              )}
            </div>
          </div>

          {/* Progress bar (accessibility alternative) */}
          <div
            role="progressbar"
            aria-valuenow={overallProgress}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-label={`Loading adapters: ${overallProgress}% complete`}
            className="sr-only"
          >
            {overallProgress}% complete
          </div>

          {/* Title */}
          <h2
            id="loading-overlay-title"
            className={cn(
              'text-xl font-semibold mb-1',
              allReady ? 'text-green-700' : hasFailed ? 'text-amber-700' : 'text-foreground'
            )}
          >
            {allReady
              ? 'All Adapters Ready!'
              : hasFailed
              ? 'Some Adapters Failed'
              : 'Loading models...'}
          </h2>

          {/* Description */}
          <p
            id="loading-overlay-description"
            className="text-sm text-muted-foreground text-center"
          >
            {allReady
              ? `${readyCount} adapter${readyCount > 1 ? 's' : ''} ready for inference`
              : hasFailed
              ? `${failedCount} failed, ${readyCount} ready, ${loadingCount} loading`
              : `${readyCount}/${totalCount} adapters ready`}
          </p>

          {/* Estimated time remaining */}
          {!allReady && estimatedTimeRemaining && estimatedTimeRemaining > 0 && (
            <p className="text-xs text-muted-foreground mt-1">
              Estimated time remaining: {formatTime(estimatedTimeRemaining)}
            </p>
          )}
        </div>

        {/* Adapter status cards */}
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 mb-6">
          {adapters.map((adapter) => {
            const Icon = STATE_ICONS[adapter.status];
            const colorClass = STATE_COLORS[adapter.status];
            const bgClass = STATE_BG_COLORS[adapter.status];

            return (
              <div
                key={adapter.id}
                className={cn(
                  'flex items-center justify-between px-3 py-2.5 rounded-md border',
                  bgClass
                )}
              >
                <div className="flex items-center gap-2.5 min-w-0">
                  {Icon && (
                    <Icon
                      className={cn(
                        'h-5 w-5 flex-shrink-0',
                        colorClass,
                        adapter.status === 'loading' &&
                          !prefersReducedMotion &&
                          'animate-spin'
                      )}
                      aria-hidden="true"
                    />
                  )}
                  <div className="min-w-0 flex-1">
                    <span className="text-sm font-medium block truncate">
                      {adapter.name}
                    </span>
                    {adapter.error && (
                      <span className="text-xs text-red-600 block truncate">
                        {adapter.error}
                      </span>
                    )}
                  </div>
                </div>

                <div className="flex items-center gap-2 flex-shrink-0 ml-2">
                  {adapter.status === 'loading' && adapter.progress !== undefined && (
                    <span className="text-xs text-blue-600 font-medium">
                      {adapter.progress}%
                    </span>
                  )}
                  {adapter.status === 'loading' &&
                    adapter.estimatedTimeRemaining !== undefined && (
                      <span className="text-xs text-muted-foreground">
                        {formatTime(adapter.estimatedTimeRemaining)}
                      </span>
                    )}
                  {adapter.status === 'ready' && (
                    <span className="text-xs text-green-600 font-medium">Ready</span>
                  )}
                  {adapter.status === 'pending' && (
                    <span className="text-xs text-muted-foreground">Pending</span>
                  )}
                </div>
              </div>
            );
          })}
        </div>

        {/* Action buttons */}
        <div className="flex flex-col sm:flex-row gap-2 justify-end">
          <Button
            variant="outline"
            onClick={onCancel}
            className="sm:w-auto w-full"
            disabled={allReady}
          >
            Cancel
          </Button>
          <Button
            onClick={onLoadAll}
            className="sm:w-auto w-full"
            disabled={allReady || loadingCount > 0}
          >
            {allReady ? 'Ready to Chat' : 'Load and Chat'}
          </Button>
        </div>
      </div>
    </div>
  );
}

export default ChatLoadingOverlay;
