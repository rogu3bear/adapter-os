import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './ui/table';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from './ui/dialog';
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from './ui/dropdown-menu';
import { Activity, Download, Eye, CheckCircle, MoreHorizontal, Shield, Trash2 } from 'lucide-react';
import { Alert, AlertDescription } from './ui/alert';
import { EmptyState } from './ui/empty-state';
import apiClient from '../api/client';
import { TelemetryBundle, User, VerifyBundleSignatureResponse } from '../api/types';
import { useSSE } from '../hooks/useSSE';
import { useTimestamp } from '../hooks/useTimestamp';
import { canonicalKey } from './ui/utils';
import { HashChainView } from './HashChainView';
import { HelpTooltip } from './ui/help-tooltip';
import { toast } from 'sonner';

import { useAuth, useTenant } from '@/layout/LayoutProvider';
import { GoldenCompareModal } from './GoldenCompareModal';
import { logger, toError } from '../utils/logger';

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
  const [showVerifyModal, setShowVerifyModal] = useState(false);
  const [showCompareModal, setShowCompareModal] = useState(false);
  const [showPurgeModal, setShowPurgeModal] = useState(false);
  const [verifyResult, setVerifyResult] = useState<VerifyBundleSignatureResponse | null>(null);
  const [selectedBundle, setSelectedBundle] = useState<TelemetryBundle | null>(null);
  const [purgeKeepCount, setPurgeKeepCount] = useState(12);
  // Golden compare modal is encapsulated in its own component

  // SSE for real-time bundle notifications
  const { data: sseBundles, connected } = useSSE<TelemetryBundle[]>('/v1/stream/telemetry');

  useEffect(() => {
    const fetchBundles = async () => {
      try {
        const data = await apiClient.listTelemetryBundles();
        setBundles(data);
      } catch (err) {
        const errorMsg = err instanceof Error ? err.message : 'Failed to fetch telemetry bundles';
        logger.error('Failed to fetch telemetry bundles', {
          component: 'Telemetry',
          operation: 'fetchBundles',
          tenantId: effectiveTenant,
          errorMessage: errorMsg,
        }, toError(err));
        toast.error(errorMsg);
      } finally {
        setLoading(false);
      }
    };
    fetchBundles();
  }, [effectiveTenant]);

  // Update bundles from SSE stream
  useEffect(() => {
    if (sseBundles && Array.isArray(sseBundles)) {
      setBundles(prev => [...sseBundles, ...prev].slice(0, 100)); // Keep last 100
      if (sseBundles.length > 0) {
        toast.info(`${sseBundles.length} new telemetry bundle(s) available`);
      }
    }
  }, [sseBundles]);

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
    toast.success(`Bundle ${bundle.id.substring(0, 8)} exported`);
  };

  const handleVerifySignature = async (bundle: TelemetryBundle) => {
    try {
      const result = await apiClient.verifyBundleSignature(bundle.id);
      setVerifyResult(result);
      setSelectedBundle(bundle);
      setShowVerifyModal(true);
      toast.success(result.valid ? 'Signature valid' : 'Signature invalid');
    } catch (err) {
      toast.error('Failed to verify signature');
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

  const handlePurge = async () => {
    try {
      const result = await apiClient.purgeOldBundles(purgeKeepCount);
      toast.success(`Purged ${result.purged_count} bundles, kept ${result.retained_count}`);
      setShowPurgeModal(false);
      // Refetch bundles
      const data = await apiClient.listTelemetryBundles();
      setBundles(data);
    } catch (err) {
      toast.error('Failed to purge bundles');
      logger.error('Failed to purge telemetry bundles', {
        component: 'Telemetry',
        operation: 'purgeBundles',
        keepCount: purgeKeepCount,
      }, toError(err));
    }
  };

  if (loading) {
    return <div className="text-center p-8">Loading telemetry data...</div>;
  }

  return (
    <div className="space-y-6">
      <div className="flex-between section-header">
        <div>
          <h1 className="section-title">Telemetry Bundles</h1>
          <p className="section-description">
            View and export telemetry data for audit and compliance
          </p>
        </div>
        <div className="flex-center">
          <div className={connected ? "status-indicator status-success" : "status-indicator status-neutral"}>
            <Activity className="icon-small mr-1" />
            {connected ? 'Capturing Events (Live)' : 'Capturing Events'}
          </div>
          <Button variant="destructive" onClick={() => setShowPurgeModal(true)}>
            <Trash2 className="icon-standard mr-2" />
            Purge Old Bundles
          </Button>
        </div>
      </div>

      <Card className="card-standard">
        <CardHeader>
          <CardTitle>Event Bundles</CardTitle>
        </CardHeader>
        <CardContent>
          <Table className="table-standard">
            <TableHeader>
              <TableRow>
                <TableHead>Bundle ID</TableHead>
                <TableHead>
                  <HelpTooltip helpId="cpid">
                    <span>CPID</span>
                  </HelpTooltip>
                </TableHead>
                <TableHead>Events</TableHead>
                <TableHead>Size</TableHead>
                <TableHead>
                  <HelpTooltip helpId="merkle-root">
                    <span>Merkle Root</span>
                  </HelpTooltip>
                </TableHead>
                <TableHead>Created</TableHead>
                <TableHead>Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {bundles.length === 0 ? (
                <TableRow>
                  <TableCell colSpan={7} className="h-32">
                    <EmptyState
                      icon={Activity}
                      title="No Telemetry Bundles Available"
                      description="Telemetry bundles will appear here as they are generated. Events are being captured in real-time."
                    />
                  </TableCell>
                </TableRow>
              ) : (
                bundles.map((bundle) => (
                  <TableRow key={bundle.id}>
                  <TableCell className="table-cell-standard font-medium">{bundle.id.substring(0, 8)}</TableCell>
                  <TableCell className="table-cell-standard">{bundle.cpid}</TableCell>
                  <TableCell className="table-cell-standard">{bundle.event_count.toLocaleString()}</TableCell>
                  <TableCell className="table-cell-standard">{(bundle.size_bytes / 1024 / 1024).toFixed(2)} MB</TableCell>
                  <TableCell className="table-cell-standard font-mono text-xs">
                    {bundle.merkle_root.substring(0, 16)}
                  </TableCell>
                  <TableCell className="table-cell-standard">{new Date(bundle.created_at).toLocaleString()}</TableCell>
                  <TableCell className="table-cell-standard">
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        <Button variant="ghost" size="sm">
                          <MoreHorizontal className="icon-standard" />
                        </Button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end">
                        <DropdownMenuItem onClick={() => handleExportBundle(bundle)}>
                          <Download className="icon-standard mr-2" />
                          Export
                        </DropdownMenuItem>
                        <DropdownMenuItem onClick={() => handleVerifySignature(bundle)}>
                          <Shield className="icon-standard mr-2" />
                          Verify Signature
                        </DropdownMenuItem>
                        <DropdownMenuItem onClick={() => handleCompareToGolden(bundle)}>
                          <Eye className="icon-standard mr-2" />
                          Compare to Golden
                        </DropdownMenuItem>
                      </DropdownMenuContent>
                    </DropdownMenu>
                  </TableCell>
                </TableRow>
                ))
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      {/* Verify Signature Modal */}
      <Dialog open={showVerifyModal} onOpenChange={setShowVerifyModal}>
        <DialogContent className="modal-large">
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
                <div className="form-field">
                  <p className="form-label text-red-600">Error</p>
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
                    toast.success('Verification receipt copied');
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
        <DialogContent className="modal-standard">
          <DialogHeader>
            <DialogTitle>Purge Old Telemetry Bundles</DialogTitle>
          </DialogHeader>
          <Alert variant="destructive">
            <AlertDescription>
              This will delete old telemetry bundles based on retention policy. This action cannot be undone.
            </AlertDescription>
          </Alert>
          <div className="form-field">
            <label className="form-label">Keep Latest Bundles Per CPID</label>
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
    </div>
  );
}
