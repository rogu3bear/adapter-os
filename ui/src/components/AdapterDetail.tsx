// Adapter Detail View with Lineage Visualization
//
// Displays comprehensive adapter information including:
// - Core metadata (name, state, tier, memory, activations)
// - Lineage tree (ancestors and descendants)
// - Fork information (parent, children, fork type/reason)
// - Lifecycle controls (promote, demote)

import React, { useEffect, useState } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import { apiClient } from '@/api/services';
import type { AdapterDetailResponse, AdapterLineageResponse, AdapterLineageNode } from '@/api/types';
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from './ui/card';
import { Slider } from './ui/slider';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter, DialogDescription } from './ui/dialog';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { useToast } from '@/hooks/use-toast';
import { ArrowUp, ArrowDown, GitBranch, Clock, Database, Activity, Layers, ExternalLink, Plus, Loader2 } from 'lucide-react';
import { getLifecycleVariant } from '@/utils/lifecycle';
import { AddToStackModal } from './AddToStackModal';
import { useAdapterStacks } from '@/hooks/admin/useAdmin';
import { LIFECYCLE_STATE_LABELS } from '@/constants/terminology';
import { formatMB, formatCount } from '@/utils';
import { buildAdaptersListLink, buildAdapterDetailLink } from '@/utils/navLinks';

