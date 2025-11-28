import React, { useState, useCallback, useRef } from 'react';
import { useNavigate } from 'react-router-dom';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Alert, AlertDescription } from '@/components/ui/alert';
import FeatureLayout from '@/layout/FeatureLayout';
import { DensityProvider } from '@/contexts/DensityContext';
import { useRBAC } from '@/hooks/useRBAC';
import { logger } from '@/utils/logger';
import { toast } from 'sonner';
import apiClient from '@/api/client';
import {
  Code,
  RefreshCw,
  Download,
  Upload,
  Brain,
  AlertCircle,
  MemoryStick,
  Activity,
} from 'lucide-react';
import type { Adapter } from '@/api/adapter-types';

import { AdapterTable } from './AdapterTable';
import { AdapterFilters } from './AdapterFilters';
import {
  useAdapters,
  useLoadAdapter,
  useUnloadAdapter,
  useDeleteAdapter,
  usePinAdapter,
  usePromoteAdapter,
  useEvictAdapter,
  type AdapterFilters as FilterValues,
} from './useAdapters';

// Export format for adapter backup/sharing
interface AdapterExportData {
  version: '1.0';
  exported_at: string;
  adapters: Array<{
    adapter_id: string;
    name: string;
    hash_b3: string;
    rank: number;
    tier: string;
    category: string;
    scope: string;
    framework?: string;
    description?: string;
    tenant_namespace?: string;
    domain?: string;
    purpose?: string;
    revision?: string;
    languages_json?: string;
  }>;
}

