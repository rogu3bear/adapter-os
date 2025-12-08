import React, { useState, useEffect, useCallback } from 'react';
import { Badge } from '@/components/ui/badge';
import { Play, Pause, Upload, Download, CheckCircle, XCircle, RefreshCw, Loader2 } from 'lucide-react';
import apiClient from '@/api/client';
import { BaseModelStatus } from '@/api/types';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { ModelImportWizard } from '@/components/ModelImportWizard';
import { Button } from '@/components/ui/button';
import { useAuth } from '@/providers/CoreProviders';
import { useTenant } from '@/providers/FeatureProviders';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { useRBAC } from '@/hooks/useRBAC';
import { usePolling } from '@/hooks/usePolling';
import { DashboardWidgetFrame, type DashboardWidgetState } from './DashboardWidgetFrame';

// Utility functions for request deduplication
const OPERATION_STORAGE_KEY = 'adapteros_model_operations';
const OPERATION_TIMEOUT_MS = 300000; // 5 minutes timeout for stale operations

interface OngoingOperation {
  operation: 'load' | 'unload';
  timestamp: number;
  tenantId: string;
}

function getOngoingOperations(): Record<string, OngoingOperation> {
  try {
    const stored = localStorage.getItem(OPERATION_STORAGE_KEY);
    return stored ? JSON.parse(stored) : {};
  } catch {
    return {};
  }
}

function setOngoingOperation(modelId: string, operation: 'load' | 'unload', tenantId: string) {
  const operations = getOngoingOperations();
  operations[modelId] = {
    operation,
    timestamp: Date.now(),
    tenantId,
  };
  localStorage.setItem(OPERATION_STORAGE_KEY, JSON.stringify(operations));
}

function clearOngoingOperation(modelId: string) {
  const operations = getOngoingOperations();
  delete operations[modelId];
  localStorage.setItem(OPERATION_STORAGE_KEY, JSON.stringify(operations));
}

function isOperationInProgress(modelId: string, tenantId: string): OngoingOperation | null {
  const operations = getOngoingOperations();
  const op = operations[modelId];

  if (!op) return null;

  // Check if operation is for the same tenant
  if (op.tenantId !== tenantId) return null;

  // Check if operation is stale (older than timeout)
  if (Date.now() - op.timestamp > OPERATION_TIMEOUT_MS) {
    clearOngoingOperation(modelId);
    return null;
  }

  return op;
}

// Cleanup stale operations on component mount
function cleanupStaleOperations() {
  const operations = getOngoingOperations();
  const now = Date.now();
  let hasChanges = false;

  Object.keys(operations).forEach(modelId => {
    if (now - operations[modelId].timestamp > OPERATION_TIMEOUT_MS) {
      delete operations[modelId];
      hasChanges = true;
    }
  });

  if (hasChanges) {
    localStorage.setItem(OPERATION_STORAGE_KEY, JSON.stringify(operations));
  }
}

function getStatusIcon(status: BaseModelStatus | null) {
  if (!status) return <XCircle className="h-5 w-5 text-gray-400" />;
  switch (status.status) {
    case 'loaded':
      return <CheckCircle className="h-5 w-5 text-gray-600" />;
    case 'loading':
    case 'unloading':
      return <RefreshCw className="h-5 w-5 text-gray-400 animate-spin" />;
    default:
      return <XCircle className="h-5 w-5 text-gray-700" />;
  }
}

