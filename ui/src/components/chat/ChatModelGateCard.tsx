/**
 * ChatModelGateCard - Display model gate warning and load controls
 *
 * Shows when base model is not ready and provides load/bypass options.
 */

import React from 'react';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Switch } from '@/components/ui/switch';
import { Loader2, RefreshCw } from 'lucide-react';

// ============================================================================
// Types
// ============================================================================

export interface ChatModelGateCardProps {
  /** Whether workspace state is loading */
  workspaceStateLoading: boolean;
  /** Active base model ID from workspace */
  activeBaseModelId?: string | null;
  /** Current base model status label */
  baseModelLabel: string;
  /** Whether models are currently loading */
  isLoadingModels: boolean;
  /** Whether bypass is allowed (developer/kernel mode) */
  canBypassModelGate: boolean;
  /** Current bypass state */
  modelGateBypass: boolean;
  /** Set bypass state */
  onBypassChange: (enabled: boolean) => void;
  /** Trigger model load */
  onLoadModel: () => void;
  /** Refresh workspace state */
  onRefresh: () => void;
  /** Whether history sidebar is open (for margin adjustment) */
  isHistoryOpen: boolean;
  /** Whether right panels are open (for margin adjustment) */
  rightPanelsOpen: boolean;
}

// ============================================================================
// Component
// ============================================================================

export function ChatModelGateCard({
  workspaceStateLoading,
  activeBaseModelId,
  baseModelLabel,
  isLoadingModels,
  canBypassModelGate,
  modelGateBypass,
  onBypassChange,
  onLoadModel,
  onRefresh,
  isHistoryOpen,
  rightPanelsOpen,
}: ChatModelGateCardProps) {
  return (
    <div className={`px-4 pt-2 transition-all ${isHistoryOpen ? 'ml-80' : ''} ${rightPanelsOpen ? 'mr-96' : ''}`}>
      <Card className="border-amber-500/70 bg-amber-50/40 dark:bg-amber-950/20">
        <CardHeader className="pb-2 flex items-start justify-between">
          <div>
            <CardTitle className="text-base">Base model required</CardTitle>
            <p className="text-xs text-muted-foreground">
              Load an active base model before running chat. Workspace guard prevents accidental runs.
            </p>
          </div>
          {workspaceStateLoading ? (
            <Loader2 className="h-4 w-4 animate-spin text-amber-600" />
          ) : activeBaseModelId ? (
            <Badge variant="outline" className="text-xs">
              Target: {activeBaseModelId}
            </Badge>
          ) : null}
        </CardHeader>
        <CardContent className="flex flex-col gap-2">
          <div className="flex flex-wrap items-center gap-2">
            <Button size="sm" onClick={onLoadModel} disabled={isLoadingModels}>
              Load base model
            </Button>
            <Button
              size="sm"
              variant="ghost"
              onClick={onRefresh}
              disabled={workspaceStateLoading}
              className="gap-2"
            >
              <RefreshCw className={`h-4 w-4 ${workspaceStateLoading ? 'animate-spin' : ''}`} />
              Refresh
            </Button>
            {canBypassModelGate && (
              <div className="flex items-center gap-2">
                <Switch
                  id="developer-bypass"
                  checked={modelGateBypass}
                  onCheckedChange={onBypassChange}
                />
                <label htmlFor="developer-bypass" className="text-xs text-muted-foreground">
                  Dev bypass
                </label>
              </div>
            )}
          </div>
          <div className="text-xs text-muted-foreground">
            Status: {baseModelLabel}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
