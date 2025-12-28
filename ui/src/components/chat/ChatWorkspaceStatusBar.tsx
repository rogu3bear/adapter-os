/**
 * ChatWorkspaceStatusBar - Display workspace status badges
 *
 * Shows current workspace, base model, adapter count, plan, and manifest
 * information in a compact badge row.
 */

import React from 'react';
import { Badge } from '@/components/ui/badge';

// ============================================================================
// Types
// ============================================================================

export interface ChatWorkspaceStatusBarProps {
  /** Current tenant/workspace ID */
  tenantId: string;
  /** Base model descriptor */
  baseModelDescriptor: string;
  /** Whether base model is ready */
  baseModelReady: boolean;
  /** Number of adapters in stack */
  adapterCount: number;
  /** Active plan ID */
  activePlanId?: string | null;
  /** Manifest hash */
  manifestHashB3?: string | null;
  /** Whether history sidebar is open (for margin adjustment) */
  isHistoryOpen: boolean;
  /** Whether right panels are open (for margin adjustment) */
  rightPanelsOpen: boolean;
  /** Data source: 'canonical', 'legacy', or null */
  dataSource?: 'canonical' | 'legacy' | null;
  /** True when data is from legacy endpoint */
  isLegacy?: boolean;
  /** True when model status is from fetch error (not actual no-model) */
  isModelFetchError?: boolean;
}

// ============================================================================
// Component
// ============================================================================

export function ChatWorkspaceStatusBar({
  tenantId,
  baseModelDescriptor,
  baseModelReady,
  adapterCount,
  activePlanId,
  manifestHashB3,
  isHistoryOpen,
  rightPanelsOpen,
  dataSource,
  isLegacy,
  isModelFetchError,
}: ChatWorkspaceStatusBarProps) {
  // Determine model status display
  const getModelStatusSuffix = (): string => {
    if (isModelFetchError) return ' (status unknown)';
    if (!baseModelReady) return ' (not loaded)';
    return '';
  };

  return (
    <div className={`px-4 pt-2 transition-all ${isHistoryOpen ? 'ml-80' : ''} ${rightPanelsOpen ? 'mr-96' : ''}`}>
      <div className="flex flex-wrap items-center gap-2 text-xs">
        <Badge variant="secondary">Workspace: {tenantId}</Badge>
        <Badge variant={baseModelReady && !isModelFetchError ? 'outline' : 'destructive'}>
          Base: {baseModelDescriptor}
          {getModelStatusSuffix()}
        </Badge>
        <Badge variant="outline">Adapters: {adapterCount || 0}</Badge>
        <Badge variant="outline">Plan: {activePlanId || 'none'}</Badge>
        {manifestHashB3 && (
          <Badge variant="outline">Manifest {manifestHashB3}</Badge>
        )}
        {isLegacy && (
          <Badge variant="secondary" className="text-amber-600">
            (legacy endpoint)
          </Badge>
        )}
      </div>
    </div>
  );
}
