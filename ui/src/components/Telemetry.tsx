import React, { useState, useEffect, useCallback, useMemo } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './ui/table';
import { VirtualizedTableRows } from './ui/virtualized-table';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from './ui/dialog';
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from './ui/dropdown-menu';
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from './ui/accordion';
import { Activity, Download, Eye, CheckCircle, MoreHorizontal, Shield, Trash2 } from 'lucide-react';
import { ExportMenu } from './ui/export-menu';
import { Alert, AlertDescription } from './ui/alert';
import { EmptyState } from './ui/empty-state';
import { LoadingState } from './ui/loading-state';
import { Checkbox } from './ui/checkbox';
import { BulkActionBar, BulkAction } from './ui/bulk-action-bar';
import apiClient from '../api/client';
import { TelemetryBundle, User, VerifyBundleSignatureResponse } from '../api/types';
import { useSSE } from '../hooks/useSSE';
import { useTimestamp } from '../hooks/useTimestamp';
import { canonicalKey } from './ui/utils';
import { HashChainView } from './HashChainView';
import { toast } from 'sonner';
import { AdvancedFilter, type FilterConfig, type FilterValues } from './ui/advanced-filter';

import { useAuth, useTenant } from '@/layout/LayoutProvider';
// 【ui/src/components/Telemetry.tsx§1-27】 - Replace toast errors with ErrorRecovery
import { GoldenCompareModal } from './GoldenCompareModal';
import { logger, toError } from '../utils/logger';
import { ErrorRecovery, ErrorRecoveryTemplates } from './ui/error-recovery';
import { DensityControls } from './ui/density-controls';
import { useDensity } from '../contexts/DensityContext';

import { useAuth, useTenant } from '@/layout/LayoutProvider';

interface TelemetryProps {
  user?: User;
  selectedTenant?: string;
}

