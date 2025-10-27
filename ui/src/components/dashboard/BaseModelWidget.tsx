import React, { useState, useEffect, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Play, Pause, Upload, CheckCircle, XCircle, RefreshCw, Loader2 } from 'lucide-react';
import { toast } from 'sonner';
import apiClient from '@/api/client';
import { ModelStatusResponse } from '@/api/types';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { ModelImportWizard } from '@/components/ModelImportWizard';
import { Button } from '@/components/ui/button';
import { useTenant } from '@/layout/LayoutProvider';

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
  const [status, setStatus] = useState<ModelStatusResponse | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isActionLoading, setIsActionLoading] = useState(false);
  const [showImportWizard, setShowImportWizard] = useState(false);

  const fetchStatus = useCallback(async () => {
    if (!selectedTenant) return;
    setIsLoading(true);
    try {
      const statusData = await apiClient.getBaseModelStatus(selectedTenant);
      setStatus(statusData);
    } catch (error) {
      toast.error('Failed to fetch base model status.');
      setStatus(null);
    } finally {
      setIsLoading(false);
    }
  }, [selectedTenant]);

  useEffect(() => {
    fetchStatus();
  }, [fetchStatus]);

  const handleLoad = async () => {
    if (!status?.model_id) {
      toast.error('No model ID available to load.');
      return;
    }
    setIsActionLoading(true);
    try {
      await apiClient.loadBaseModel(status.model_id);
      toast.success('Base model loaded successfully.');
      fetchStatus();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to load model.');
    } finally {
      setIsActionLoading(false);
    }
  };

  const handleUnload = async () => {
    if (!status?.model_id) {
      toast.error('No model ID available to unload.');
      return;
    }
    setIsActionLoading(true);
    try {
      await apiClient.unloadBaseModel(status.model_id);
      toast.success('Base model unloaded successfully.');
      fetchStatus();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to unload model.');
    } finally {
      setIsActionLoading(false);
    }
  };

  const handleImportComplete = () => {
    setShowImportWizard(false);
    toast.success('Model import process started.');
    fetchStatus();
  };

  const canLoad = status && ['unloaded', 'error'].includes(status.status);
  const canUnload = status && status.status === 'loaded';

  return (
    <>
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
              <Button onClick={() => setShowImportWizard(true)} variant="secondary" className="w-full">
                <Upload className="h-4 w-4 mr-2" />
                Import New Model
              </Button>
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
