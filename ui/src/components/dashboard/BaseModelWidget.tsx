import React, { useState, useEffect, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Play, Pause, Upload, Download, CheckCircle, XCircle, RefreshCw, Loader2 } from 'lucide-react';
import apiClient from '@/api/client';
import { ModelStatusResponse } from '@/api/types';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { ModelImportWizard } from '@/components/ModelImportWizard';
import { Button } from '@/components/ui/button';
import { useTenant, useAuth } from '@/layout/LayoutProvider';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { ErrorRecoveryTemplates } from '@/components/ui/error-recovery';

function getStatusIcon(status: ModelStatusResponse | null) {
  if (!status) return <XCircle className="h-5 w-5 text-gray-400" />;
  switch (status.status) {
    case 'loaded':
      return <CheckCircle className="h-5 w-5 text-green-500" />;
    case 'loading':
    case 'unloading':
      return <RefreshCw className="h-5 w-5 text-blue-500 animate-spin" />;
    default:
      return <XCircle className="h-5 w-5 text-red-400" />;
  }
}

export function BaseModelWidget() {
  const { selectedTenant } = useTenant();
  const { user } = useAuth();
  const [status, setStatus] = useState<ModelStatusResponse | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isActionLoading, setIsActionLoading] = useState(false);
  const [showImportWizard, setShowImportWizard] = useState(false);
  const [statusMessage, setStatusMessage] = useState<{ message: string; variant: 'success' | 'info' | 'warning' } | null>(null);
  const [errorRecovery, setErrorRecovery] = useState<React.ReactElement | null>(null);

  const isAdmin = user?.role === 'Admin';

  const showStatus = (message: string, variant: 'success' | 'info' | 'warning') => {
    setStatusMessage({ message, variant });
  };

  const fetchStatus = useCallback(async () => {
    if (!selectedTenant) return;
    setIsLoading(true);
    try {
      const statusData = await apiClient.getBaseModelStatus(selectedTenant);
      setStatus(statusData);
      setStatusMessage(null);
      setErrorRecovery(null);
    } catch (error) {
      setStatus(null);
      setStatusMessage({ message: 'Failed to fetch base model status.', variant: 'warning' });
      setErrorRecovery(
        ErrorRecoveryTemplates.genericError(
          error instanceof Error ? error : new Error('Failed to fetch base model status.'),
          () => fetchStatus()
        )
      );
    } finally {
      setIsLoading(false);
    }
  }, [selectedTenant]);

  useEffect(() => {
    fetchStatus();
  }, [fetchStatus]);

  const handleLoad = async () => {
    if (!status?.model_id) {
      showStatus('No model ID available to load.', 'warning');
      return;
    }
    setIsActionLoading(true);
    try {
      await apiClient.loadBaseModel(status.model_id);
      fetchStatus();
      showStatus('Base model load requested.', 'success');
    } catch (err) {
      setStatusMessage({ message: err instanceof Error ? err.message : 'Failed to load model.', variant: 'warning' });
      setErrorRecovery(
        ErrorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error('Failed to load model.'),
          () => handleLoad()
        )
      );
    } finally {
      setIsActionLoading(false);
    }
  };

  const handleUnload = async () => {
    if (!status?.model_id) {
      showStatus('No model ID available to unload.', 'warning');
      return;
    }
    setIsActionLoading(true);
    try {
      await apiClient.unloadBaseModel(status.model_id);
      fetchStatus();
      showStatus('Base model unload requested.', 'success');
    } catch (err) {
      setStatusMessage({ message: err instanceof Error ? err.message : 'Failed to unload model.', variant: 'warning' });
      setErrorRecovery(
        ErrorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error('Failed to unload model.'),
          () => handleUnload()
        )
      );
    } finally {
      setIsActionLoading(false);
    }
  };

  const handleImportComplete = () => {
    setShowImportWizard(false);
    showStatus('Model import process started.', 'success');
    fetchStatus();
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
    setIsActionLoading(true);
    try {
      const response = await apiClient.downloadModel(status.model_id);
      if (!response.artifacts || response.artifacts.length === 0) {
        showStatus('No downloadable artifacts are available for this model.', 'warning');
        return;
      }

      const targetArtifact = response.artifacts.find((artifact) => artifact.artifact === 'weights') ?? response.artifacts[0];
      const downloadUrl = apiClient.buildUrl(targetArtifact.download_url);

      const anchor = document.createElement('a');
      anchor.href = downloadUrl;
      anchor.download = targetArtifact.filename;
      anchor.rel = 'noopener noreferrer';
      anchor.style.display = 'none';
      document.body.appendChild(anchor);
      anchor.click();
      document.body.removeChild(anchor);

      showStatus(`Download starting for ${targetArtifact.filename}.`, 'success');
    } catch (err) {
      setStatusMessage({ message: err instanceof Error ? err.message : 'Failed to download model.', variant: 'warning' });
    } finally {
      setIsActionLoading(false);
    }
  };

  const canLoad = status && ['unloaded', 'error'].includes(status.status);
  const canUnload = status && status.status === 'loaded';
  const disableDownload = !status?.model_id || isActionLoading || !isAdmin;

  return (
    <>
      {errorRecovery && (
        <div className="mb-4">
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

      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <CardTitle className="flex items-center gap-2">
              {isLoading ? <Loader2 className="h-5 w-5 animate-spin" /> : getStatusIcon(status)}
              Base Model
            </CardTitle>
            {status && (
              <Badge variant={status.is_loaded ? 'default' : 'secondary'}>
                {status.status}
              </Badge>
            )}
          </div>
        </CardHeader>
        <CardContent className="space-y-4">
          {isLoading ? (
            <div className="h-24 animate-pulse bg-muted rounded" />
          ) : (
            <>
              <div>
                <p className="text-sm font-medium">{status?.model_name || 'No Model'}</p>
                <p className="text-xs text-muted-foreground">{status?.model_id || 'No model has been imported'}</p>
              </div>
              <div className="flex gap-2">
                <Button onClick={handleLoad} disabled={!canLoad || isActionLoading} className="flex-1">
                  <Play className="h-4 w-4 mr-2" />
                  Load
                </Button>
                <Button onClick={handleUnload} variant="outline" disabled={!canUnload || isActionLoading} className="flex-1">
                  <Pause className="h-4 w-4 mr-2" />
                  Unload
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
                <Button onClick={() => setShowImportWizard(true)} variant="secondary" className="flex-1">
                  <Upload className="h-4 w-4 mr-2" />
                  Import New Model
                </Button>
              </div>
            </>
          )}
        </CardContent>
      </Card>
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
