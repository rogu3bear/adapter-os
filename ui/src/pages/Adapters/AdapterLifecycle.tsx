// AdapterLifecycle - Lifecycle tab with state promotion/demotion controls
// Provides lifecycle management controls and state transition history

import React, { useState } from 'react';
import {
  TrendingUp,
  TrendingDown,
  Activity,
  AlertCircle,
  Clock,
  ArrowUp,
  ArrowDown,
  Info,
} from 'lucide-react';
import { formatDistanceToNow, parseISO } from 'date-fns';

import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Skeleton } from '@/components/ui/skeleton';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import { AdapterDetailResponse, LifecycleTransitionResponse, AdapterState } from '@/api/adapter-types';
import { getLifecycleVariant } from '@/utils/lifecycle';
import { toast } from 'sonner';
import { formatBytes, formatRelativeTime } from '@/lib/formatters';

interface AdapterLifecycleProps {
  adapterId: string;
  adapter: AdapterDetailResponse | null;
  onPromote: (reason: string) => Promise<LifecycleTransitionResponse>;
  onDemote: (reason: string) => Promise<LifecycleTransitionResponse>;
  isPromoting: boolean;
  isDemoting: boolean;
}

export default function AdapterLifecycle({
  adapterId,
  adapter,
  onPromote,
  onDemote,
  isPromoting,
  isDemoting,
}: AdapterLifecycleProps) {
  const [showPromoteDialog, setShowPromoteDialog] = useState(false);
  const [showDemoteDialog, setShowDemoteDialog] = useState(false);
  const [reason, setReason] = useState('');

  if (!adapter) {
    return <LifecycleSkeleton />;
  }

  const currentState = adapter.current_state || adapter.adapter?.current_state || 'unknown';
  const lifecycleState = adapter.adapter?.lifecycle_state || 'active';

  // Handle promote
  const handlePromote = async () => {
    if (!reason.trim()) {
      toast.error('Please provide a reason for promotion');
      return;
    }

    try {
      const result = await onPromote(reason);
      toast.success(`Adapter promoted: ${result.from_state} → ${result.to_state}`);
      setShowPromoteDialog(false);
      setReason('');
    } catch (err) {
      toast.error(`Failed to promote: ${err instanceof Error ? err.message : 'Unknown error'}`);
    }
  };

  // Handle demote
  const handleDemote = async () => {
    if (!reason.trim()) {
      toast.error('Please provide a reason for demotion');
      return;
    }

    try {
      const result = await onDemote(reason);
      toast.success(`Adapter demoted: ${result.from_state} → ${result.to_state}`);
      setShowDemoteDialog(false);
      setReason('');
    } catch (err) {
      toast.error(`Failed to demote: ${err instanceof Error ? err.message : 'Unknown error'}`);
    }
  };

  // State transition info
  const stateTransitions: Record<AdapterState, { next?: AdapterState; prev?: AdapterState }> = {
    unloaded: { next: 'cold' },
    cold: { next: 'warm', prev: 'unloaded' },
    warm: { next: 'hot', prev: 'cold' },
    hot: { next: 'resident', prev: 'warm' },
    resident: { prev: 'hot' },
    loading: {},
    error: {},
  };

  const canPromote = !!stateTransitions[currentState as AdapterState]?.next;
  const canDemote = !!stateTransitions[currentState as AdapterState]?.prev;
  const nextState = stateTransitions[currentState as AdapterState]?.next;
  const prevState = stateTransitions[currentState as AdapterState]?.prev;

  return (
    <div className="space-y-6">
      {/* Current State */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Activity className="h-5 w-5" />
            Current State
            <GlossaryTooltip brief="The current lifecycle state of the adapter in the memory hierarchy" />
          </CardTitle>
          <CardDescription>Adapter state in the lifecycle hierarchy</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="flex items-center justify-between">
            <div>
              <div className="text-sm text-muted-foreground mb-2">Lifecycle State</div>
              <Badge variant={getLifecycleVariant(lifecycleState)} className="text-lg px-4 py-2">
                {lifecycleState}
              </Badge>
            </div>
            <div className="text-right">
              <div className="text-sm text-muted-foreground mb-2">Memory State</div>
              <Badge variant="outline" className="text-lg px-4 py-2">
                {currentState}
              </Badge>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* State Transition Controls */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <TrendingUp className="h-5 w-5" />
            State Transitions
            <GlossaryTooltip brief="Manually promote or demote the adapter in the lifecycle hierarchy" />
          </CardTitle>
          <CardDescription>Manage adapter lifecycle state transitions</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          {/* Lifecycle Diagram */}
          <div className="bg-muted rounded-md p-6">
            <div className="flex items-center justify-between">
              <StateNode state="unloaded" current={currentState === 'unloaded'} />
              <ArrowRight />
              <StateNode state="cold" current={currentState === 'cold'} />
              <ArrowRight />
              <StateNode state="warm" current={currentState === 'warm'} />
              <ArrowRight />
              <StateNode state="hot" current={currentState === 'hot'} />
              <ArrowRight />
              <StateNode state="resident" current={currentState === 'resident'} />
            </div>
          </div>

          {/* Action Buttons */}
          <div className="flex gap-4">
            <Button
              onClick={() => setShowPromoteDialog(true)}
              disabled={!canPromote || isPromoting || isDemoting}
              className="flex-1"
            >
              <ArrowUp className="h-4 w-4 mr-2" />
              Promote to {nextState || 'N/A'}
            </Button>
            <Button
              onClick={() => setShowDemoteDialog(true)}
              disabled={!canDemote || isPromoting || isDemoting}
              variant="outline"
              className="flex-1"
            >
              <ArrowDown className="h-4 w-4 mr-2" />
              Demote to {prevState || 'N/A'}
            </Button>
          </div>

          {/* State Descriptions */}
          <Alert>
            <Info className="h-4 w-4" />
            <AlertTitle>State Information</AlertTitle>
            <AlertDescription>
              <ul className="mt-2 space-y-1 text-sm">
                <li><strong>Not Loaded:</strong> Adapter not in memory, requires loading before use</li>
                <li><strong>Ready:</strong> Metadata loaded, ready to be activated</li>
                <li><strong>Standby:</strong> Weights loaded in RAM, ready for quick activation</li>
                <li><strong>Loaded:</strong> Active on GPU, serving requests</li>
                <li><strong>Pinned:</strong> Protected in memory, will not be removed</li>
              </ul>
            </AlertDescription>
          </Alert>
        </CardContent>
      </Card>

      {/* Activation Metrics */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Activity className="h-5 w-5" />
            Activation Metrics
            <GlossaryTooltip brief="Usage statistics that influence lifecycle transitions" />
          </CardTitle>
          <CardDescription>Metrics affecting lifecycle state changes</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            <MetricItem
              label="Activation Count"
              value={adapter.adapter?.activation_count ?? adapter.activation_count ?? 0}
              description="Total times adapter was selected by router"
            />
            <MetricItem
              label="Memory Usage"
              value={formatBytes(adapter.adapter?.memory_bytes ?? adapter.memory_bytes ?? 0)}
              description="Current memory footprint"
            />
            <MetricItem
              label="Last Activated"
              value={formatTime(adapter.adapter?.last_activated ?? adapter.last_activated)}
              description="Time since last use"
            />
          </div>
        </CardContent>
      </Card>

      {/* Promotion/Demotion Guidelines */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <AlertCircle className="h-5 w-5" />
            Transition Guidelines
          </CardTitle>
          <CardDescription>Best practices for manual state transitions</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="space-y-3">
            <GuidelineItem
              type="promote"
              title="When to Promote"
              items={[
                'Adapter is frequently used and needs faster access',
                'Expected increase in usage for this adapter',
                'Preventing eviction under memory pressure',
                'Ensuring critical adapter stays loaded',
              ]}
            />
            <GuidelineItem
              type="demote"
              title="When to Demote"
              items={[
                'Adapter usage has decreased significantly',
                'Freeing memory for other adapters',
                'Testing adapter performance at lower tiers',
                'Temporary reduction in adapter priority',
              ]}
            />
          </div>
        </CardContent>
      </Card>

      {/* Promote Dialog */}
      <Dialog open={showPromoteDialog} onOpenChange={setShowPromoteDialog}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Promote Adapter to {nextState}</DialogTitle>
            <DialogDescription>
              This will promote the adapter from <strong>{currentState}</strong> to <strong>{nextState}</strong>.
              Please provide a reason for this transition.
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4 py-4">
            <div className="space-y-2">
              <Label htmlFor="promote-reason">Reason</Label>
              <Textarea
                id="promote-reason"
                placeholder="e.g., Increased usage expected for upcoming project"
                value={reason}
                onChange={(e) => setReason(e.target.value)}
                rows={4}
              />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setShowPromoteDialog(false)} disabled={isPromoting}>
              Cancel
            </Button>
            <Button onClick={handlePromote} disabled={isPromoting || !reason.trim()}>
              {isPromoting ? 'Promoting...' : 'Promote'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Demote Dialog */}
      <Dialog open={showDemoteDialog} onOpenChange={setShowDemoteDialog}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Demote Adapter to {prevState}</DialogTitle>
            <DialogDescription>
              This will demote the adapter from <strong>{currentState}</strong> to <strong>{prevState}</strong>.
              Please provide a reason for this transition.
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4 py-4">
            <div className="space-y-2">
              <Label htmlFor="demote-reason">Reason</Label>
              <Textarea
                id="demote-reason"
                placeholder="e.g., Reduced usage, freeing memory for other adapters"
                value={reason}
                onChange={(e) => setReason(e.target.value)}
                rows={4}
              />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setShowDemoteDialog(false)} disabled={isDemoting}>
              Cancel
            </Button>
            <Button onClick={handleDemote} disabled={isDemoting || !reason.trim()}>
              {isDemoting ? 'Demoting...' : 'Demote'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

// Arrow component for lifecycle diagram
function ArrowRight() {
  return <div className="text-muted-foreground">→</div>;
}

// State node component for lifecycle diagram
interface StateNodeProps {
  state: string;
  current: boolean;
}

function StateNode({ state, current }: StateNodeProps) {
  return (
    <div
      className={`
        px-3 py-2 rounded-md border-2 text-sm font-medium transition-colors
        ${current
          ? 'border-primary bg-primary text-primary-foreground'
          : 'border-muted bg-background text-muted-foreground'
        }
      `}
    >
      {state}
    </div>
  );
}

// Metric item component
interface MetricItemProps {
  label: string;
  value: string | number;
  description: string;
}

function MetricItem({ label, value, description }: MetricItemProps) {
  return (
    <div className="space-y-1 p-4 border rounded-md">
      <div className="text-sm text-muted-foreground">{label}</div>
      <div className="text-2xl font-bold">{value}</div>
      <div className="text-xs text-muted-foreground">{description}</div>
    </div>
  );
}

// Guideline item component
interface GuidelineItemProps {
  type: 'promote' | 'demote';
  title: string;
  items: string[];
}

function GuidelineItem({ type, title, items }: GuidelineItemProps) {
  const Icon = type === 'promote' ? TrendingUp : TrendingDown;
  const colorClass = type === 'promote' ? 'text-green-500' : 'text-orange-500';

  return (
    <div className="space-y-2">
      <div className={`flex items-center gap-2 font-medium ${colorClass}`}>
        <Icon className="h-4 w-4" />
        {title}
      </div>
      <ul className="list-disc list-inside space-y-1 text-sm text-muted-foreground pl-6">
        {items.map((item, idx) => (
          <li key={idx}>{item}</li>
        ))}
      </ul>
    </div>
  );
}

// Helper function
function formatTime(timestamp: string | undefined): string {
  if (!timestamp) return 'Never';
  try {
    return formatRelativeTime(timestamp);
  } catch {
    return timestamp;
  }
}

// Skeleton for loading state
function LifecycleSkeleton() {
  return (
    <div className="space-y-6">
      <Card>
        <CardHeader>
          <Skeleton className="h-6 w-48" />
          <Skeleton className="h-4 w-64" />
        </CardHeader>
        <CardContent>
          <div className="flex justify-between">
            <Skeleton className="h-12 w-32" />
            <Skeleton className="h-12 w-32" />
          </div>
        </CardContent>
      </Card>
      <Card>
        <CardHeader>
          <Skeleton className="h-6 w-48" />
          <Skeleton className="h-4 w-64" />
        </CardHeader>
        <CardContent className="space-y-4">
          <Skeleton className="h-24 w-full" />
          <div className="flex gap-4">
            <Skeleton className="h-10 flex-1" />
            <Skeleton className="h-10 flex-1" />
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