export function BaseModelWidget() {
  const { selectedTenant } = useTenant();
  const { user } = useAuth();
  const { can } = useRBAC();
  const [isActionLoading, setIsActionLoading] = useState(false);
  const [showImportWizard, setShowImportWizard] = useState(false);
  const [statusMessage, setStatusMessage] = useState<{ message: string; variant: 'success' | 'info' | 'warning' } | null>(null);
  const [errorRecovery, setErrorRecovery] = useState<React.ReactElement | null>(null);

  const isAdmin = user?.role.toLowerCase() === 'admin';
  const canRegister = can('adapter:register');

  // Cleanup stale operations on mount
  useEffect(() => {
    cleanupStaleOperations();
  }, []);

  const showStatus = (message: string, variant: 'success' | 'info' | 'warning') => {
    setStatusMessage({ message, variant });
  };

  const fetchStatus = useCallback(async () => {
    if (!selectedTenant) return null;
    const statusData = await apiClient.getBaseModelStatus(selectedTenant);
    setStatusMessage(null);
    setErrorRecovery(null);
    return statusData;
  }, [selectedTenant]);

  const {
    data: status,
    isLoading,
    refetch: refetchStatus,
    error: pollingError
  } = usePolling(
    fetchStatus,
    'normal',
    {
      showLoadingIndicator: true,
      onError: (error) => {
        setStatusMessage({ message: 'Failed to fetch base model status.', variant: 'warning' });
        setErrorRecovery(
          errorRecoveryTemplates.genericError(
            error instanceof Error ? error : new Error('Failed to fetch base model status.'),
            () => refetchStatus()
          )
        );
      }
    }
  );

  const handleLoad = async () => {
    if (!status?.model_id) {
      showStatus('No model ID available to load.', 'warning');
      return;
    }

    if (!selectedTenant) {
      showStatus('No tenant selected.', 'warning');
      return;
    }

    // Check for ongoing operations
    const ongoingOp = isOperationInProgress(status.model_id, selectedTenant);
    if (ongoingOp) {
      showStatus(`Model is already ${ongoingOp.operation === 'load' ? 'loading' : 'unloading'} in another tab.`, 'warning');
      return;
    }

    setIsActionLoading(true);
    setOngoingOperation(status.model_id, 'load', selectedTenant);

    try {
      await apiClient.loadBaseModel(status.model_id);
      refetchStatus();
      showStatus('Base model load requested.', 'success');
    } catch (err) {
      setStatusMessage({ message: err instanceof Error ? err.message : 'Failed to load model.', variant: 'warning' });
      setErrorRecovery(
        errorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error('Failed to load model.'),
          () => handleLoad()
        )
      );
    } finally {
      setIsActionLoading(false);
      clearOngoingOperation(status.model_id);
    }
  };

  const handleUnload = async () => {
    if (!status?.model_id) {
      showStatus('No model ID available to unload.', 'warning');
      return;
    }

    if (!selectedTenant) {
      showStatus('No tenant selected.', 'warning');
      return;
    }

    // Check for ongoing operations
    const ongoingOp = isOperationInProgress(status.model_id, selectedTenant);
    if (ongoingOp) {
      showStatus(`Model is already ${ongoingOp.operation === 'load' ? 'loading' : 'unloading'} in another tab.`, 'warning');
      return;
    }

    setIsActionLoading(true);
    setOngoingOperation(status.model_id, 'unload', selectedTenant);

    try {
      await apiClient.unloadBaseModel(status.model_id);
      refetchStatus();
      showStatus('Base model unload requested.', 'success');
    } catch (err) {
      setStatusMessage({ message: err instanceof Error ? err.message : 'Failed to unload model.', variant: 'warning' });
      setErrorRecovery(
        errorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error('Failed to unload model.'),
          () => handleUnload()
        )
      );
    } finally {
      setIsActionLoading(false);
      clearOngoingOperation(status.model_id);
    }
  };

  const handleImportComplete = () => {
    setShowImportWizard(false);
    showStatus('Model import process started.', 'success');
    refetchStatus();
  };

  const handleDownload = async () => {
    if (!status?.model_id) {
      showStatus('No model ID available to download.', 'warning');
      return;
    }
    if (!isAdmin) {
      showStatus('Only administrators can download base models.', 'warning');
      return;
    }
    // Model download requires fetching artifact details from a separate endpoint
    // This is a placeholder for when artifact download is fully implemented
    showStatus('Model download is not yet available. Please use the CLI to export models.', 'info');
  };

  const canLoad = status && ['no-model', 'error'].includes(status.status);
  const canUnload = status && status.status === 'ready';
  const disableDownload = !status?.model_id || isActionLoading || !isAdmin;

  const widgetState: DashboardWidgetState = pollingError
    ? 'error'
    : isLoading
      ? 'loading'
      : status
        ? 'ready'
        : 'empty';

  return (
    <>
      <DashboardWidgetFrame
        title={
          <div className="flex items-center gap-2">
            {isLoading ? <Loader2 className="h-5 w-5 animate-spin" /> : getStatusIcon(status)}
            <span>Base Model</span>
            <GlossaryTooltip termId="base-model-status" />
          </div>
        }
        subtitle="Base model lifecycle and actions"
        state={widgetState}
        onRefresh={() => refetchStatus()}
        onRetry={() => refetchStatus()}
        lastUpdated={lastUpdated}
        errorMessage={pollingError?.message}
        emptyMessage="No base model detected"
        emptyAction={
          <Button
            onClick={() => setShowImportWizard(true)}
            variant="secondary"
          >
            <Upload className="h-4 w-4 mr-2" />
            Import New Model
          </Button>
        }
        headerRight={
          status ? (
            <Badge variant={status.is_loaded ? 'default' : 'secondary'}>
              {status.status}
            </Badge>
          ) : null
        }
        loadingContent={<div className="h-24 animate-pulse bg-muted rounded" />}
      >
        <div className="space-y-4">
          {errorRecovery && (
            <div className="mb-2">
              {errorRecovery}
            </div>
          )}

          {statusMessage && (
            <Alert
              className={
                statusMessage.variant === 'success'
                  ? 'border-green-200 bg-green-50'
                  : statusMessage.variant === 'warning'
                    ? 'border-amber-200 bg-amber-50'
                    : 'border-blue-200 bg-blue-50'
              }
            >
              <AlertDescription
                className={
                  statusMessage.variant === 'success'
                    ? 'text-green-700'
                    : statusMessage.variant === 'warning'
                      ? 'text-amber-700'
                      : 'text-blue-700'
                }
              >
                {statusMessage.message}
              </AlertDescription>
            </Alert>
          )}

          {status && (
            <>
              <div>
                <p className="text-sm font-medium">
                  {status.model_name || 'No Model'}
                  <GlossaryTooltip termId="base-model-name" />
                </p>
                <p className="text-xs text-muted-foreground">{status.model_id || 'No model has been imported'}</p>
                {status.model_path && (
                  <p className="text-xs text-muted-foreground/70 mt-1 truncate" title={status.model_path}>
                    📁 {status.model_path}
                  </p>
                )}
              </div>
              <div className="flex gap-2">
                <Button onClick={handleLoad} disabled={!canLoad || isActionLoading} className="flex-1">
                  <Play className="h-4 w-4 mr-2" />
                  Activate
                </Button>
                <Button onClick={handleUnload} variant="outline" disabled={!canUnload || isActionLoading} className="flex-1">
                  <Pause className="h-4 w-4 mr-2" />
                  Deactivate
                </Button>
              </div>
              <div className="flex gap-2">
                <Button
                  onClick={handleDownload}
                  disabled={disableDownload}
                  variant="outline"
                  className="flex-1"
                  title={!isAdmin ? 'Only admins can download base models' : undefined}
                >
                  <Download className="h-4 w-4 mr-2" />
                  Download
                </Button>
                <Button
                  onClick={() => setShowImportWizard(true)}
                  variant="secondary"
                  className="flex-1"
                  disabled={!canRegister}
                  title={!canRegister ? 'Requires adapter:register permission' : undefined}
                >
                  <Upload className="h-4 w-4 mr-2" />
                  Import New Model
                </Button>
              </div>
            </>
          )}
        </div>
      </DashboardWidgetFrame>
      <Dialog open={showImportWizard} onOpenChange={setShowImportWizard}>
        <DialogContent className="max-w-4xl max-h-[90vh] overflow-y-auto">
          <DialogHeader>
            <DialogTitle>Import Base Model</DialogTitle>
          </DialogHeader>
          <ModelImportWizard onComplete={handleImportComplete} onCancel={() => setShowImportWizard(false)} />
        </DialogContent>
      </Dialog>
    </>
  );
}
