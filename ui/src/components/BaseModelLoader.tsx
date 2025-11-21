import React, { useState } from 'react';
import { Button } from './ui/button';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Badge } from './ui/badge';
import { Play, Pause, Upload, CheckCircle, XCircle, RefreshCw } from 'lucide-react';
import { toast } from 'sonner';
import apiClient from '../api/client';
import { ModelStatusResponse } from '../api/types';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from './ui/dialog';
import { ModelImportWizard } from './ModelImportWizard';
import { ErrorRecovery, errorRecoveryTemplates } from './ui/error-recovery';

interface BaseModelLoaderProps {
  status: ModelStatusResponse | null;
  onRefresh: () => void;
}

export function BaseModelLoader({ status, onRefresh }: BaseModelLoaderProps) {
  const [isLoading, setIsLoading] = useState(false);
  const [showImportWizard, setShowImportWizard] = useState(false);
  const [loadError, setLoadError] = useState<Error | null>(null);
  const [unloadError, setUnloadError] = useState<Error | null>(null);

  const handleLoad = async () => {
    if (!status?.model_id) {
      setLoadError(new Error('No model to load'));
      return;
    }

    setLoadError(null);
    setIsLoading(true);
    try {
      await apiClient.loadBaseModel(status.model_id);
      toast.success('Base model loaded successfully');
      onRefresh();
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Failed to load model');
      setLoadError(error);
    } finally {
      setIsLoading(false);
    }
  };

  const handleUnload = async () => {
    if (!status?.model_id) {
      setUnloadError(new Error('No model to unload'));
      return;
    }

    setUnloadError(null);
    setIsLoading(true);
    try {
      await apiClient.unloadBaseModel(status.model_id);
      toast.success('Base model unloaded successfully');
      onRefresh();
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Failed to unload model');
      setUnloadError(error);
    } finally {
      setIsLoading(false);
    }
  };

  const handleImportComplete = (importId: string) => {
    setShowImportWizard(false);
    toast.success('Model import completed');
    onRefresh();
  };

  const getStatusIcon = () => {
    if (!status) return <XCircle className="h-5 w-5 text-gray-400" />;
    switch (status.status) {
      case 'loaded':
        return <CheckCircle className="h-5 w-5 text-green-500" />;
      case 'loading':
      case 'unloading':
        return <RefreshCw className="h-5 w-5 text-blue-500 animate-spin" />;
      default:
        return <XCircle className="h-5 w-5 text-gray-400" />;
    }
  };

  const canLoad = status && ['unloaded', 'error'].includes(status.status);
  const canUnload = status && ['loaded'].includes(status.status);

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
              {status?.is_loaded ? 'Loaded' : 'Unloaded'}
            </Badge>
          </div>
        </CardHeader>
        <CardContent className="space-y-4">
          {loadError && (
            <ErrorRecovery
              error={loadError.message}
              onRetry={handleLoad}
            />
          )}
          {unloadError && (
            <ErrorRecovery
              error={unloadError.message}
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
