import { useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Skeleton } from '@/components/ui/skeleton';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import {
  Database,
  Download,
  Upload,
  Power,
  PowerOff,
  Pin,
  PinOff,
  RefreshCw,
  Loader2,
} from 'lucide-react';
import { toast } from 'sonner';
import { apiClient } from '@/api/client';

interface BaseModel {
  id: string;
  name: string;
  size_bytes?: number;
  format?: string;
  status?: 'loaded' | 'available' | 'loading' | 'error';
  path?: string;
}

interface Adapter {
  id: string;
  adapter_id?: string;
  name?: string;
  lifecycle_state?: string;
  pinned?: boolean;
}

interface ModelControlPanelProps {
  models: BaseModel[];
  adapters: Adapter[];
  isLoading: boolean;
  onRefresh: () => void;
}

interface OperationState {
  [key: string]: boolean;
}

function formatBytes(bytes: number | undefined): string {
  if (bytes === undefined) return 'N/A';

  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let size = bytes;
  let unitIndex = 0;

  while (size >= 1024 && unitIndex < units.length - 1) {
    size /= 1024;
    unitIndex++;
  }

  return `${size.toFixed(1)} ${units[unitIndex]}`;
}

function getStatusBadgeVariant(status: BaseModel['status']): 'default' | 'secondary' | 'destructive' | 'outline' {
  switch (status) {
    case 'loaded':
      return 'default';
    case 'available':
      return 'secondary';
    case 'loading':
      return 'outline';
    case 'error':
      return 'destructive';
    default:
      return 'secondary';
  }
}

function getLifecycleStateBadgeVariant(state: Adapter['lifecycle_state']): 'default' | 'secondary' | 'outline' {
  switch (state) {
    case 'hot':
    case 'resident':
      return 'default';
    case 'warm':
      return 'secondary';
    case 'cold':
    case 'unloaded':
      return 'outline';
    default:
      return 'outline';
  }
}

