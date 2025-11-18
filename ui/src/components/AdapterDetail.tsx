// PRD-08: Adapter Detail View with Lineage Visualization
//
// Displays comprehensive adapter information including:
// - Core metadata (name, state, tier, memory, activations)
// - Lineage tree (ancestors and descendants)
// - Fork information (parent, children, fork type/reason)
// - Lifecycle controls (promote, demote)

import React, { useEffect, useState } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import { apiClient } from '../api/client';
import type { AdapterDetailResponse, AdapterLineageResponse, LineageNode } from '../api/types';
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter, DialogDescription } from './ui/dialog';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { useToast } from '@/hooks/use-toast';
import { ArrowUp, ArrowDown, GitBranch, Clock, Database, Activity } from 'lucide-react';

export const AdapterDetail: React.FC = () => {
  const { adapterId } = useParams<{ adapterId: string }>();
  const navigate = useNavigate();
  const { toast } = useToast();

  const [adapter, setAdapter] = useState<AdapterDetailResponse | null>(null);
  const [lineage, setLineage] = useState<AdapterLineageResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Dialog state
  const [showPromoteDialog, setShowPromoteDialog] = useState(false);
  const [showDemoteDialog, setShowDemoteDialog] = useState(false);
  const [promoteReason, setPromoteReason] = useState('');
  const [demoteReason, setDemoteReason] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);

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

  const getStateColor = (state: string): string => {
    const colors: Record<string, string> = {
      unloaded: 'gray',
      cold: 'blue',
      warm: 'yellow',
      hot: 'orange',
      resident: 'red',
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
            <CardTitle className="text-red-600">Error</CardTitle>
          </CardHeader>
          <CardContent>
            <p>{error || 'Adapter not found'}</p>
            <Button onClick={() => navigate('/adapters')} className="mt-4">
              Back to Adapters
            </Button>
          </CardContent>
        </Card>
      </div>
    );
  }

  return (
    <div className="container mx-auto p-6 space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-bold">{adapter.adapter_name || adapter.name}</h1>
          <p className="text-gray-500">
            {adapter.tenant_namespace && `${adapter.tenant_namespace}/`}
            {adapter.domain && `${adapter.domain}/`}
            {adapter.purpose}
            {adapter.revision && ` (${adapter.revision})`}
          </p>
        </div>
        <div className="flex gap-2">
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
              <Activity className="h-5 w-5 text-gray-500" />
              <div>
                <p className="text-sm text-gray-500">State</p>
                <Badge className={`bg-${getStateColor(adapter.current_state)}-500`}>
                  {adapter.current_state}
                </Badge>
              </div>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="pt-6">
            <div className="flex items-center gap-2">
              <Database className="h-5 w-5 text-gray-500" />
              <div>
                <p className="text-sm text-gray-500">Memory</p>
                <p className="text-2xl font-bold">
                  {(adapter.memory_bytes / 1024 / 1024).toFixed(2)} MB
                </p>
              </div>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="pt-6">
            <div className="flex items-center gap-2">
              <Activity className="h-5 w-5 text-gray-500" />
              <div>
                <p className="text-sm text-gray-500">Activations</p>
                <p className="text-2xl font-bold">{adapter.activation_count}</p>
              </div>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="pt-6">
            <div className="flex items-center gap-2">
              <Clock className="h-5 w-5 text-gray-500" />
              <div>
                <p className="text-sm text-gray-500">Last Active</p>
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
              <p className="text-sm text-gray-500">Adapter ID</p>
              <p className="font-mono text-sm">{adapter.adapter_id}</p>
            </div>
            <div>
              <p className="text-sm text-gray-500">Hash (B3)</p>
              <p className="font-mono text-sm truncate">{adapter.hash_b3}</p>
            </div>
            <div>
              <p className="text-sm text-gray-500">Rank</p>
              <p>{adapter.rank}</p>
            </div>
            <div>
              <p className="text-sm text-gray-500">Alpha</p>
              <p>{adapter.alpha}</p>
            </div>
            <div>
              <p className="text-sm text-gray-500">Category</p>
              <p>{adapter.category}</p>
            </div>
            <div>
              <p className="text-sm text-gray-500">Scope</p>
              <p>{adapter.scope}</p>
            </div>
            {adapter.framework && (
              <div>
                <p className="text-sm text-gray-500">Framework</p>
                <p>{adapter.framework}</p>
              </div>
            )}
            <div>
              <p className="text-sm text-gray-500">Tier</p>
              <p>{adapter.tier}</p>
            </div>
          </div>
        </CardContent>
      </Card>

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
            {lineage.ancestors.length > 0 && (
              <div className="mb-4">
                <h4 className="font-semibold mb-2">Ancestors</h4>
                <div className="space-y-2 pl-4 border-l-2 border-gray-300">
                  {lineage.ancestors.map((ancestor) => (
                    <div
                      key={ancestor.adapter_id}
                      className="flex items-center justify-between p-2 bg-gray-50 rounded cursor-pointer hover:bg-gray-100"
                      onClick={() => navigate(`/adapters/${ancestor.adapter_id}`)}
                    >
                      <div>
                        <p className="font-medium">{ancestor.adapter_name || ancestor.adapter_id}</p>
                        <p className="text-sm text-gray-500">{ancestor.revision}</p>
                      </div>
                      <Badge>{ancestor.current_state}</Badge>
                    </div>
                  ))}
                </div>
              </div>
            )}

            {/* Current Adapter */}
            <div className="my-4 p-4 bg-blue-50 border-2 border-blue-500 rounded">
              <p className="font-bold text-blue-900">Current Adapter</p>
              <p className="text-sm text-blue-700">
                {lineage.self_node.adapter_name || lineage.self_node.adapter_id}
              </p>
            </div>

            {/* Descendants */}
            {lineage.descendants.length > 0 && (
              <div>
                <h4 className="font-semibold mb-2">Descendants</h4>
                <div className="space-y-2 pl-4 border-l-2 border-gray-300">
                  {lineage.descendants.map((descendant) => (
                    <div
                      key={descendant.adapter_id}
                      className="flex items-center justify-between p-2 bg-gray-50 rounded cursor-pointer hover:bg-gray-100"
                      onClick={() => navigate(`/adapters/${descendant.adapter_id}`)}
                    >
                      <div>
                        <p className="font-medium">{descendant.adapter_name || descendant.adapter_id}</p>
                        <div className="flex items-center gap-2 text-sm text-gray-500">
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

            {lineage.ancestors.length === 0 && lineage.descendants.length === 0 && (
              <p className="text-center text-gray-500 py-4">
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
    </div>
  );
};

export default AdapterDetail;
