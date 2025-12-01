// Federation Status Page - View federation health, node sync status, and quarantine management
import React, { useState, useCallback } from 'react';
import FeatureLayout from '@/layout/FeatureLayout';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { apiClient } from '@/api/client';
import { FederationStatusResponse, FederationAuditResponse, QuarantineStatusResponse, PeerListResponse } from '@/api/federation-types';
import { DensityProvider, useDensity } from '@/contexts/DensityContext';
import { PeerSyncStatusCard } from '@/components/federation/PeerSyncStatusCard';
import { derivePeerSyncInfoList } from '@/utils/peerSync';
import { DensityControls } from '@/components/ui/density-controls';
import { useRBAC } from '@/hooks/useRBAC';
import { ErrorRecovery, errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { usePolling } from '@/hooks/usePolling';
import { RefreshCw, ShieldAlert, CheckCircle, AlertTriangle, Server, Activity } from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { toast } from 'sonner';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';

function FederationPageInner() {
  const { density, setDensity } = useDensity();
  const { can } = useRBAC();
  const [quarantineDialogOpen, setQuarantineDialogOpen] = useState(false);
  const [releasing, setReleasing] = useState(false);

  // RBAC: Check permissions (typically admin or SRE)
  const canViewFederation = can('federation:view') || can('audit:view');
  const canReleaseFederation = can('federation:manage') || can('admin');

  if (!canViewFederation) {
    return (
      <FeatureLayout
        title="Federation Status"
        description="Cross-node synchronization and health"
      >
        <ErrorRecovery
          error="You do not have permission to view federation status. This page requires federation:view or audit:view permission."
          onRetry={() => window.location.reload()}
        />
      </FeatureLayout>
    );
  }

  // Fetch federation status
  const fetchFederationStatus = useCallback(async () => {
    return await apiClient.getFederationStatus();
  }, []);

  const {
    data: federationStatus,
    isLoading: statusLoading,
    error: statusError,
    refetch: refetchStatus,
    lastUpdated: statusLastUpdated
  } = usePolling<FederationStatusResponse>(
    fetchFederationStatus,
    'normal', // 10s polling
    {
      enabled: true,
      operationName: 'fetchFederationStatus',
    }
  );

  // Fetch quarantine status
  const fetchQuarantineStatus = useCallback(async () => {
    return await apiClient.getQuarantineStatus();
  }, []);

  const {
    data: quarantineStatus,
    isLoading: quarantineLoading,
    error: quarantineError,
    refetch: refetchQuarantine,
  } = usePolling<QuarantineStatusResponse>(
    fetchQuarantineStatus,
    'normal',
    {
      enabled: true,
      operationName: 'fetchQuarantineStatus',
    }
  );

  // Fetch federation audit
  const fetchFederationAudit = useCallback(async () => {
    return await apiClient.getFederationAudit({ limit: 100 });
  }, []);

  const {
    data: auditData,
    isLoading: auditLoading,
    error: auditError,
    refetch: refetchAudit,
  } = usePolling<FederationAuditResponse>(
    fetchFederationAudit,
    'slow', // 30s polling
    {
      enabled: true,
      operationName: 'fetchFederationAudit',
    }
  );

  // Fetch peer list for sync status
  const fetchPeers = useCallback(async () => {
    return await apiClient.getFederationPeers();
  }, []);

  const {
    data: peersData,
    isLoading: peersLoading,
    error: peersError,
    refetch: refetchPeers,
  } = usePolling<PeerListResponse>(
    fetchPeers,
    'normal', // 10s polling
    {
      enabled: true,
      operationName: 'fetchFederationPeers',
    }
  );

  const handleReleaseQuarantine = async () => {
    setReleasing(true);
    try {
      const result = await apiClient.releaseQuarantine({ reason: 'Manual release from UI' });
      if (result.success) {
        toast.success('Quarantine released successfully');
        refetchStatus();
        refetchQuarantine();
        setQuarantineDialogOpen(false);
      } else {
        toast.error(result.message || 'Failed to release quarantine');
      }
    } catch (error) {
      toast.error(error instanceof Error ? error.message : 'Failed to release quarantine');
    } finally {
      setReleasing(false);
    }
  };

  const handleRefreshAll = () => {
    refetchStatus();
    refetchQuarantine();
    refetchAudit();
    refetchPeers();
  };

  const formatTimestamp = (timestamp: string) => {
    return new Date(timestamp).toLocaleString();
  };

  const getStatusBadge = (operational: boolean, quarantined: boolean) => {
    if (quarantined) {
      return <Badge variant="destructive" className="flex items-center gap-1">
        <ShieldAlert className="h-3 w-3" />
        Quarantined
      </Badge>;
    }
    if (operational) {
      return <Badge variant="default" className="flex items-center gap-1">
        <CheckCircle className="h-3 w-3" />
        Operational
      </Badge>;
    }
    return <Badge variant="secondary" className="flex items-center gap-1">
      <AlertTriangle className="h-3 w-3" />
      Degraded
    </Badge>;
  };

  return (
    <FeatureLayout
      title="Federation Status"
      description="Cross-node synchronization and health monitoring"
      headerActions={<DensityControls density={density} onDensityChange={setDensity} />}
    >
      <SectionErrorBoundary sectionName="Federation Status">
        <div className="space-y-6">
          {/* Controls */}
          <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              Controls
              <GlossaryTooltip termId="federation-controls">
                <span className="cursor-help text-muted-foreground">(?)</span>
              </GlossaryTooltip>
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="flex gap-4 items-center flex-wrap">
              <Button onClick={handleRefreshAll} disabled={statusLoading} variant="outline">
                <RefreshCw className={`h-4 w-4 mr-2 ${statusLoading ? 'animate-spin' : ''}`} />
                Refresh All
              </Button>
              {statusLastUpdated && (
                <span className="text-xs text-muted-foreground">
                  Last updated: {statusLastUpdated.toLocaleTimeString()}
                </span>
              )}
            </div>
          </CardContent>
        </Card>

        {/* Federation Status Summary */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Server className="h-5 w-5" />
              Federation Status
              <GlossaryTooltip termId="federation-status">
                <span className="cursor-help text-muted-foreground">(?)</span>
              </GlossaryTooltip>
            </CardTitle>
          </CardHeader>
          <CardContent>
            {statusError && errorRecoveryTemplates.genericError(statusError.message, refetchStatus)}

            {statusLoading && !federationStatus ? (
              <div className="flex justify-center py-8">
                <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary"></div>
              </div>
            ) : federationStatus ? (
              <div className="space-y-4">
                <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
                  <div className="p-4 border rounded-lg">
                    <div className="text-sm text-muted-foreground mb-1">Status</div>
                    <div>{getStatusBadge(federationStatus.operational, federationStatus.quarantined)}</div>
                  </div>
                  <div className="p-4 border rounded-lg">
                    <div className="text-sm text-muted-foreground mb-1">Total Hosts</div>
                    <div className="text-2xl font-semibold">{federationStatus.total_hosts}</div>
                  </div>
                  <div className="p-4 border rounded-lg">
                    <div className="text-sm text-muted-foreground mb-1">Last Updated</div>
                    <div className="text-sm">{formatTimestamp(federationStatus.timestamp)}</div>
                  </div>
                  <div className="p-4 border rounded-lg">
                    <div className="text-sm text-muted-foreground mb-1">Verification</div>
                    <div className="text-sm">
                      {federationStatus.latest_verification ? (
                        <Badge variant="outline">Available</Badge>
                      ) : (
                        <Badge variant="secondary">None</Badge>
                      )}
                    </div>
                  </div>
                </div>

                {federationStatus.quarantined && federationStatus.quarantine_reason && (
                  <Alert variant="destructive">
                    <ShieldAlert className="h-4 w-4" />
                    <AlertDescription>
                      <strong>Quarantine Active:</strong> {federationStatus.quarantine_reason}
                      {canReleaseFederation && (
                        <Button
                          variant="outline"
                          size="sm"
                          className="ml-4"
                          onClick={() => setQuarantineDialogOpen(true)}
                        >
                          Release Quarantine
                        </Button>
                      )}
                    </AlertDescription>
                  </Alert>
                )}
              </div>
            ) : (
              <div className="text-center py-8 text-muted-foreground">
                No federation status data available
              </div>
            )}
          </CardContent>
        </Card>

        {/* Peer Sync Status */}
        {peersError && errorRecoveryTemplates.genericError(peersError.message, refetchPeers)}
        {peersData && (
          <PeerSyncStatusCard
            peers={derivePeerSyncInfoList(peersData.peers)}
            isLoading={peersLoading}
            showTitle={true}
            compact={false}
          />
        )}

        {/* Quarantine Status */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <ShieldAlert className="h-5 w-5" />
              Quarantine Status
              <GlossaryTooltip termId="quarantine-status">
                <span className="cursor-help text-muted-foreground">(?)</span>
              </GlossaryTooltip>
            </CardTitle>
          </CardHeader>
          <CardContent>
            {quarantineError && errorRecoveryTemplates.genericError(quarantineError.message, refetchQuarantine)}

            {quarantineLoading && !quarantineStatus ? (
              <div className="flex justify-center py-8">
                <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary"></div>
              </div>
            ) : quarantineStatus ? (
              quarantineStatus.quarantined && quarantineStatus.details ? (
                <div className="space-y-4">
                  <Alert variant="destructive">
                    <ShieldAlert className="h-4 w-4" />
                    <AlertDescription>
                      <div className="space-y-2">
                        <div><strong>Reason:</strong> {quarantineStatus.details.reason}</div>
                        <div><strong>Violation Type:</strong> {quarantineStatus.details.violation_type}</div>
                        <div><strong>Triggered At:</strong> {formatTimestamp(quarantineStatus.details.triggered_at)}</div>
                        {quarantineStatus.details.cpid && (
                          <div><strong>Control Plane ID:</strong> {quarantineStatus.details.cpid}</div>
                        )}
                      </div>
                    </AlertDescription>
                  </Alert>
                  {canReleaseFederation && (
                    <Button
                      variant="outline"
                      onClick={() => setQuarantineDialogOpen(true)}
                    >
                      <ShieldAlert className="h-4 w-4 mr-2" />
                      Release Quarantine
                    </Button>
                  )}
                </div>
              ) : (
                <div className="flex items-center gap-2 text-green-600">
                  <CheckCircle className="h-5 w-5" />
                  <span>No active quarantine</span>
                </div>
              )
            ) : (
              <div className="text-center py-8 text-muted-foreground">
                No quarantine status data available
              </div>
            )}
          </CardContent>
        </Card>

        {/* Federation Audit Summary */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Activity className="h-5 w-5" />
              Sync Summary
              <GlossaryTooltip termId="federation-audit">
                <span className="cursor-help text-muted-foreground">(?)</span>
              </GlossaryTooltip>
            </CardTitle>
          </CardHeader>
          <CardContent>
            {auditError && errorRecoveryTemplates.genericError(auditError.message, refetchAudit)}

            {auditLoading && !auditData ? (
              <div className="flex justify-center py-8">
                <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary"></div>
              </div>
            ) : auditData ? (
              <div className="space-y-4">
                <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
                  <div className="p-4 border rounded-lg">
                    <div className="text-sm text-muted-foreground mb-1">Total Hosts</div>
                    <div className="text-2xl font-semibold">{auditData.total_hosts}</div>
                  </div>
                  <div className="p-4 border rounded-lg">
                    <div className="text-sm text-muted-foreground mb-1">Total Signatures</div>
                    <div className="text-2xl font-semibold">{auditData.total_signatures}</div>
                  </div>
                  <div className="p-4 border rounded-lg">
                    <div className="text-sm text-muted-foreground mb-1">Verified</div>
                    <div className="text-2xl font-semibold text-green-600">{auditData.verified_signatures}</div>
                  </div>
                  <div className="p-4 border rounded-lg">
                    <div className="text-sm text-muted-foreground mb-1">Verification Rate</div>
                    <div className="text-2xl font-semibold">
                      {auditData.total_signatures > 0
                        ? Math.round((auditData.verified_signatures / auditData.total_signatures) * 100)
                        : 0}%
                    </div>
                  </div>
                </div>

                {/* Host Chains Table */}
                {auditData.host_chains && auditData.host_chains.length > 0 && (
                  <div className="mt-6">
                    <h3 className="text-lg font-semibold mb-4">Host Chains</h3>
                    <div className="overflow-x-auto">
                      <Table>
                        <TableHeader>
                          <TableRow>
                            <TableHead>Host ID</TableHead>
                            <TableHead>Bundle Count</TableHead>
                            <TableHead>Latest Bundle</TableHead>
                          </TableRow>
                        </TableHeader>
                        <TableBody>
                          {auditData.host_chains.map((host) => (
                            <TableRow key={host.host_id}>
                              <TableCell className="font-mono text-sm">{host.host_id}</TableCell>
                              <TableCell>{host.bundle_count}</TableCell>
                              <TableCell className="font-mono text-xs truncate max-w-xs">
                                {host.latest_bundle || 'N/A'}
                              </TableCell>
                            </TableRow>
                          ))}
                        </TableBody>
                      </Table>
                    </div>
                  </div>
                )}
              </div>
            ) : (
              <div className="text-center py-8 text-muted-foreground">
                No sync data available
              </div>
            )}
          </CardContent>
        </Card>
        </div>
      </SectionErrorBoundary>

      {/* Release Quarantine Dialog */}
      <Dialog open={quarantineDialogOpen} onOpenChange={setQuarantineDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Release Quarantine</DialogTitle>
            <DialogDescription>
              Are you sure you want to release this node from quarantine? This will allow it to participate in
              federation again.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setQuarantineDialogOpen(false)}
              disabled={releasing}
            >
              Cancel
            </Button>
            <Button
              onClick={handleReleaseQuarantine}
              disabled={releasing}
            >
              {releasing && <RefreshCw className="h-4 w-4 mr-2 animate-spin" />}
              Release Quarantine
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </FeatureLayout>
  );
}

export default function FederationPage() {
  return (
    <DensityProvider pageKey="federation">
      <FederationPageInner />
    </DensityProvider>
  );
}
