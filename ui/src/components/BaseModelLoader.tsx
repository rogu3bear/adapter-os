import React, { useState } from 'react';
import { Button } from './ui/button';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Badge } from './ui/badge';
import { Play, Pause, Upload, CheckCircle, XCircle, RefreshCw } from 'lucide-react';
import { toast } from 'sonner';
import apiClient from '@/api/client';
import { ModelStatusResponse } from '@/api/types';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from './ui/dialog';
import { ModelImportWizard } from './ModelImportWizard';
import { ErrorRecovery, errorRecoveryTemplates } from './ui/error-recovery';
import { useAsyncAction } from '@/hooks/useAsyncAction';

interface BaseModelLoaderProps {
  status: ModelStatusResponse | null;
  onRefresh: () => void;
}

export function BaseModelLoader({ status, onRefresh }: BaseModelLoaderProps) {
  const [showImportWizard, setShowImportWizard] = useState(false);

  const loadAction = useAsyncAction(
    async () => {
      if (!status?.model_id) {
        throw new Error('No model to load');
      }
      await apiClient.loadBaseModel(status.model_id);
    },
    {
      successToast: 'Base model loaded successfully',
      onSuccess: () => onRefresh(),
      componentName: 'BaseModelLoader',
      operationName: 'loadModel',
    }
  );

  const unloadAction = useAsyncAction(
    async () => {
      if (!status?.model_id) {
        throw new Error('No model to unload');
      }
      await apiClient.unloadBaseModel(status.model_id);
    },
    {
      successToast: 'Base model unloaded successfully',
      onSuccess: () => onRefresh(),
      componentName: 'BaseModelLoader',
      operationName: 'unloadModel',
    }
  );

  const handleLoad = () => loadAction.execute();
  const handleUnload = () => unloadAction.execute();

  const handleImportComplete = (importId: string) => {
    setShowImportWizard(false);
    toast.success('Model import completed');
    onRefresh();
  };

  const getStatusIcon = () => {
    if (!status) return <XCircle className="h-5 w-5 text-gray-400" />;
    switch (status.status) {
      case 'ready':
        return <CheckCircle className="h-5 w-5 text-green-500" />;
      case 'loading':
      case 'unloading':
        return <RefreshCw className="h-5 w-5 text-blue-500 animate-spin" />;
      default:
        return <XCircle className="h-5 w-5 text-gray-400" />;
    }
  };

  const canLoad = status && ['no-model', 'error'].includes(status.status);
  const canUnload = status && status.status === 'ready';
  const isLoading = loadAction.isLoading || unloadAction.isLoading;

  return (
    <>
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <CardTitle className="flex items-center gap-2">
              {getStatusIcon()}
              Base Model Controls
            </CardTitle>
            <Badge variant={status?.is_loaded ? 'default' : 'secondary'}>
              {status?.is_loaded ? 'Ready' : 'Not Loaded'}
            </Badge>
          </div>
        </CardHeader>
        <CardContent className="space-y-4">
          {loadAction.error && (
            <ErrorRecovery
              error={loadAction.error.message}
              onRetry={handleLoad}
            />
          )}
          {unloadAction.error && (
            <ErrorRecovery
              error={unloadAction.error.message}
              onRetry={handleUnload}
            />
          )}
          <div className="flex gap-2">
            <Button
              onClick={handleLoad}
              disabled={!canLoad || isLoading}
              className="flex-1"
            >
              <Play className="h-4 w-4 mr-2" />
              Load Model
            </Button>
            <Button
              onClick={handleUnload}
              variant="outline"
              disabled={!canUnload || isLoading}
              className="flex-1"
            >
              <Pause className="h-4 w-4 mr-2" />
              Unload Model
            </Button>
          </div>
          <Button
            onClick={() => setShowImportWizard(true)}
            variant="secondary"
            className="w-full"
          >
            <Upload className="h-4 w-4 mr-2" />
            Import New Model
          </Button>
        </CardContent>
      </Card>

      <Dialog open={showImportWizard} onOpenChange={setShowImportWizard}>
        <DialogContent className="max-w-4xl max-h-[90vh] overflow-y-auto">
          <DialogHeader>
            <DialogTitle>Import Base Model</DialogTitle>
          </DialogHeader>
          <ModelImportWizard
            onComplete={handleImportComplete}
            onCancel={() => setShowImportWizard(false)}
          />
        </DialogContent>
      </Dialog>
    </>
  );
}
