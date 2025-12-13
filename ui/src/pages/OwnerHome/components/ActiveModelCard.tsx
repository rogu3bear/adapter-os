/**
 * ActiveModelCard - Simplified model display and control
 *
 * Shows the currently loaded model with load/unload actions.
 * Replaces the more complex ModelControlPanel with focused functionality.
 */

import React, { useState, useMemo } from 'react';
import { useNavigate } from 'react-router-dom';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Skeleton } from '@/components/ui/skeleton';
import {
  Database,
  Power,
  PowerOff,
  Loader2,
  ExternalLink,
  MemoryStick,
  AlertCircle,
} from 'lucide-react';
import { toast } from 'sonner';
import { apiClient } from '@/api/client';
import { cn } from '@/components/ui/utils';
import { formatBytes } from '@/utils/format';

interface BaseModel {
  id: string;
  name: string;
  size_bytes?: number;
  format?: string;
  status?: 'ready' | 'available' | 'loading' | 'error' | 'no-model' | 'unloading' | 'checking' | 'loaded';
  path?: string;
}

interface ActiveModelCardProps {
  models: BaseModel[];
  isLoading: boolean;
  onRefresh: () => void;
  className?: string;
}

export function ActiveModelCard({
  models,
  isLoading,
  onRefresh,
  className,
}: ActiveModelCardProps) {
  const navigate = useNavigate();
  const [operationLoading, setOperationLoading] = useState(false);

  // Find the currently loaded model
  const loadedModel = useMemo(() => {
    return models.find((m) => m.status === 'ready');
  }, [models]);

  // Calculate memory usage estimate
  const memoryEstimate = useMemo(() => {
    if (!loadedModel?.size_bytes) return null;
    const memoryMB = (loadedModel.size_bytes * 1.2) / (1024 * 1024);
    return memoryMB >= 1024
      ? `~${(memoryMB / 1024).toFixed(1)} GB`
      : `~${memoryMB.toFixed(0)} MB`;
  }, [loadedModel]);

  const handleUnloadModel = async () => {
    if (!loadedModel) return;

    setOperationLoading(true);
    try {
      await apiClient.unloadBaseModel(loadedModel.id);
      toast.success(`Model "${loadedModel.name}" unloaded`);
      onRefresh();
    } catch (error) {
      const msg = error instanceof Error ? error.message : 'Unknown error';
      toast.error(`Failed to unload: ${msg}`);
    } finally {
      setOperationLoading(false);
    }
  };

  if (isLoading) {
    return (
      <Card className={className}>
        <CardHeader className="pb-3">
          <Skeleton className="h-5 w-32" />
        </CardHeader>
        <CardContent>
          <Skeleton className="h-20 w-full" />
        </CardContent>
      </Card>
    );
  }

  return (
    <Card className={className}>
      <CardHeader className="pb-3">
        <CardTitle className="text-sm font-medium text-slate-600 flex items-center gap-2">
          <Database className="h-4 w-4" />
          Active Model
        </CardTitle>
      </CardHeader>
      <CardContent>
        {loadedModel ? (
          <div className="space-y-4">
            {/* Model Info */}
            <div className="flex items-start justify-between gap-3">
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2 mb-1">
                  <h3 className="text-lg font-semibold text-slate-900 truncate">
                    {loadedModel.name}
                  </h3>
                  <Badge variant="default" className="text-xs bg-green-600">
                    Loaded
                  </Badge>
                </div>
                <div className="flex flex-wrap items-center gap-2 text-sm text-slate-600">
                  <span>{formatBytes(loadedModel.size_bytes)}</span>
                  {loadedModel.format && (
                    <>
                      <span className="text-slate-300">•</span>
                      <span className="uppercase text-xs">{loadedModel.format}</span>
                    </>
                  )}
                  {memoryEstimate && (
                    <>
                      <span className="text-slate-300">•</span>
                      <span className="flex items-center gap-1">
                        <MemoryStick className="h-3 w-3" />
                        {memoryEstimate}
                      </span>
                    </>
                  )}
                </div>
              </div>
            </div>

            {/* Actions */}
            <div className="flex items-center gap-2">
              <Button
                variant="outline"
                size="sm"
                onClick={handleUnloadModel}
                disabled={operationLoading}
                className="text-red-600 hover:text-red-700 hover:bg-red-50"
              >
                {operationLoading ? (
                  <Loader2 className="h-4 w-4 mr-1.5 animate-spin" />
                ) : (
                  <PowerOff className="h-4 w-4 mr-1.5" />
                )}
                Unload
              </Button>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => navigate('/base-models')}
              >
                <ExternalLink className="h-4 w-4 mr-1.5" />
                Manage Models
              </Button>
            </div>
          </div>
        ) : (
          /* No Model State */
          <div className="text-center py-6">
            <div className="inline-flex items-center justify-center h-12 w-12 rounded-full bg-slate-100 mb-3">
              <AlertCircle className="h-6 w-6 text-slate-400" />
            </div>
            <h3 className="text-sm font-medium text-slate-900 mb-1">
              No Model Loaded
            </h3>
            <p className="text-sm text-slate-500 mb-4">
              Import and load a base model to enable inference
            </p>
            <Button
              variant="default"
              size="sm"
              onClick={() => navigate('/base-models')}
            >
              <Power className="h-4 w-4 mr-1.5" />
              Import Model
            </Button>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
