/**
 * ChatCurrentlyLoadedPanel - Display currently loaded stack context
 *
 * Shows stack information, adapters, knowledge base, and strength overrides.
 */

import React from 'react';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { AdapterAttachmentChip } from './AdapterAttachmentChip';
import type { SuggestedAdapter } from '@/contexts/ChatContext';

// ============================================================================
// Types
// ============================================================================

export interface AdapterListItem {
  id: string;
  name: string;
  tier?: string;
  domain?: string;
  strength: number;
}

export interface AttachedAdapter {
  id: string;
  confidence?: number;
}

export interface ChatCurrentlyLoadedPanelProps {
  /** Whether this is the default stack */
  isDefaultStack: boolean;
  /** Stack name/label */
  stackLabel: string;
  /** Stack details (lifecycle state or description) */
  stackDetails?: string | null;
  /** Number of adapters */
  adapterCount: number;
  /** Selected collection name */
  selectedCollectionName: string;
  /** Base model label */
  baseModelLabel: string;
  /** List of adapters in the stack */
  adapterList: AdapterListItem[];
  /** Adapter strength overrides */
  strengthOverrides: Record<string, number>;
  /** Handler for strength changes */
  onStrengthChange: (adapterId: string, value: number) => void;
  /** Attached adapters (via magnet) */
  attachedAdapters: AttachedAdapter[];
  /** Last attached adapter ID (for flash effect) */
  lastAttachedAdapterId?: string | null;
  /** Handler for removing attachment */
  onRemoveAttachment: (adapterId: string) => void;
  /** Whether to show context */
  showContext: boolean;
  /** Toggle context visibility */
  onToggleContext: () => void;
  /** Whether history sidebar is open (for margin adjustment) */
  isHistoryOpen: boolean;
  /** Whether right panels are open (for margin adjustment) */
  rightPanelsOpen: boolean;
}

// ============================================================================
// Component
// ============================================================================

export function ChatCurrentlyLoadedPanel({
  isDefaultStack,
  stackLabel,
  stackDetails,
  adapterCount,
  selectedCollectionName,
  baseModelLabel,
  adapterList,
  strengthOverrides,
  onStrengthChange,
  attachedAdapters,
  lastAttachedAdapterId,
  onRemoveAttachment,
  showContext,
  onToggleContext,
  isHistoryOpen,
  rightPanelsOpen,
}: ChatCurrentlyLoadedPanelProps) {
  return (
    <div className={`px-4 mt-2 ${isHistoryOpen ? 'ml-80' : ''} ${rightPanelsOpen ? 'mr-96' : ''}`}>
      <Card>
        <CardHeader className="flex flex-row items-start justify-between space-y-0">
          <div className="space-y-1">
            <CardTitle className="text-base">Currently Loaded</CardTitle>
            <p className="text-xs text-muted-foreground">
              Stack context for this chat session.
            </p>
            {isDefaultStack && (
              <Badge
                variant="secondary"
                className="w-fit"
                aria-label="This is the default adapter stack for your workspace"
              >
                Default stack for this workspace
              </Badge>
            )}
          </div>
          <Button
            variant="ghost"
            size="sm"
            onClick={onToggleContext}
            aria-label={showContext ? 'Hide stack context' : 'Show stack context'}
          >
            {showContext ? 'Hide' : 'Show'}
          </Button>
        </CardHeader>
        {showContext && (
          <CardContent className="grid grid-cols-1 sm:grid-cols-4 gap-3">
            <div>
              <p className="text-xs text-muted-foreground">Stack</p>
              <p className="font-medium truncate">{stackLabel}</p>
              {stackDetails && (
                <p className="text-xs text-muted-foreground truncate">{stackDetails}</p>
              )}
            </div>
            <div>
              <p className="text-xs text-muted-foreground">Adapters</p>
              <p className="font-medium">{adapterCount || '—'}</p>
            </div>
            <div>
              <p className="text-xs text-muted-foreground">Knowledge Base</p>
              <p className="font-medium truncate">{selectedCollectionName}</p>
            </div>
            <div>
              <p className="text-xs text-muted-foreground">Base model</p>
              <p className="font-medium text-muted-foreground">{baseModelLabel}</p>
            </div>
            <div className="sm:col-span-4 space-y-2">
              <div className="flex items-center justify-between">
                <p className="text-xs text-muted-foreground">Active adapters</p>
                <p className="text-xs text-muted-foreground">
                  Strength overrides (0.0–2.0, default 1.0)
                </p>
              </div>
              {adapterList.length === 0 ? (
                <p className="text-xs text-muted-foreground">No adapters selected</p>
              ) : (
                <div className="space-y-3">
                  {adapterList.map((adapter) => (
                    <div key={adapter.id} className="flex items-center gap-3">
                      <div className="flex-1 min-w-0">
                        <p className="text-sm font-medium truncate">{adapter.name}</p>
                        <p className="text-xs text-muted-foreground truncate">
                          {[adapter.tier, adapter.domain].filter(Boolean).join(' • ') || 'Adapter'}
                        </p>
                      </div>
                      <div className="flex items-center gap-2">
                        <input
                          type="range"
                          min={0}
                          max={2}
                          step={0.05}
                          value={strengthOverrides[adapter.id] ?? 1}
                          onChange={(e) => onStrengthChange(adapter.id, Number(e.target.value))}
                          aria-label={`Strength for ${adapter.name}`}
                          className="w-32"
                        />
                        <span className="text-xs tabular-nums">
                          {(strengthOverrides[adapter.id] ?? 1).toFixed(2)}x
                        </span>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
            {attachedAdapters.length > 0 && (
              <div className="sm:col-span-4">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="text-xs text-muted-foreground">Magnet attachments:</span>
                  {attachedAdapters.map((adapter) => (
                    <AdapterAttachmentChip
                      key={`${adapter.id}-active`}
                      adapterId={adapter.id}
                      confidence={adapter.confidence}
                      onRemove={() => onRemoveAttachment(adapter.id)}
                      flash={lastAttachedAdapterId === adapter.id}
                    />
                  ))}
                </div>
              </div>
            )}
          </CardContent>
        )}
      </Card>
    </div>
  );
}