export function Telemetry({ user: userProp, selectedTenant: tenantProp }: TelemetryProps) {
  const { user } = useAuth();
  const { selectedTenant } = useTenant();
  const effectiveUser = userProp ?? user!;
  const effectiveTenant = tenantProp ?? selectedTenant;
  const [bundles, setBundles] = useState<TelemetryBundle[]>([]);
  const [loading, setLoading] = useState(true);
  const [telemetryError, setTelemetryError] = useState<Error | null>(null);
  const [showVerifyModal, setShowVerifyModal] = useState(false);
  const [showCompareModal, setShowCompareModal] = useState(false);
  const [showPurgeModal, setShowPurgeModal] = useState(false);
  const [verifyResult, setVerifyResult] = useState<VerifyBundleSignatureResponse | null>(null);
  const [selectedBundle, setSelectedBundle] = useState<TelemetryBundle | null>(null);
  const [selectedBundles, setSelectedBundles] = useState<string[]>([]);
  const [purgeKeepCount, setPurgeKeepCount] = useState(12);
  
  // Filtering state
  const [filterValues, setFilterValues] = useState<FilterValues>({});
  
  // Filter configurations for telemetry bundles
  const telemetryFilterConfigs: FilterConfig[] = [
    {
      id: 'search',
      label: 'Search',
      type: 'text',
      placeholder: 'Search by bundle ID or CPID...',
    },
    {
      id: 'cpid',
      label: 'CPID',
      type: 'text',
      placeholder: 'Filter by CPID...',
    },
    {
      id: 'dateRange',
      label: 'Created Date Range',
      type: 'dateRange',
    },
    {
      id: 'eventCount',
      label: 'Event Count Range',
      type: 'number',
      min: 0,
      placeholder: 'Min/Max events',
    },
    {
      id: 'sizeRange',
      label: 'Size Range (MB)',
      type: 'number',
      min: 0,
      placeholder: 'Min/Max size',
    },
  ];
  
  // Filter bundles based on filter values
  const filteredBundles = bundles.filter(bundle => {
    // Search filter
    if (filterValues.search) {
      const searchLower = String(filterValues.search).toLowerCase();
      if (
        !bundle.id.toLowerCase().includes(searchLower) &&
        !bundle.cpid.toLowerCase().includes(searchLower)
      ) {
        return false;
      }
    }
    
    // CPID filter
    if (filterValues.cpid && !bundle.cpid.toLowerCase().includes(String(filterValues.cpid).toLowerCase())) {
      return false;
    }
    
    // Date range filter
    if (filterValues.dateRange && typeof filterValues.dateRange === 'object') {
      const range = filterValues.dateRange as { start?: string; end?: string };
      const bundleDate = new Date(bundle.created_at);
      if (range.start && bundleDate < new Date(range.start)) {
        return false;
      }
      if (range.end) {
        const endDate = new Date(range.end);
        endDate.setHours(23, 59, 59, 999); // Include entire end day
        if (bundleDate > endDate) {
          return false;
        }
      }
    }
    
    // Event count range
    if (filterValues.eventCount && typeof filterValues.eventCount === 'object') {
      const range = filterValues.eventCount as { min?: number; max?: number };
      if (range.min !== undefined && bundle.event_count < range.min) {
        return false;
      }
      if (range.max !== undefined && bundle.event_count > range.max) {
        return false;
      }
    }
    
    // Size range (convert MB to bytes for comparison)
    if (filterValues.sizeRange && typeof filterValues.sizeRange === 'object') {
      const range = filterValues.sizeRange as { min?: number; max?: number };
      const bundleSizeMB = bundle.size_bytes / 1024 / 1024;
      if (range.min !== undefined && bundleSizeMB < range.min) {
        return false;
      }
      if (range.max !== undefined && bundleSizeMB > range.max) {
        return false;
      }
    }
    
    return true;
  });

  useEffect(() => {
    if (loading) {
      logger.debug('Telemetry: showing loading state', {
        component: 'Telemetry',
        tenantId: effectiveTenant,
      });
    }
  }, [loading, effectiveTenant]);

  useEffect(() => {
    if (!loading && filteredBundles.length === 0) {
      logger.info('Telemetry: empty state rendered', {
        component: 'Telemetry',
        tenantId: effectiveTenant,
        totalBundles: bundles.length,
        filterCount: filteredBundles.length,
      });
    }
  }, [bundles.length, effectiveTenant, filteredBundles.length, loading]);
  
  // Golden compare modal is encapsulated in its own component

  // SSE connection state for bundles
  const [sseConnected, setSseConnected] = useState(false);

  useEffect(() => {
    const fetchBundles = async () => {
      try {
        setTelemetryError(null);
        const data = await apiClient.listTelemetryBundles();
        setBundles(data);
      } catch (err) {
        const error = err instanceof Error ? err : new Error('Failed to fetch telemetry bundles');
        setTelemetryError(error);
        logger.error('Failed to fetch telemetry bundles', {
          component: 'Telemetry',
          operation: 'fetchBundles',
          tenantId: effectiveTenant,
        }, toError(err));
      } finally {
        setLoading(false);
      }
    };
    fetchBundles();
  }, [effectiveTenant]);

  // SSE for real-time bundle notifications
  useEffect(() => {
    const baseUrl = (import.meta as { env?: { VITE_API_URL?: string } }).env?.VITE_API_URL || '/api';
    const url = `${baseUrl}/v1/stream/telemetry`;
    
    const eventSource = new EventSource(url);

    eventSource.onopen = () => {
      setSseConnected(true);
    };

    eventSource.onerror = () => {
      setSseConnected(false);
    };

    // Listen for bundle events (single objects or arrays)
    eventSource.addEventListener('bundles', (event) => {
      try {
        const payload = JSON.parse(event.data);
        // Normalize: handle both single object and array
        const bundleList = Array.isArray(payload) ? payload : [payload];
        
        setBundles((prev) => {
          // Merge new bundles, avoiding duplicates by ID
          const existingIds = new Set(prev.map(b => b.id));
          const newBundles = bundleList.filter(b => !existingIds.has(b.id));
          if (newBundles.length === 0) return prev;
          
          // Prepend new bundles and limit to last 100
          const merged = [...newBundles, ...prev];
          return merged.slice(0, 100);
        });
      } catch (err) {
        logger.error('Failed to parse bundles SSE payload', {
          component: 'Telemetry',
          operation: 'sse_bundles_parse',
        }, toError(err));
      }
    });

    return () => {
      eventSource.close();
    };
  }, []);

  const handleExportBundle = (bundle: TelemetryBundle) => {
    // Download bundle as JSON
    const dataStr = JSON.stringify(bundle, null, 2);
    const dataBlob = new Blob([dataStr], { type: 'application/json' });
    const url = URL.createObjectURL(dataBlob);
    const link = document.createElement('a');
    link.href = url;
    link.download = `telemetry-bundle-${bundle.id}.json`;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);
    // Browser download feedback is sufficient
  };

  const handleExportAllBundles = useCallback(async (format: 'csv' | 'json') => {
    try {
      if (format === 'json') {
        // Export all bundles using the API endpoint
        const exportPromises = bundles.map(bundle => apiClient.exportTelemetryBundle(bundle.id));
        const exportResults = await Promise.all(exportPromises);
        
        // For bundles with download URLs, we can either download each or combine them
        // For now, export as a JSON array of bundle metadata and download URLs
        const exportData = {
          exported_at: new Date().toISOString(),
          bundle_count: bundles.length,
          bundles: exportResults.map((result, index) => ({
            bundle_id: result.bundle_id,
            events_count: result.events_count,
            size_bytes: result.size_bytes,
            download_url: result.download_url,
            expires_at: result.expires_at,
            bundle_info: bundles[index]
          }))
        };
        
        const dataStr = JSON.stringify(exportData, null, 2);
        const dataBlob = new Blob([dataStr], { type: 'application/json' });
        const url = URL.createObjectURL(dataBlob);
        const link = document.createElement('a');
        link.href = url;
        link.download = `telemetry-bundles-export-${new Date().toISOString().split('T')[0]}.json`;
        document.body.appendChild(link);
        link.click();
        document.body.removeChild(link);
        URL.revokeObjectURL(url);
      } else {
        // CSV format - export bundle metadata
        const csvHeaders = ['Bundle ID', 'CPID', 'Events', 'Size (MB)', 'Merkle Root', 'Created At'];
        const csvRows = bundles.map(bundle => [
          bundle.id,
          bundle.cpid,
          bundle.event_count.toString(),
          (bundle.size_bytes / 1024 / 1024).toFixed(2),
          bundle.merkle_root || 'N/A',
          bundle.created_at
        ]);
        const csvContent = [csvHeaders.join(','), ...csvRows.map(row => row.map(cell => `"${cell}"`).join(','))].join('\n');
        const csvBlob = new Blob([csvContent], { type: 'text/csv' });
        const url = URL.createObjectURL(csvBlob);
        const link = document.createElement('a');
        link.href = url;
        link.download = `telemetry-bundles-export-${new Date().toISOString().split('T')[0]}.csv`;
        document.body.appendChild(link);
        link.click();
        document.body.removeChild(link);
        URL.revokeObjectURL(url);
      }
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Failed to export telemetry bundles');
      setTelemetryError(error);
      logger.error('Failed to export telemetry bundles', {
        component: 'Telemetry',
        operation: 'exportAllBundles',
        bundleCount: bundles.length,
      }, toError(err));
    }
  }, [bundles]);

  const handleVerifySignature = async (bundle: TelemetryBundle) => {
    try {
      const result = await apiClient.verifyBundleSignature(bundle.id);
      setVerifyResult(result);
      setSelectedBundle(bundle);
      setShowVerifyModal(true);
      // Verification result shown in modal - no need for toast
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Failed to verify signature');
      setTelemetryError(error);
      logger.error('Telemetry bundle signature verification failed', {
        component: 'Telemetry',
        operation: 'verifySignature',
        bundleId: bundle.id,
      }, toError(err));
    }
  };

  const handleCompareToGolden = (bundle: TelemetryBundle) => {
    setSelectedBundle(bundle);
    setShowCompareModal(true);
  };

  // Compare execution moved into GoldenCompareModal

  const handlePurge = useCallback(async () => {
    try {
      const result = await apiClient.purgeOldBundles(purgeKeepCount);
      setShowPurgeModal(false);
      // Refetch bundles
      const data = await apiClient.listTelemetryBundles();
      setBundles(data);
      // UI updates provide sufficient feedback for purge results
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Failed to purge bundles');
      setTelemetryError(error);
      logger.error('Failed to purge telemetry bundles', {
        component: 'Telemetry',
        operation: 'purgeBundles',
        keepCount: purgeKeepCount,
      }, toError(err));
    }
  }, [purgeKeepCount]);

  const handleBulkExportBundles = useCallback(async (bundleIds: string[]) => {
    try {
      const bundlesToExport = bundles.filter(b => bundleIds.includes(b.id));
      const exportPromises = bundlesToExport.map(bundle => apiClient.exportTelemetryBundle(bundle.id));
      const exportResults = await Promise.all(exportPromises);
      
      const exportData = {
        exported_at: new Date().toISOString(),
        bundle_count: bundlesToExport.length,
        bundles: exportResults.map((result, index) => ({
          bundle_id: result.bundle_id,
          events_count: result.events_count,
          size_bytes: result.size_bytes,
          download_url: result.download_url,
          expires_at: result.expires_at,
          bundle_info: bundlesToExport[index]
        }))
      };
      
      const dataStr = JSON.stringify(exportData, null, 2);
      const dataBlob = new Blob([dataStr], { type: 'application/json' });
      const url = URL.createObjectURL(dataBlob);
      const link = document.createElement('a');
      link.href = url;
      link.download = `telemetry-bundles-selected-export-${new Date().toISOString().split('T')[0]}.json`;
      document.body.appendChild(link);
      link.click();
      document.body.removeChild(link);
      URL.revokeObjectURL(url);
      toast.success(`Exported ${bundleIds.length} bundle(s).`);
      setSelectedBundles([]);
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Failed to export telemetry bundles');
      setTelemetryError(error);
      logger.error('Failed to export selected telemetry bundles', {
        component: 'Telemetry',
        operation: 'bulkExportBundles',
        bundleIds,
      }, toError(err));
    }
  }, [bundles]);

  const bulkActions: BulkAction[] = useMemo(() => [
    {
      id: 'export',
      label: 'Export Selected',
      handler: handleBulkExportBundles
    }
  ], [handleBulkExportBundles]);

  if (telemetryError) {
    return (
      <ErrorRecovery
        title="Telemetry Error"
        message={telemetryError.message}
        recoveryActions={[
          { label: 'Retry Loading', action: () => {
            setTelemetryError(null);
            // Trigger refetch by re-running useEffect
            window.location.reload();
          }},
          { label: 'View Logs', action: () => {/* Navigate to logs */} }
        ]}
      />
    );
  }

  if (loading) {
    return (
      <LoadingState
        title="Loading telemetry data"
        description="Gathering recent bundles and live stream status."
        skeletonLines={5}
      />
    );
  }

  useEffect(() => {
    if (!onToolbarChange) return;
    const toolbar = (
      <TelemetryToolbar
        density={density}
        onDensityChange={setDensity}
        connected={sseConnected}
        onExportAll={handleExportAllBundles}
        exportDisabled={bundles.length === 0}
        onPurge={() => setShowPurgeModal(true)}
      />
    );
    onToolbarChange(toolbar);
    return () => {
      onToolbarChange(null);
    };
  }, [onToolbarChange, density, setDensity, sseConnected, handleExportAllBundles, bundles.length]);

  return (
    <div className="space-y-6">

      <AdvancedFilter
        configs={telemetryFilterConfigs}
        values={filterValues}
        onChange={setFilterValues}
        className="mb-4"
        title="Filter Bundles"
      />

      <Card className="p-4 rounded-lg border border-border bg-card shadow-md">
        <CardHeader>
          <CardTitle>
            Event Bundles
            {filteredBundles.length !== bundles.length && (
              <span className="ml-2 text-sm font-normal text-muted-foreground">
                ({filteredBundles.length} of {bundles.length})
              </span>
            )}
          </CardTitle>
        </CardHeader>
        <CardContent>
          <Table className="border-collapse w-full">
            <TableHeader>
              <TableRow>
                <TableHead className="p-4 border-b border-border w-12">
                  <Checkbox
                    checked={
                      filteredBundles.length === 0
                        ? false
                        : selectedBundles.length === filteredBundles.length
                          ? true
                          : selectedBundles.length > 0
                            ? 'indeterminate'
                            : false
                    }
                    onCheckedChange={(checked) => {
                      if (checked) {
                        setSelectedBundles(filteredBundles.map(b => b.id));
                      } else {
                        setSelectedBundles([]);
                      }
                    }}
                    aria-label="Select all bundles"
                  />
                </TableHead>
                <TableHead>Bundle ID</TableHead>
                <TableHead role="columnheader" scope="col">
                  <HelpTooltip helpId="cpid">
                    <span>CPID</span>
                  </HelpTooltip>
                </TableHead>
                <TableHead role="columnheader" scope="col">Events</TableHead>
                <TableHead role="columnheader" scope="col">Size</TableHead>
                <TableHead role="columnheader" scope="col">
                  <HelpTooltip helpId="merkle-root">
                    <span>Merkle Root</span>
                  </HelpTooltip>
                </TableHead>
                <TableHead role="columnheader" scope="col">Created</TableHead>
                <TableHead role="columnheader" scope="col">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {filteredBundles.length === 0 ? (
                <TableRow role="row">
                  <TableCell colSpan={7} className="h-32" role="gridcell" aria-live="polite">
                    <EmptyState
                      icon={Activity}
                      title={bundles.length === 0 ? "No Telemetry Bundles Available" : "No Bundles Match Filters"}
                      description={bundles.length === 0
                        ? "Telemetry bundles will appear here as they are generated. Events are being captured in real-time."
                        : "Try adjusting your filters to see more results."}
                    />
                  </TableCell>
                </TableRow>
              ) : (
                <VirtualizedTableRows items={filteredBundles} estimateSize={60}>
                  {(bundle) => {
                    const bundleTyped = bundle as typeof filteredBundles[0];
                    return (
                      <TableRow key={bundleTyped.id}>
                        <TableCell className="p-4 border-b border-border">
                          <Checkbox
                            checked={selectedBundles.includes(bundleTyped.id)}
                            onCheckedChange={(checked) => {
                              if (checked) {
                                setSelectedBundles(prev => [...prev, bundleTyped.id]);
                              } else {
                                setSelectedBundles(prev => prev.filter(id => id !== bundleTyped.id));
                              }
                            }}
                            aria-label={`Select ${bundleTyped.id}`}
                          />
                        </TableCell>
                        <TableCell className="p-4 border-b border-border font-medium">{bundleTyped.id.substring(0, 8)}</TableCell>
                        <TableCell className="p-4 border-b border-border">{bundleTyped.cpid}</TableCell>
                        <TableCell className="p-4 border-b border-border">{bundleTyped.event_count.toLocaleString()}</TableCell>
                        <TableCell className="p-4 border-b border-border">{(bundleTyped.size_bytes / 1024 / 1024).toFixed(2)} MB</TableCell>
                        <TableCell className="p-4 border-b border-border font-mono text-xs">
                          {bundleTyped.merkle_root ? bundleTyped.merkle_root.substring(0, 16) : 'N/A'}
                        </TableCell>
                        <TableCell className="p-4 border-b border-border">{new Date(bundleTyped.created_at).toLocaleString()}</TableCell>
                        <TableCell className="p-4 border-b border-border">
                          <DropdownMenu>
                            <DropdownMenuTrigger asChild>
                              <Button variant="ghost" size="sm">
                                <MoreHorizontal className="icon-standard" />
                              </Button>
                            </DropdownMenuTrigger>
                            <DropdownMenuContent align="end">
                              <DropdownMenuItem onClick={() => handleExportBundle(bundleTyped)}>
                                <Download className="icon-standard mr-2" />
                                Export
                              </DropdownMenuItem>
                              <DropdownMenuItem onClick={() => handleVerifySignature(bundleTyped)}>
                                <Shield className="icon-standard mr-2" />
                                Verify Signature
                              </DropdownMenuItem>
                              <DropdownMenuItem onClick={() => handleCompareToGolden(bundleTyped)}>
                                <Eye className="icon-standard mr-2" />
                                Compare to Golden
                              </DropdownMenuItem>
                            </DropdownMenuContent>
                          </DropdownMenu>
                        </TableCell>
                      </TableRow>
                    );
                  }}
                </VirtualizedTableRows>
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      {/* Verify Signature Modal */}
      <Dialog open={showVerifyModal} onOpenChange={setShowVerifyModal}>
        <DialogContent className="max-w-4xl">
          <DialogHeader>
            <DialogTitle>Bundle Signature Verification</DialogTitle>
          </DialogHeader>
          {verifyResult && (
            <div className="space-y-3">
              <Alert variant={verifyResult.valid ? 'default' : 'destructive'}>
                <AlertDescription>
                  {verifyResult.valid ? '✓ Signature is valid' : '✗ Signature is invalid'}
                </AlertDescription>
              </Alert>
              
              {selectedBundle && (
                <HashChainView 
                  manifestHash={selectedBundle.manifest_hash_b3 || 'N/A'}
                  policyHash={selectedBundle.policy_hash_b3 || 'N/A'}
                  verified={verifyResult.valid}
                />
              )}
              
              <div className="form-field">
                <p className="form-label">Bundle ID</p>
                <p className="text-sm text-muted-foreground font-mono">{verifyResult.bundle_id}</p>
              </div>
              <div className="form-field">
                <p className="form-label">Signature</p>
                <p className="text-xs text-muted-foreground font-mono break-all">{verifyResult.signature}</p>
              </div>
              <div className="form-field">
                <p className="form-label">Signed By</p>
                <p className="text-sm text-muted-foreground">{verifyResult.signed_by}</p>
              </div>
              <div className="form-field">
                <p className="form-label">Signed At</p>
                <p className="text-sm text-muted-foreground">{useTimestamp(verifyResult.signed_at)}</p>
              </div>
              {verifyResult.verification_error && (
                <div className="mb-4">
                  <p className="font-medium text-sm mb-1 text-red-600">Error</p>
                  <p className="text-sm text-muted-foreground">{verifyResult.verification_error}</p>
                </div>
              )}

              {/* Verification receipt actions */}
              <div className="flex gap-2">
                <Button
                  variant="outline"
                  onClick={() => {
                    const receipt = {
                      bundle_id: verifyResult.bundle_id,
                      signature: verifyResult.signature,
                      signed_by: verifyResult.signed_by,
                      signed_at: verifyResult.signed_at,
                      valid: verifyResult.valid,
                      verification_error: verifyResult.verification_error,
                    };
                    navigator.clipboard.writeText(JSON.stringify(receipt, null, 2));
                    // Browser clipboard API provides feedback
                  }}
                >
                  Copy Receipt
                </Button>
                <Button
                  onClick={() => {
                    const receipt = {
                      bundle_id: verifyResult.bundle_id,
                      signature: verifyResult.signature,
                      signed_by: verifyResult.signed_by,
                      signed_at: verifyResult.signed_at,
                      valid: verifyResult.valid,
                      verification_error: verifyResult.verification_error,
                    };
                    const dataStr = JSON.stringify(receipt, null, 2);
                    const blob = new Blob([dataStr], { type: 'application/json' });
                    const url = URL.createObjectURL(blob);
                    const link = document.createElement('a');
                    link.href = url;
                    link.download = `verification-receipt-${verifyResult.bundle_id}.json`;
                    document.body.appendChild(link);
                    link.click();
                    document.body.removeChild(link);
                    URL.revokeObjectURL(url);
                  }}
                >
                  Download Receipt
                </Button>
              </div>
            </div>
          )}
          <DialogFooter>
            <Button onClick={() => setShowVerifyModal(false)}>Close</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Purge Bundles Modal */}
      <Dialog open={showPurgeModal} onOpenChange={setShowPurgeModal}>
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle>Purge Old Telemetry Bundles</DialogTitle>
          </DialogHeader>
          <Alert variant="destructive">
            <AlertDescription>
              This will delete old telemetry bundles based on retention policy. This action cannot be undone.
            </AlertDescription>
          </Alert>
          <div className="mb-4">
            <label className="font-medium text-sm mb-1">Keep Latest Bundles Per CPID</label>
            <input
              type="number"
              className="w-full p-2 border rounded"
              value={purgeKeepCount}
              onChange={(e) => setPurgeKeepCount(parseInt(e.target.value) || 12)}
              min={1}
              max={100}
            />
            <p className="text-xs text-muted-foreground">
              Older bundles will be deleted, keeping only the most recent {purgeKeepCount} per CPID
            </p>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setShowPurgeModal(false)}>
              Cancel
            </Button>
            <Button variant="destructive" onClick={handlePurge}>
              Purge Bundles
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Compare to Golden Modal */}
      <GoldenCompareModal
        open={showCompareModal}
        onOpenChange={setShowCompareModal}
        bundleId={selectedBundle ? selectedBundle.id : null}
      />

      {/* Bulk Action Bar */}
      <BulkActionBar
        selectedItems={selectedBundles}
        actions={bulkActions}
        onClearSelection={() => setSelectedBundles([])}
        itemName="bundle"
      />
    </div>
  );
}