export function ModelControlPanel({
  models,
  adapters,
  isLoading,
  onRefresh,
}: ModelControlPanelProps) {
  const [loadingOperations, setLoadingOperations] = useState<OperationState>({});

  const setOperationLoading = (key: string, loading: boolean) => {
    setLoadingOperations((prev) => ({ ...prev, [key]: loading }));
  };

  const handleLoadModel = async (model: BaseModel) => {
    const operationKey = `load-${model.id}`;
    setOperationLoading(operationKey, true);

    try {
      await apiClient.loadBaseModel(model.id);
      toast.success(`Model "${model.name}" loaded successfully`);
      onRefresh();
    } catch (error) {
      toast.error(`Failed to load model: ${error instanceof Error ? error.message : 'Unknown error'}`);
    } finally {
      setOperationLoading(operationKey, false);
    }
  };

  const handleUnloadModel = async (model: BaseModel) => {
    const operationKey = `unload-${model.id}`;
    setOperationLoading(operationKey, true);

    try {
      await apiClient.unloadBaseModel(model.id);
      toast.success(`Model "${model.name}" unloaded successfully`);
      onRefresh();
    } catch (error) {
      toast.error(`Failed to unload model: ${error instanceof Error ? error.message : 'Unknown error'}`);
    } finally {
      setOperationLoading(operationKey, false);
    }
  };

  const handleDownloadModel = async (model: BaseModel) => {
    const operationKey = `download-${model.id}`;
    setOperationLoading(operationKey, true);

    try {
      await apiClient.downloadModel(model.id);
      toast.success(`Model "${model.name}" download started`);
      onRefresh();
    } catch (error) {
      toast.error(`Failed to download model: ${error instanceof Error ? error.message : 'Unknown error'}`);
    } finally {
      setOperationLoading(operationKey, false);
    }
  };

  const handlePinAdapter = async (adapter: Adapter) => {
    const operationKey = `pin-${adapter.id}`;
    setOperationLoading(operationKey, true);

    try {
      await apiClient.pinAdapter(adapter.adapter_id || adapter.id, true);
      toast.success(`Adapter "${adapter.name || adapter.adapter_id || adapter.id}" pinned`);
      onRefresh();
    } catch (error) {
      toast.error(`Failed to pin adapter: ${error instanceof Error ? error.message : 'Unknown error'}`);
    } finally {
      setOperationLoading(operationKey, false);
    }
  };

  const handleUnpinAdapter = async (adapter: Adapter) => {
    const operationKey = `unpin-${adapter.id}`;
    setOperationLoading(operationKey, true);

    try {
      await apiClient.unpinAdapter(adapter.adapter_id || adapter.id);
      toast.success(`Adapter "${adapter.name || adapter.adapter_id || adapter.id}" unpinned`);
      onRefresh();
    } catch (error) {
      toast.error(`Failed to unpin adapter: ${error instanceof Error ? error.message : 'Unknown error'}`);
    } finally {
      setOperationLoading(operationKey, false);
    }
  };

  const adapterStateCounts = {
    hot: adapters.filter((a) => a.lifecycle_state === 'hot').length,
    warm: adapters.filter((a) => a.lifecycle_state === 'warm').length,
    cold: adapters.filter((a) => a.lifecycle_state === 'cold').length,
    resident: adapters.filter((a) => a.lifecycle_state === 'resident').length,
    unloaded: adapters.filter((a) => a.lifecycle_state === 'unloaded').length,
  };

  const pinnedAdapters = adapters.filter((a) => a.pinned);

  return (
    <div className="space-y-6" role="region" aria-label="Model and adapter controls">
      {/* Base Models Section */}
      <Card>
        <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-4">
          <div className="flex items-center gap-2">
            <Database className="h-5 w-5 text-muted-foreground" aria-hidden="true" />
            <CardTitle>Base Models</CardTitle>
          </div>
          <Button
            variant="outline"
            size="sm"
            onClick={onRefresh}
            disabled={isLoading}
            aria-label="Refresh models list"
          >
            <RefreshCw className={`h-4 w-4 mr-2 ${isLoading ? 'animate-spin' : ''}`} aria-hidden="true" />
            Refresh
          </Button>
        </CardHeader>
        <CardContent>
          {isLoading ? (
            <div className="space-y-3">
              {[1, 2, 3].map((i) => (
                <Skeleton key={i} className="h-12 w-full" />
              ))}
            </div>
          ) : models.length === 0 ? (
            <div className="text-center py-8 text-muted-foreground">
              No base models available
            </div>
          ) : (
            <Table aria-label="Base models list">
              <TableHeader>
                <TableRow>
                  <TableHead scope="col">Name</TableHead>
                  <TableHead scope="col">Size</TableHead>
                  <TableHead scope="col">Format</TableHead>
                  <TableHead scope="col">Status</TableHead>
                  <TableHead scope="col" className="text-right">Actions</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {models.map((model) => {
                  const loadKey = `load-${model.id}`;
                  const unloadKey = `unload-${model.id}`;
                  const downloadKey = `download-${model.id}`;

                  return (
                    <TableRow key={model.id}>
                      <TableCell className="font-medium">{model.name}</TableCell>
                      <TableCell>{formatBytes(model.size_bytes)}</TableCell>
                      <TableCell>
                        <Badge variant="outline">{model.format || 'N/A'}</Badge>
                      </TableCell>
                      <TableCell>
                        <Badge variant={getStatusBadgeVariant(model.status)}>
                          {model.status || 'unknown'}
                        </Badge>
                      </TableCell>
                      <TableCell className="text-right">
                        <div className="flex justify-end gap-2">
                          {model.status === 'loaded' ? (
                            <Button
                              variant="outline"
                              size="sm"
                              onClick={() => handleUnloadModel(model)}
                              disabled={loadingOperations[unloadKey]}
                            >
                              {loadingOperations[unloadKey] ? (
                                <Loader2 className="h-4 w-4 animate-spin" />
                              ) : (
                                <>
                                  <PowerOff className="h-4 w-4 mr-2" />
                                  Unload
                                </>
                              )}
                            </Button>
                          ) : model.status === 'available' ? (
                            <Button
                              variant="default"
                              size="sm"
                              onClick={() => handleLoadModel(model)}
                              disabled={loadingOperations[loadKey]}
                            >
                              {loadingOperations[loadKey] ? (
                                <Loader2 className="h-4 w-4 animate-spin" />
                              ) : (
                                <>
                                  <Power className="h-4 w-4 mr-2" />
                                  Load
                                </>
                              )}
                            </Button>
                          ) : model.status === 'loading' ? (
                            <Button variant="outline" size="sm" disabled>
                              <Loader2 className="h-4 w-4 animate-spin mr-2" />
                              Loading...
                            </Button>
                          ) : (
                            <Button
                              variant="secondary"
                              size="sm"
                              onClick={() => handleDownloadModel(model)}
                              disabled={loadingOperations[downloadKey]}
                            >
                              {loadingOperations[downloadKey] ? (
                                <Loader2 className="h-4 w-4 animate-spin" />
                              ) : (
                                <>
                                  <Download className="h-4 w-4 mr-2" />
                                  Download
                                </>
                              )}
                            </Button>
                          )}
                        </div>
                      </TableCell>
                    </TableRow>
                  );
                })}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>

      {/* Key Adapters Summary Section */}
      <Card>
        <CardHeader>
          <div className="flex items-center gap-2">
            <Upload className="h-5 w-5 text-muted-foreground" />
            <CardTitle>Key Adapters</CardTitle>
          </div>
        </CardHeader>
        <CardContent>
          {isLoading ? (
            <div className="space-y-4">
              <Skeleton className="h-20 w-full" />
              <Skeleton className="h-32 w-full" />
            </div>
          ) : (
            <div className="space-y-6">
              {/* Lifecycle State Summary */}
              <div>
                <h3 className="text-sm font-medium mb-3">Lifecycle States</h3>
                <div className="grid grid-cols-2 md:grid-cols-5 gap-3">
                  <div className="p-3 bg-slate-50 rounded-lg">
                    <div className="text-xs text-muted-foreground mb-1">Hot</div>
                    <div className="text-2xl font-bold">{adapterStateCounts.hot}</div>
                  </div>
                  <div className="p-3 bg-slate-50 rounded-lg">
                    <div className="text-xs text-muted-foreground mb-1">Warm</div>
                    <div className="text-2xl font-bold">{adapterStateCounts.warm}</div>
                  </div>
                  <div className="p-3 bg-slate-50 rounded-lg">
                    <div className="text-xs text-muted-foreground mb-1">Cold</div>
                    <div className="text-2xl font-bold">{adapterStateCounts.cold}</div>
                  </div>
                  <div className="p-3 bg-slate-50 rounded-lg">
                    <div className="text-xs text-muted-foreground mb-1">Resident</div>
                    <div className="text-2xl font-bold">{adapterStateCounts.resident}</div>
                  </div>
                  <div className="p-3 bg-slate-50 rounded-lg">
                    <div className="text-xs text-muted-foreground mb-1">Unloaded</div>
                    <div className="text-2xl font-bold">{adapterStateCounts.unloaded}</div>
                  </div>
                </div>
              </div>

              {/* Pinned Adapters */}
              <div>
                <h3 className="text-sm font-medium mb-3">
                  Pinned Adapters ({pinnedAdapters.length})
                </h3>
                {pinnedAdapters.length === 0 ? (
                  <div className="text-center py-6 text-muted-foreground bg-slate-50 rounded-lg">
                    No pinned adapters
                  </div>
                ) : (
                  <div className="space-y-2">
                    {pinnedAdapters.map((adapter) => {
                      const unpinKey = `unpin-${adapter.id}`;

                      return (
                        <div
                          key={adapter.id}
                          className="flex items-center justify-between p-3 bg-slate-50 rounded-lg"
                        >
                          <div className="flex items-center gap-3">
                            <Pin className="h-4 w-4 text-blue-600" />
                            <div>
                              <div className="font-medium">
                                {adapter.name || adapter.adapter_id || adapter.id}
                              </div>
                              {adapter.lifecycle_state && (
                                <Badge
                                  variant={getLifecycleStateBadgeVariant(adapter.lifecycle_state)}
                                  className="mt-1"
                                >
                                  {adapter.lifecycle_state}
                                </Badge>
                              )}
                            </div>
                          </div>
                          <Button
                            variant="ghost"
                            size="sm"
                            onClick={() => handleUnpinAdapter(adapter)}
                            disabled={loadingOperations[unpinKey]}
                          >
                            {loadingOperations[unpinKey] ? (
                              <Loader2 className="h-4 w-4 animate-spin" />
                            ) : (
                              <>
                                <PinOff className="h-4 w-4 mr-2" />
                                Unpin
                              </>
                            )}
                          </Button>
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>

              {/* Quick Pin Section for non-pinned adapters */}
              {adapters.length > pinnedAdapters.length && (
                <div>
                  <h3 className="text-sm font-medium mb-3">Quick Pin</h3>
                  <div className="space-y-2 max-h-64 overflow-y-auto">
                    {adapters
                      .filter((a) => !a.pinned)
                      .slice(0, 5)
                      .map((adapter) => {
                        const pinKey = `pin-${adapter.id}`;

                        return (
                          <div
                            key={adapter.id}
                            className="flex items-center justify-between p-3 bg-slate-50 rounded-lg hover:bg-slate-100 transition-colors"
                          >
                            <div className="flex items-center gap-3">
                              <div>
                                <div className="font-medium text-sm">
                                  {adapter.name || adapter.adapter_id || adapter.id}
                                </div>
                                {adapter.lifecycle_state && (
                                  <Badge
                                    variant={getLifecycleStateBadgeVariant(adapter.lifecycle_state)}
                                    className="mt-1"
                                  >
                                    {adapter.lifecycle_state}
                                  </Badge>
                                )}
                              </div>
                            </div>
                            <Button
                              variant="ghost"
                              size="sm"
                              onClick={() => handlePinAdapter(adapter)}
                              disabled={loadingOperations[pinKey]}
                            >
                              {loadingOperations[pinKey] ? (
                                <Loader2 className="h-4 w-4 animate-spin" />
                              ) : (
                                <>
                                  <Pin className="h-4 w-4 mr-2" />
                                  Pin
                                </>
                              )}
                            </Button>
                          </div>
                        );
                      })}
                  </div>
                </div>
              )}
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