export const AdapterDetail: React.FC = () => {
  const { adapterId } = useParams<{ adapterId: string }>();
  const navigate = useNavigate();
  const { toast } = useToast();

  const [adapter, setAdapter] = useState<AdapterDetailResponse | null>(null);
  const [lineage, setLineage] = useState<AdapterLineageResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [strength, setStrength] = useState<number | null>(null);
  const [isUpdatingStrength, setIsUpdatingStrength] = useState(false);

  // Dialog state
  const [showPromoteDialog, setShowPromoteDialog] = useState(false);
  const [showDemoteDialog, setShowDemoteDialog] = useState(false);
  const [showAddToStackDialog, setShowAddToStackDialog] = useState(false);
  const [promoteReason, setPromoteReason] = useState('');
  const [demoteReason, setDemoteReason] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);

  // Fetch stacks to find which ones use this adapter
  const { data: stacks = [] } = useAdapterStacks();
  const stacksUsingAdapter = stacks.filter(stack => 
    stack.adapter_ids?.includes(adapterId || '')
  );

  useEffect(() => {
    if (!adapterId) return;

    const fetchData = async () => {
      try {
        setLoading(true);
        const [adapterData, lineageData] = await Promise.all([
          apiClient.getAdapterDetail(adapterId),
          apiClient.getAdapterLineage(adapterId),
        ]);
        setAdapter(adapterData);
        setStrength(adapterData.lora_strength ?? 1);
        setLineage(lineageData);
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to load adapter');
      } finally {
        setLoading(false);
      }
    };

    fetchData();
  }, [adapterId]);

  const handlePromoteSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!adapterId || !promoteReason.trim()) return;

    setIsSubmitting(true);
    try {
      await apiClient.promoteAdapterLifecycle(adapterId, promoteReason.trim());

      // Refresh data
      const adapterData = await apiClient.getAdapterDetail(adapterId);
      setAdapter(adapterData);

      toast({
        title: 'Adapter Promoted',
        description: `Successfully promoted adapter to next lifecycle state.`,
      });

      setShowPromoteDialog(false);
      setPromoteReason('');
    } catch (err) {
      toast({
        title: 'Promotion Failed',
        description: err instanceof Error ? err.message : 'Unknown error occurred',
        variant: 'destructive',
      });
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleDemoteSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!adapterId || !demoteReason.trim()) return;

    setIsSubmitting(true);
    try {
      await apiClient.demoteAdapterLifecycle(adapterId, demoteReason.trim());

      // Refresh data
      const adapterData = await apiClient.getAdapterDetail(adapterId);
      setAdapter(adapterData);

      toast({
        title: 'Adapter Demoted',
        description: `Successfully demoted adapter to previous lifecycle state.`,
      });

      setShowDemoteDialog(false);
      setDemoteReason('');
    } catch (err) {
      toast({
        title: 'Demotion Failed',
        description: err instanceof Error ? err.message : 'Unknown error occurred',
        variant: 'destructive',
      });
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleStrengthCommit = async (value: number) => {
    if (!adapterId) return;
    setIsUpdatingStrength(true);
    setStrength(value);
    try {
      const updated = await apiClient.updateAdapterStrength(adapterId, value);
      setAdapter(updated);
      toast({
        title: 'Strength updated',
        description: `LoRA strength set to ${value.toFixed(2)}`,
      });
    } catch (err) {
      toast({
        title: 'Update failed',
        description: err instanceof Error ? err.message : 'Unable to update strength',
        variant: 'destructive',
      });
    } finally {
      setIsUpdatingStrength(false);
    }
  };

  const getStateColor = (state: string): string => {
    const colors: Record<string, string> = {
      unloaded: 'gray',
      loading: 'blue',
      cold: 'blue',
      warm: 'yellow',
      hot: 'orange',
      resident: 'red',
      error: 'red',
    };
    return colors[state] || 'gray';
  };

  if (loading) {
    return <div className="flex items-center justify-center h-screen">Loading...</div>;
  }

  if (error || !adapter) {
    return (
      <div className="flex items-center justify-center h-screen">
        <Card className="w-96">
          <CardHeader>
            <CardTitle className="text-destructive">Error</CardTitle>
          </CardHeader>
          <CardContent>
            <p>{error || 'Adapter not found'}</p>
            <Button onClick={() => navigate(buildAdaptersListLink())} className="mt-4">
              Back to Adapters
            </Button>
          </CardContent>
        </Card>
      </div>
    );
  }

  const releaseState = adapter.lifecycle_state || adapter.adapter?.lifecycle_state;
  const runtimeState = adapter.runtime_state || adapter.current_state;

  return (
    <div className="container mx-auto p-6 space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-bold">{adapter.adapter_name || adapter.name}</h1>
          <p className="text-muted-foreground">
            {adapter.tenant_namespace && `${adapter.tenant_namespace}/`}
            {adapter.domain && `${adapter.domain}/`}
            {adapter.purpose}
            {adapter.revision && ` (${adapter.revision})`}
          </p>
        </div>
        <div className="flex gap-2">
          <Button onClick={() => setShowAddToStackDialog(true)} variant="default" size="sm">
            <Plus className="mr-2 h-4 w-4" />
            Add to Stack
          </Button>
          <Button onClick={() => setShowPromoteDialog(true)} variant="outline" size="sm">
            <ArrowUp className="mr-2 h-4 w-4" />
            Promote
          </Button>
          <Button onClick={() => setShowDemoteDialog(true)} variant="outline" size="sm">
            <ArrowDown className="mr-2 h-4 w-4" />
            Demote
          </Button>
        </div>
      </div>

      {/* Core Metrics */}
      <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
        <Card>
          <CardContent className="pt-6">
            <div className="flex items-center gap-2">
              <Activity className="h-5 w-5 text-muted-foreground" />
              <div>
                <p className="text-sm text-muted-foreground">Release State</p>
                <Badge variant={getLifecycleVariant(releaseState || 'draft')}>
                  {releaseState || 'draft'}
                </Badge>
                <p className="text-sm text-muted-foreground mt-2">Runtime State</p>
                <Badge className={`bg-${getStateColor(runtimeState || 'unloaded')}-500`}>
                  {LIFECYCLE_STATE_LABELS[runtimeState || 'unloaded'] || runtimeState || 'unloaded'}
                </Badge>
              </div>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="pt-6">
            <div className="flex items-center gap-2">
              <Database className="h-5 w-5 text-muted-foreground" />
              <div>
                <p className="text-sm text-muted-foreground">Memory</p>
                <p className="text-2xl font-bold">
                  {formatMB(adapter.memory_bytes)}
                </p>
              </div>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="pt-6">
            <div className="flex items-center gap-2">
              <Activity className="h-5 w-5 text-muted-foreground" />
              <div>
                <p className="text-sm text-muted-foreground">Activations</p>
                <p className="text-2xl font-bold">{formatCount(adapter.activation_count)}</p>
              </div>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="pt-6">
            <div className="flex items-center gap-2">
              <Clock className="h-5 w-5 text-muted-foreground" />
              <div>
                <p className="text-sm text-muted-foreground">Last Active</p>
                <p className="text-sm">
                  {adapter.last_activated
                    ? new Date(adapter.last_activated).toLocaleString()
                    : 'Never'}
                </p>
              </div>
            </div>
          </CardContent>
        </Card>
      </div>

      {/* Metadata */}
      <Card>
        <CardHeader>
          <CardTitle>Metadata</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-2 gap-4">
            <div>
              <p className="text-sm text-muted-foreground">Adapter ID</p>
              <p className="font-mono text-sm">{adapter.adapter.adapter_id}</p>
            </div>
            <div>
              <p className="text-sm text-muted-foreground">Hash (B3)</p>
              <p className="font-mono text-sm truncate">{adapter.hash_b3}</p>
            </div>
            <div>
              <p className="text-sm text-muted-foreground">Rank</p>
              <p>{adapter.rank}</p>
            </div>
            <div>
              <p className="text-sm text-muted-foreground">Alpha</p>
              <p>{adapter.alpha}</p>
            </div>
            <div className="space-y-2">
              <div className="flex items-center gap-2">
                <p className="text-sm text-muted-foreground">Strength</p>
                {isUpdatingStrength && <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />}
              </div>
              <p className="font-medium">{(strength ?? adapter.lora_strength ?? 1).toFixed(2)}</p>
              <Slider
                min={0.2}
                max={2}
                step={0.05}
                value={[strength ?? adapter.lora_strength ?? 1]}
                onValueChange={([value]) => setStrength(value)}
                onValueCommit={([value]) => handleStrengthCommit(value)}
              />
              <div className="flex gap-2 text-xs text-muted-foreground">
                <Button size="sm" variant="outline" onClick={() => handleStrengthCommit(0.4)}>Light</Button>
                <Button size="sm" variant="outline" onClick={() => handleStrengthCommit(0.7)}>Medium</Button>
                <Button size="sm" variant="outline" onClick={() => handleStrengthCommit(1.0)}>Strong</Button>
              </div>
            </div>
            <div>
              <p className="text-sm text-muted-foreground">Category</p>
              <p>{adapter.category}</p>
            </div>
            <div>
              <p className="text-sm text-muted-foreground">Scope</p>
              <p>{adapter.lora_scope ?? adapter.scope}</p>
            </div>
            {adapter.framework && (
              <div>
                <p className="text-sm text-muted-foreground">Framework</p>
                <p>{adapter.framework}</p>
              </div>
            )}
            <div>
              <p className="text-sm text-muted-foreground">LoRA Tier</p>
              <p>{adapter.lora_tier ?? adapter.tier}</p>
            </div>
            {adapter.adapter?.lifecycle_state && (
              <div>
                <p className="text-sm text-muted-foreground">Lifecycle State</p>
                <Badge variant={getLifecycleVariant(adapter.adapter.lifecycle_state)}>
                  {adapter.adapter.lifecycle_state}
                </Badge>
              </div>
            )}
            {adapter.adapter?.version && (
              <div>
                <p className="text-sm text-muted-foreground">Version</p>
                <p>{adapter.adapter.version}</p>
              </div>
            )}
          </div>
        </CardContent>
      </Card>

      {/* Stacks Using This Adapter */}
      {stacksUsingAdapter.length > 0 && (
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Layers className="h-5 w-5" />
              Stacks Using This Adapter ({stacksUsingAdapter.length})
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="space-y-2">
              {stacksUsingAdapter.map(stack => (
                <div
                  key={stack.id}
                  className="flex items-center justify-between p-3 border rounded-lg hover:bg-muted/50 cursor-pointer"
                  onClick={() => navigate(`/admin/stacks?stack=${stack.id}`)}
                >
                  <div className="flex-1">
                    <p className="font-medium">{stack.name}</p>
                    {stack.description && (
                      <p className="text-sm text-muted-foreground">{stack.description}</p>
                    )}
                  </div>
                  <Button variant="ghost" size="sm">
                    <ExternalLink className="h-4 w-4" />
                  </Button>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
      )}

      {/* Lineage */}
      {lineage && (
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <GitBranch className="h-5 w-5" />
              Lineage Tree ({lineage.total_nodes} nodes)
            </CardTitle>
          </CardHeader>
          <CardContent>
            {/* Ancestors */}
            {lineage.ancestors && lineage.ancestors.length > 0 && (
              <div className="mb-4">
                <h4 className="font-semibold mb-2">Ancestors</h4>
                <div className="space-y-2 pl-4 border-l-2 border-border">
                  {lineage.ancestors.map((ancestor) => (
                    <div
                      key={ancestor.adapter_id}
                      className="flex items-center justify-between p-2 bg-muted/50 rounded cursor-pointer hover:bg-muted"
                      onClick={() => navigate(buildAdapterDetailLink(ancestor.adapter_id))}
                    >
                      <div>
                        <p className="font-medium">{ancestor.adapter_name || ancestor.adapter_id}</p>
                        <p className="text-sm text-muted-foreground">{ancestor.revision}</p>
                      </div>
                      <Badge>{ancestor.current_state}</Badge>
                    </div>
                  ))}
                </div>
              </div>
            )}

            {/* Current Adapter */}
            {lineage.self_node && (
              <div className="my-4 p-4 bg-primary/10 border-2 border-primary rounded">
                <p className="font-bold text-primary">Current Adapter</p>
                <p className="text-sm text-primary/80">
                  {lineage.self_node.adapter_name || lineage.self_node.adapter_id}
                </p>
              </div>
            )}

            {/* Descendants */}
            {lineage.descendants && lineage.descendants.length > 0 && (
              <div>
                <h4 className="font-semibold mb-2">Descendants</h4>
                <div className="space-y-2 pl-4 border-l-2 border-border">
                  {lineage.descendants.map((descendant) => (
                    <div
                      key={descendant.adapter_id}
                      className="flex items-center justify-between p-2 bg-muted/50 rounded cursor-pointer hover:bg-muted"
                      onClick={() => navigate(buildAdapterDetailLink(descendant.adapter_id))}
                    >
                      <div>
                        <p className="font-medium">{descendant.adapter_name || descendant.adapter_id}</p>
                        <div className="flex items-center gap-2 text-sm text-muted-foreground">
                          <span>{descendant.revision}</span>
                          {descendant.fork_type && (
                            <Badge variant="outline">{descendant.fork_type}</Badge>
                          )}
                        </div>
                      </div>
                      <Badge>{descendant.current_state}</Badge>
                    </div>
                  ))}
                </div>
              </div>
            )}

            {(!lineage.ancestors || lineage.ancestors.length === 0) && (!lineage.descendants || lineage.descendants.length === 0) && (
              <p className="text-center text-muted-foreground py-4">
                This adapter has no parent or children.
              </p>
            )}
          </CardContent>
        </Card>
      )}

      {/* Promote Dialog */}
      <Dialog open={showPromoteDialog} onOpenChange={setShowPromoteDialog}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Promote Adapter Lifecycle</DialogTitle>
            <DialogDescription>
              Advance this adapter to the next lifecycle state. Please provide a reason for this promotion.
            </DialogDescription>
          </DialogHeader>
          <form onSubmit={handlePromoteSubmit}>
            <div className="space-y-4 py-4">
              <div className="space-y-2">
                <Label htmlFor="promote-reason">Reason for Promotion</Label>
                <Input
                  id="promote-reason"
                  value={promoteReason}
                  onChange={(e) => setPromoteReason(e.target.value)}
                  placeholder="e.g., Passed all integration tests"
                  required
                  disabled={isSubmitting}
                />
              </div>
            </div>
            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                onClick={() => {
                  setShowPromoteDialog(false);
                  setPromoteReason('');
                }}
                disabled={isSubmitting}
              >
                Cancel
              </Button>
              <Button type="submit" disabled={isSubmitting || !promoteReason.trim()}>
                {isSubmitting ? 'Promoting...' : 'Promote'}
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>

      {/* Demote Dialog */}
      <Dialog open={showDemoteDialog} onOpenChange={setShowDemoteDialog}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Demote Adapter Lifecycle</DialogTitle>
            <DialogDescription>
              Move this adapter back to the previous lifecycle state. Please provide a reason for this demotion.
            </DialogDescription>
          </DialogHeader>
          <form onSubmit={handleDemoteSubmit}>
            <div className="space-y-4 py-4">
              <div className="space-y-2">
                <Label htmlFor="demote-reason">Reason for Demotion</Label>
                <Input
                  id="demote-reason"
                  value={demoteReason}
                  onChange={(e) => setDemoteReason(e.target.value)}
                  placeholder="e.g., Failed validation checks"
                  required
                  disabled={isSubmitting}
                />
              </div>
            </div>
            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                onClick={() => {
                  setShowDemoteDialog(false);
                  setDemoteReason('');
                }}
                disabled={isSubmitting}
              >
                Cancel
              </Button>
              <Button type="submit" disabled={isSubmitting || !demoteReason.trim()}>
                {isSubmitting ? 'Demoting...' : 'Demote'}
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>

      {/* Add to Stack Modal */}
      {adapterId && (
        <AddToStackModal
          open={showAddToStackDialog}
          onOpenChange={setShowAddToStackDialog}
          adapterId={adapterId}
        />
      )}
    </div>
  );
};

export default AdapterDetail;
