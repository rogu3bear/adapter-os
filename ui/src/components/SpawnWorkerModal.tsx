import React, { useState, useEffect, useCallback } from 'react';
import { FormModal } from './shared/Modal';
import { Label } from './ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Alert, AlertDescription } from './ui/alert';
import { AlertTriangle, CheckCircle } from 'lucide-react';

// 【ui/src/components/SpawnWorkerModal.tsx§1-35】 - Replace toast notifications with ErrorRecovery patterns
import { errorRecoveryTemplates } from './ui/error-recovery';
import { toast } from 'sonner';
import apiClient from '@/api/client';
import { Node, Plan, SpawnWorkerRequest } from '@/api/types';
import { logger, toError } from '@/utils/logger';
import { useAsyncAction } from '@/hooks/async/useAsyncAction';

interface SpawnWorkerModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  selectedTenant: string;
  onSuccess: () => void;
}

export function SpawnWorkerModal({
  open,
  onOpenChange,
  selectedTenant,
  onSuccess,
}: SpawnWorkerModalProps) {
  const [nodes, setNodes] = useState<Node[]>([]);
  const [plans, setPlans] = useState<Plan[]>([]);
  const [selectedNode, setSelectedNode] = useState<string>('');
  const [selectedPlan, setSelectedPlan] = useState<string>('');
  const [tenantId, setTenantId] = useState<string>(selectedTenant);

  const [modalError, setModalError] = useState<Error | null>(null);
  const [validationMessage, setValidationMessage] = useState<string | null>(null);

  const { execute: spawnWorker, isLoading } = useAsyncAction(
    async (request: SpawnWorkerRequest) => {
      const worker = await apiClient.spawnWorker(request);
      return worker;
    },
    {
      errorToast: (error) => error.message || 'Failed to spawn worker',
      onSuccess: (worker) => {
        toast.success(`Worker ${worker.id} spawned successfully`);
        logger.info('Worker spawned successfully', {
          component: 'SpawnWorkerModal',
          operation: 'spawnWorker',
          workerId: worker.id,
        });
        onSuccess();
        onOpenChange(false);
        // Reset form
        setSelectedNode('');
        setSelectedPlan('');
      },
      onError: (error, request) => {
        setModalError(error);
        setValidationMessage(null);
        logger.error('Failed to spawn worker', {
          component: 'SpawnWorkerModal',
          operation: 'spawnWorker',
          tenantId: request.tenant_id,
          nodeId: request.node_id,
          planId: request.plan_id,
        }, error);
      },
      componentName: 'SpawnWorkerModal',
      operationName: 'spawn_worker',
    }
  );

  const loadData = useCallback(async () => {
    try {
      const [nodesData, plansData] = await Promise.all([
        apiClient.listNodes(),
        apiClient.listPlans(),
      ]);
      
      // Filter only healthy nodes
      const healthyNodes = nodesData.filter((node) => node.status === 'healthy');
      setNodes(healthyNodes);
      setPlans(plansData);

      // Auto-select first healthy node and plan if available

      if (healthyNodes.length > 0) {
        setSelectedNode((prev) => prev || healthyNodes[0].id);
      }
      if (plansData.length > 0) {
        setSelectedPlan((prev) => prev || plansData[0].id);
      }
      setModalError(null);
      setValidationMessage(null);
    } catch (err) {
      logger.error('Failed to load spawn worker data', {
        component: 'SpawnWorkerModal',
        operation: 'loadData',
        tenantId: selectedTenant,
      }, toError(err));
      const error = err instanceof Error ? err : new Error('Failed to load nodes and plans');
      setModalError(error);
      setValidationMessage('Failed to load nodes and plans. Try refreshing.');
    }
  }, [selectedTenant]);

  useEffect(() => {
    if (open) {
      void loadData();
      setTenantId(selectedTenant);
      setModalError(null);
      setValidationMessage(null);
    }
  }, [open, selectedTenant, loadData]);

  const handleSpawn = async () => {
    setValidationMessage(null);
    setModalError(null);

    if (!selectedNode || !selectedPlan || !tenantId) {
      setValidationMessage('Please select node, plan, and tenant.');
      return;
    }

    const request: SpawnWorkerRequest = {
      node_id: selectedNode,
      tenant_id: tenantId,
      plan_id: selectedPlan,
    };

    await spawnWorker(request);
  };

  const selectedNodeDetails = nodes.find((n) => n.id === selectedNode);

  return (
    <FormModal
      open={open}
      onOpenChange={onOpenChange}
      title="Spawn New Worker"
      size="md"
      onSubmit={handleSpawn}
      submitText="Spawn Worker"
      isSubmitting={isLoading}
      isValid={!!selectedNode && !!selectedPlan && nodes.length > 0}
      onCancel={() => {
        setSelectedNode('');
        setSelectedPlan('');
      }}
    >
      <div className="space-y-4">

          {modalError && errorRecoveryTemplates.genericError(
            modalError,
            () => {
              setModalError(null);
              if (!selectedNode || !selectedPlan) {
                void loadData();
              } else {
                void handleSpawn();
              }
            }
          )}

          {validationMessage && (
            <Alert className="border-amber-200 bg-amber-50">
              <AlertTriangle className="h-4 w-4 text-amber-600" />
              <AlertDescription className="text-amber-700">{validationMessage}</AlertDescription>
            </Alert>
          )}

          {nodes.length === 0 && (
            <Alert>
              <AlertTriangle className="h-4 w-4" />
              <AlertDescription>
                No healthy nodes available. Please ensure at least one node is online and healthy.
              </AlertDescription>
            </Alert>
          )}

          <div className="space-y-2">
            <Label htmlFor="tenant">Organization ID</Label>
            <div className="p-2 bg-muted rounded border text-sm font-mono">
              {tenantId}
            </div>
          </div>

          <div className="space-y-2">
            <Label htmlFor="node">Compute Node</Label>
            <Select value={selectedNode} onValueChange={setSelectedNode}>
              <SelectTrigger id="node">
                <SelectValue placeholder="Select a node..." />
              </SelectTrigger>
              <SelectContent>
                {nodes.filter(node => node.id && node.id !== '').map((node) => (
                  <SelectItem key={node.id} value={node.id}>
                    {node.hostname} - {node.metal_family} ({node.memory_gb}GB)
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            {selectedNodeDetails && (
              <div className="text-xs text-muted-foreground">
                <p>Metal: {selectedNodeDetails.metal_family}</p>
                <p>Memory: {selectedNodeDetails.memory_gb}GB</p>
                <p>Last seen: {selectedNodeDetails.last_heartbeat ? new Date(selectedNodeDetails.last_heartbeat).toLocaleString() : 'N/A'}</p>
              </div>
            )}
          </div>

          <div className="space-y-2">
            <Label htmlFor="plan">Plan</Label>
            <Select value={selectedPlan} onValueChange={setSelectedPlan}>
              <SelectTrigger id="plan">
                <SelectValue placeholder="Select a plan..." />
              </SelectTrigger>
              <SelectContent>
                {plans.filter(plan => plan.id && plan.id !== '').map((plan) => (
                  <SelectItem key={plan.id} value={plan.id}>
                    {plan.cpid} - {plan.status}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            {plans.length === 0 && (
              <p className="text-xs text-muted-foreground">
                No plans available. Please build a plan first.
              </p>
            )}
          </div>

          {selectedNode && selectedPlan && tenantId && (
            <Alert>
              <CheckCircle className="h-4 w-4" />
              <AlertDescription>
                Ready to spawn worker for tenant <span className="font-mono">{tenantId}</span> on{' '}
                <span className="font-mono">{selectedNodeDetails?.hostname}</span>
              </AlertDescription>
            </Alert>
          )}
      </div>
    </FormModal>
  );
}