export function AdaptersPage() {
  const navigate = useNavigate();
  const { can } = useRBAC();
  const [filters, setFilters] = useState<FilterValues>({});
  const [selectedAdapters, setSelectedAdapters] = useState<string[]>([]);
  const [isImporting, setIsImporting] = useState(false);
  const [isExporting, setIsExporting] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);

  // React Query hooks
  const {
    data,
    isLoading,
    error,
    refetch,
    invalidateAdapters,
  } = useAdapters(filters);

  const loadMutation = useLoadAdapter();
  const unloadMutation = useUnloadAdapter();
  const deleteMutation = useDeleteAdapter();
  const pinMutation = usePinAdapter();
  const promoteMutation = usePromoteAdapter();
  const evictMutation = useEvictAdapter();

  // RBAC permissions
  const canRegister = can('adapter:register');
  const canLoad = can('adapter:load');
  const canUnload = can('adapter:unload');
  const canDelete = can('adapter:delete');

  // Extract data
  const adapters = data?.adapters ?? [];
  const totalMemory = data?.totalMemory ?? 0;
  const systemMetrics = data?.systemMetrics;

  // Action handlers
  const handleLoad = useCallback((adapterId: string) => {
    loadMutation.mutate(adapterId);
  }, [loadMutation]);

  const handleUnload = useCallback((adapterId: string) => {
    unloadMutation.mutate(adapterId);
  }, [unloadMutation]);

  const handleDelete = useCallback((adapterId: string) => {
    deleteMutation.mutate(adapterId);
    setSelectedAdapters(prev => prev.filter(id => id !== adapterId));
  }, [deleteMutation]);

  const handlePin = useCallback((adapterId: string, pinned: boolean) => {
    pinMutation.mutate({ adapterId, pinned });
  }, [pinMutation]);

  const handlePromote = useCallback((adapterId: string) => {
    promoteMutation.mutate(adapterId);
  }, [promoteMutation]);

  const handleEvict = useCallback((adapterId: string) => {
    evictMutation.mutate(adapterId);
  }, [evictMutation]);

  const handleViewHealth = useCallback((adapterId: string) => {
    // Navigate to adapter detail page with health tab
    navigate(`/adapters/${adapterId}?tab=health`);
  }, [navigate]);

  const handleDownloadManifest = useCallback(async (adapterId: string) => {
    try {
      const manifest = await apiClient.downloadAdapterManifest(adapterId);
      const blob = new Blob([JSON.stringify(manifest, null, 2)], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `${adapterId}-manifest.json`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
      toast.success('Manifest downloaded');
    } catch (err) {
      logger.error('Failed to download manifest', {
        component: 'AdaptersPage',
        operation: 'downloadManifest',
        adapterId,
      }, err instanceof Error ? err : new Error(String(err)));
      toast.error('Failed to download manifest');
    }
  }, []);

  // Import adapters from file (JSON metadata or .aos files)
  const handleImportAdapters = useCallback(() => {
    fileInputRef.current?.click();
  }, []);

  const handleFileSelected = useCallback(async (event: React.ChangeEvent<HTMLInputElement>) => {
    const files = event.target.files;
    if (!files || files.length === 0) return;

    setIsImporting(true);
    const importResults: { success: number; failed: number; errors: string[] } = {
      success: 0,
      failed: 0,
      errors: [],
    };

    try {
      for (const file of Array.from(files)) {
        const fileName = file.name.toLowerCase();

        try {
          if (fileName.endsWith('.aos')) {
            // Import .aos adapter file via API
            await apiClient.importAdapter(file, false);
            importResults.success++;
          } else if (fileName.endsWith('.json')) {
            // Parse JSON file - could be export data or single adapter manifest
            const content = await file.text();
            const parsed = JSON.parse(content);

            if (parsed.version === '1.0' && Array.isArray(parsed.adapters)) {
              // Bulk export format - register each adapter
              for (const adapterData of parsed.adapters) {
                try {
                  await apiClient.registerAdapter({
                    adapter_id: adapterData.adapter_id,
                    name: adapterData.name,
                    hash_b3: adapterData.hash_b3,
                    rank: adapterData.rank,
                    tier: adapterData.tier,
                    category: adapterData.category as 'code' | 'framework' | 'codebase' | 'ephemeral',
                    scope: adapterData.scope as 'global' | 'tenant' | 'repo' | 'commit',
                    framework: adapterData.framework,
                    languages: adapterData.languages,
                  });
                  importResults.success++;
                } catch (adapterErr) {
                  importResults.failed++;
                  importResults.errors.push(
                    `Failed to import ${adapterData.adapter_id}: ${adapterErr instanceof Error ? adapterErr.message : 'Unknown error'}`
                  );
                }
              }
            } else {
              // Assume single adapter manifest format
              importResults.errors.push(
                `${file.name}: Unsupported JSON format. Expected adapter export format with version "1.0".`
              );
              importResults.failed++;
            }
          } else {
            importResults.errors.push(`${file.name}: Unsupported file type. Use .aos or .json files.`);
            importResults.failed++;
          }
        } catch (fileErr) {
          importResults.failed++;
          importResults.errors.push(
            `${file.name}: ${fileErr instanceof Error ? fileErr.message : 'Failed to process file'}`
          );
        }
      }

      // Show results
      if (importResults.success > 0 && importResults.failed === 0) {
        toast.success(`Successfully imported ${importResults.success} adapter(s)`);
        invalidateAdapters();
      } else if (importResults.success > 0 && importResults.failed > 0) {
        toast.warning(
          `Imported ${importResults.success} adapter(s), ${importResults.failed} failed`,
          { description: importResults.errors.slice(0, 3).join('\n') }
        );
        invalidateAdapters();
      } else {
        toast.error('Failed to import adapters', {
          description: importResults.errors.slice(0, 3).join('\n'),
        });
      }

      logger.info('Adapter import completed', {
        component: 'AdaptersPage',
        operation: 'importAdapters',
        success: importResults.success,
        failed: importResults.failed,
      });
    } catch (err) {
      logger.error('Failed to import adapters', {
        component: 'AdaptersPage',
        operation: 'importAdapters',
      }, err instanceof Error ? err : new Error(String(err)));
      toast.error('Failed to import adapters');
    } finally {
      setIsImporting(false);
      // Reset file input
      if (fileInputRef.current) {
        fileInputRef.current.value = '';
      }
    }
  }, [invalidateAdapters]);

  // Export adapters to JSON file
  const handleExportAdapters = useCallback(async () => {
    setIsExporting(true);
    try {
      // Determine which adapters to export
      const adaptersToExport: Adapter[] = selectedAdapters.length > 0
        ? adapters.filter(a => selectedAdapters.includes(a.adapter_id))
        : adapters;

      if (adaptersToExport.length === 0) {
        toast.warning('No adapters to export');
        return;
      }

      // Build export data
      const exportData: AdapterExportData = {
        version: '1.0',
        exported_at: new Date().toISOString(),
        adapters: adaptersToExport.map(adapter => ({
          adapter_id: adapter.adapter_id,
          name: adapter.name,
          hash_b3: adapter.hash_b3,
          rank: adapter.rank,
          tier: adapter.tier,
          category: adapter.category,
          scope: adapter.scope,
          framework: adapter.framework,
          description: adapter.description,
          tenant_namespace: adapter.tenant_namespace,
          domain: adapter.domain,
          purpose: adapter.purpose,
          revision: adapter.revision,
          languages_json: adapter.languages_json,
        })),
      };

      // Create and download file
      const blob = new Blob([JSON.stringify(exportData, null, 2)], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      const timestamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
      a.download = `adapters-export-${timestamp}.json`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);

      toast.success(`Exported ${adaptersToExport.length} adapter(s)`);
      logger.info('Adapters exported', {
        component: 'AdaptersPage',
        operation: 'exportAdapters',
        count: adaptersToExport.length,
        selectedOnly: selectedAdapters.length > 0,
      });
    } catch (err) {
      logger.error('Failed to export adapters', {
        component: 'AdaptersPage',
        operation: 'exportAdapters',
      }, err instanceof Error ? err : new Error(String(err)));
      toast.error('Failed to export adapters');
    } finally {
      setIsExporting(false);
    }
  }, [adapters, selectedAdapters]);

  // Bulk actions
  const handleBulkLoad = useCallback(() => {
    selectedAdapters.forEach(id => loadMutation.mutate(id));
    setSelectedAdapters([]);
  }, [selectedAdapters, loadMutation]);

  const handleBulkUnload = useCallback(() => {
    selectedAdapters.forEach(id => unloadMutation.mutate(id));
    setSelectedAdapters([]);
  }, [selectedAdapters, unloadMutation]);

  const handleBulkDelete = useCallback(() => {
    selectedAdapters.forEach(id => deleteMutation.mutate(id));
    setSelectedAdapters([]);
  }, [selectedAdapters, deleteMutation]);

  // Stats for header
  const loadedCount = adapters.filter(a =>
    ['warm', 'hot', 'resident'].includes(a.current_state)
  ).length;
  const pinnedCount = adapters.filter(a => a.pinned).length;
  const totalMemoryUsedMB = adapters.reduce((acc, a) => acc + a.memory_bytes, 0) / (1024 * 1024);

  const isAnyMutationLoading =
    loadMutation.isPending ||
    unloadMutation.isPending ||
    deleteMutation.isPending ||
    pinMutation.isPending ||
    promoteMutation.isPending ||
    evictMutation.isPending;

  return (
    <DensityProvider pageKey="adapters">
      <FeatureLayout
        title="Adapters"
        description="Manage and monitor LoRA adapters"
        maxWidth="xl"
        contentPadding="default"
        primaryAction={{
          label: 'Train New Adapter',
          icon: Brain,
          onClick: () => navigate('/training'),
          disabled: !canRegister,
          size: 'sm',
        }}
        helpContent="Manage your LoRA adapter fleet - load, unload, pin, and monitor adapter performance."
      >
        <div className="space-y-6">
          {/* Error Alert */}
          {error && (
            <Alert variant="destructive">
              <AlertCircle className="h-4 w-4" />
              <AlertDescription>
                Failed to load adapters: {error instanceof Error ? error.message : 'Unknown error'}
                <Button variant="link" className="ml-2 p-0 h-auto" onClick={() => refetch()}>
                  Retry
                </Button>
              </AlertDescription>
            </Alert>
          )}

          {/* Stats Cards */}
          <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
            <Card>
              <CardContent className="pt-4">
                <div className="flex items-center justify-between">
                  <div>
                    <p className="text-sm text-muted-foreground">Total Adapters</p>
                    <p className="text-2xl font-bold">{adapters.length}</p>
                  </div>
                  <Code className="h-8 w-8 text-muted-foreground" />
                </div>
              </CardContent>
            </Card>

            <Card>
              <CardContent className="pt-4">
                <div className="flex items-center justify-between">
                  <div>
                    <p className="text-sm text-muted-foreground">Loaded</p>
                    <p className="text-2xl font-bold">{loadedCount}</p>
                  </div>
                  <Activity className="h-8 w-8 text-green-500" />
                </div>
              </CardContent>
            </Card>

            <Card>
              <CardContent className="pt-4">
                <div className="flex items-center justify-between">
                  <div>
                    <p className="text-sm text-muted-foreground">Protected</p>
                    <p className="text-2xl font-bold">{pinnedCount}</p>
                  </div>
                  <Badge variant="secondary" className="h-8 px-3">
                    {pinnedCount > 0 ? 'Protected' : 'None'}
                  </Badge>
                </div>
              </CardContent>
            </Card>

            <Card>
              <CardContent className="pt-4">
                <div className="flex items-center justify-between">
                  <div>
                    <p className="text-sm text-muted-foreground">Memory Used</p>
                    <p className="text-2xl font-bold">{totalMemoryUsedMB.toFixed(0)} MB</p>
                  </div>
                  <MemoryStick className="h-8 w-8 text-muted-foreground" />
                </div>
              </CardContent>
            </Card>
          </div>

          {/* Main Content Card */}
          <Card>
            <CardHeader>
              <div className="flex items-center justify-between">
                <CardTitle className="flex items-center gap-2">
                  <Code className="h-5 w-5" />
                  Adapter Registry
                  {adapters.length > 0 && (
                    <Badge variant="secondary">{adapters.length}</Badge>
                  )}
                </CardTitle>
                <div className="flex items-center gap-2">
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => refetch()}
                    disabled={isLoading}
                  >
                    <RefreshCw className={`h-4 w-4 mr-1 ${isLoading ? 'animate-spin' : ''}`} />
                    Refresh
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={!canRegister || isImporting}
                    onClick={handleImportAdapters}
                  >
                    <Upload className={`h-4 w-4 mr-1 ${isImporting ? 'animate-pulse' : ''}`} />
                    {isImporting ? 'Importing...' : 'Import'}
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={adapters.length === 0 || isExporting}
                    onClick={handleExportAdapters}
                    title={selectedAdapters.length > 0 ? `Export ${selectedAdapters.length} selected` : 'Export all adapters'}
                  >
                    <Download className={`h-4 w-4 mr-1 ${isExporting ? 'animate-pulse' : ''}`} />
                    {isExporting ? 'Exporting...' : selectedAdapters.length > 0 ? `Export (${selectedAdapters.length})` : 'Export'}
                  </Button>
                  {/* Hidden file input for import */}
                  <input
                    ref={fileInputRef}
                    type="file"
                    accept=".json,.aos"
                    multiple
                    className="hidden"
                    onChange={handleFileSelected}
                  />
                </div>
              </div>
            </CardHeader>
            <CardContent>
              {/* Filters */}
              <AdapterFilters
                filters={filters}
                onFiltersChange={setFilters}
              />

              {/* Bulk Actions Bar */}
              {selectedAdapters.length > 0 && (
                <div className="flex items-center gap-2 mb-4 p-3 bg-accent rounded-lg">
                  <span className="text-sm font-medium">
                    {selectedAdapters.length} selected
                  </span>
                  <div className="flex-1" />
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={handleBulkLoad}
                    disabled={!canLoad || isAnyMutationLoading}
                  >
                    Load All
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={handleBulkUnload}
                    disabled={!canUnload || isAnyMutationLoading}
                  >
                    Unload All
                  </Button>
                  <Button
                    variant="destructive"
                    size="sm"
                    onClick={handleBulkDelete}
                    disabled={!canDelete || isAnyMutationLoading}
                  >
                    Delete All
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => setSelectedAdapters([])}
                  >
                    Clear Selection
                  </Button>
                </div>
              )}

              {/* Adapter Table */}
              <AdapterTable
                adapters={adapters}
                isLoading={isLoading}
                selectedAdapters={selectedAdapters}
                onSelectionChange={setSelectedAdapters}
                onLoad={handleLoad}
                onUnload={handleUnload}
                onDelete={handleDelete}
                onPin={handlePin}
                onPromote={handlePromote}
                onEvict={handleEvict}
                onViewHealth={handleViewHealth}
                onDownloadManifest={handleDownloadManifest}
                canLoad={canLoad}
                canUnload={canUnload}
                canDelete={canDelete}
                totalMemory={totalMemory}
              />
            </CardContent>
          </Card>
        </div>
      </FeatureLayout>
    </DensityProvider>
  );
}

export default AdaptersPage;
