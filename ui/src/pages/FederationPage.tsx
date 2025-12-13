// Federation Status Page - View federation health, node sync status, and quarantine management
import React, { useState, useCallback } from 'react';
import PageWrapper from '@/layout/PageWrapper';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { apiClient } from '@/api/client';
import { FederationStatusResponse, FederationAuditResponse, QuarantineStatusResponse, PeerListResponse } from '@/api/federation-types';
import { useDensity } from '@/contexts/DensityContext';
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
import PageTable from '@/components/ui/PageTable';

const LoadingSpinner = () => (
  <div className="flex justify-center py-8">
    <div className="h-8 w-8 animate-spin rounded-full border-b-2 border-primary"></div>
  </div>
);

const formatTimestamp = (timestamp: string) => new Date(timestamp).toLocaleString();

function StatusBadge({ operational, quarantined }: { operational: boolean; quarantined: boolean }) {
  if (quarantined) {
    return (
      <Badge variant="destructive" className="flex items-center gap-1">
        <ShieldAlert className="h-3 w-3" />
        Quarantined
      </Badge>
    );
  }
  if (operational) {
    return (
      <Badge variant="default" className="flex items-center gap-1">
        <CheckCircle className="h-3 w-3" />
        Operational
      </Badge>
    );
  }
  return (
    <Badge variant="secondary" className="flex items-center gap-1">
      <AlertTriangle className="h-3 w-3" />
      Degraded
    </Badge>
  );
}

const VerificationBadge = ({ hasVerification }: { hasVerification: boolean }) =>
  hasVerification ? <Badge variant="outline">Available</Badge> : <Badge variant="secondary">None</Badge>;

function StatusStat({
  label,
  value,
  valueClassName,
}: {
  label: string;
  value: React.ReactNode;
  valueClassName?: string;
}) {
  return (
    <div>
      <div className="mb-1 text-sm text-muted-foreground">{label}</div>
      <div className={valueClassName}>{value}</div>
    </div>
  );
}

function QuarantineNotice({
  reason,
  canRelease,
  onReleaseClick,
}: {
  reason: string;
  canRelease: boolean;
  onReleaseClick: () => void;
}) {
  return (
    <Alert variant="destructive">
      <ShieldAlert className="h-4 w-4" />
      <AlertDescription>
        <div className="flex flex-wrap items-center gap-3">
          <span>
            <strong>Quarantine Active:</strong> {reason}
          </span>
          {canRelease && (
            <Button variant="outline" size="sm" onClick={onReleaseClick}>
              Release Quarantine
            </Button>
          )}
        </div>
      </AlertDescription>
    </Alert>
  );
}

function QuarantineDetails({
  details,
  canRelease,
  onReleaseClick,
}: {
  details: NonNullable<QuarantineStatusResponse['details']>;
  canRelease: boolean;
  onReleaseClick: () => void;
}) {
  return (
    <div className="space-y-4">
      <Alert variant="destructive">
        <ShieldAlert className="h-4 w-4" />
        <AlertDescription>
          <div className="space-y-2">
            <div>
              <strong>Reason:</strong> {details.reason}
            </div>
            <div>
              <strong>Violation Type:</strong> {details.violation_type}
            </div>
            <div>
              <strong>Triggered At:</strong> {formatTimestamp(details.triggered_at)}
            </div>
            {details.cpid && (
              <div>
                <strong>Control Plane ID:</strong> {details.cpid}
              </div>
            )}
          </div>
        </AlertDescription>
      </Alert>
      {canRelease && (
        <Button variant="outline" onClick={onReleaseClick}>
          <ShieldAlert className="mr-2 h-4 w-4" />
          Release Quarantine
        </Button>
      )}
    </div>
  );
}

function ControlsCard({
  onRefresh,
  lastUpdated,
  isLoading,
}: {
  onRefresh: () => void;
  lastUpdated?: Date;
  isLoading: boolean;
}) {
  return (
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
        <div className="flex flex-wrap items-center gap-4">
          <Button onClick={onRefresh} disabled={isLoading} variant="outline">
            <RefreshCw className={`h-4 w-4 mr-2 ${isLoading ? 'animate-spin' : ''}`} />
            Refresh All
          </Button>
          {lastUpdated && (
            <span className="text-xs text-muted-foreground">
              Last updated: {lastUpdated.toLocaleTimeString()}
            </span>
          )}
        </div>
      </CardContent>
    </Card>
  );
}

function StatusSummaryCard({
  status,
  isLoading,
  error,
  onRetry,
  canRelease,
  onReleaseClick,
}: {
  status?: FederationStatusResponse;
  isLoading: boolean;
  error: Error | null;
  onRetry: () => void;
  canRelease: boolean;
  onReleaseClick: () => void;
}) {
  return (
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
        {error && errorRecoveryTemplates.genericError(error.message, onRetry)}
        {isLoading && !status && <LoadingSpinner />}
        {status ? (
          <div className="space-y-4">
            <div className="grid grid-cols-1 gap-4 rounded-lg border p-4 md:grid-cols-2 lg:grid-cols-4">
              <StatusStat
                label="Status"
                value={<StatusBadge operational={status.operational} quarantined={status.quarantined} />}
                valueClassName=""
              />
              <StatusStat label="Total Hosts" value={status.total_hosts} valueClassName="text-2xl font-semibold" />
              <StatusStat
                label="Last Updated"
                value={formatTimestamp(status.timestamp)}
                valueClassName="text-sm"
              />
              <StatusStat
                label="Verification"
                value={<VerificationBadge hasVerification={!!status.latest_verification} />}
                valueClassName="text-sm"
              />
            </div>
            {status.quarantined && status.quarantine_reason && (
              <QuarantineNotice
                reason={status.quarantine_reason}
                canRelease={canRelease}
                onReleaseClick={onReleaseClick}
              />
            )}
          </div>
        ) : (
          !isLoading && <div className="py-8 text-center text-muted-foreground">No federation status data available</div>
        )}
      </CardContent>
    </Card>
  );
}

function QuarantineCard({
  status,
  isLoading,
  error,
  onRetry,
  canRelease,
  onReleaseClick,
}: {
  status?: QuarantineStatusResponse;
  isLoading: boolean;
  error: Error | null;
  onRetry: () => void;
  canRelease: boolean;
  onReleaseClick: () => void;
}) {
  const showDetails = status?.quarantined && status.details;

  return (
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
        {error && errorRecoveryTemplates.genericError(error.message, onRetry)}
        {isLoading && !status && <LoadingSpinner />}
        {showDetails && status?.details && (
          <QuarantineDetails
            details={status.details}
            canRelease={canRelease}
            onReleaseClick={onReleaseClick}
          />
        )}
        {!showDetails && !isLoading && (
          <div className="flex items-center gap-2 text-green-600">
            <CheckCircle className="h-5 w-5" />
            <span>No active quarantine</span>
          </div>
        )}
        {!status && !isLoading && !error && (
          <div className="py-8 text-center text-muted-foreground">No quarantine status data available</div>
        )}
      </CardContent>
    </Card>
  );
}

function HostChainsTable({ chains }: { chains: FederationAuditResponse['host_chains'] }) {
  if (!chains || chains.length === 0) return null;
  return (
    <div className="mt-6">
      <h3 className="mb-4 text-lg font-semibold">Host Chains</h3>
      <PageTable minWidth="md">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Host ID</TableHead>
              <TableHead>Bundle Count</TableHead>
              <TableHead>Latest Bundle</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {chains.map((host) => (
              <TableRow key={host.host_id}>
                <TableCell className="font-mono text-sm">{host.host_id}</TableCell>
                <TableCell>{host.bundle_count}</TableCell>
                <TableCell className="max-w-xs truncate font-mono text-xs">
                  {host.latest_bundle || 'N/A'}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </PageTable>
    </div>
  );
}

function AuditCard({
  audit,
  isLoading,
  error,
  onRetry,
}: {
  audit?: FederationAuditResponse;
  isLoading: boolean;
  error: Error | null;
  onRetry: () => void;
}) {
  return (
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
        {error && errorRecoveryTemplates.genericError(error.message, onRetry)}
        {isLoading && !audit && <LoadingSpinner />}
        {audit ? (
          <div className="space-y-4">
            <div className="grid grid-cols-1 gap-4 rounded-lg border p-4 md:grid-cols-2 lg:grid-cols-4">
              <div>
                <div className="mb-1 text-sm text-muted-foreground">Total Hosts</div>
                <div className="text-2xl font-semibold">{audit.total_hosts}</div>
              </div>
              <div>
                <div className="mb-1 text-sm text-muted-foreground">Total Signatures</div>
                <div className="text-2xl font-semibold">{audit.total_signatures}</div>
              </div>
              <div>
                <div className="mb-1 text-sm text-muted-foreground">Verified</div>
                <div className="text-2xl font-semibold text-green-600">
                  {audit.verified_signatures}
                </div>
              </div>
              <div>
                <div className="mb-1 text-sm text-muted-foreground">Verification Rate</div>
                <div className="text-2xl font-semibold">
                  {audit.total_signatures > 0
                    ? Math.round((audit.verified_signatures / audit.total_signatures) * 100)
                    : 0}
                  %
                </div>
              </div>
            </div>
            <HostChainsTable chains={audit.host_chains} />
          </div>
        ) : (
          !isLoading && <div className="py-8 text-center text-muted-foreground">No sync data available</div>
        )}
      </CardContent>
    </Card>
  );
}

function ReleaseQuarantineDialog({
  isOpen,
  onClose,
  onConfirm,
  isLoading,
}: {
  isOpen: boolean;
  onClose: (open: boolean) => void;
  onConfirm: () => void;
  isLoading: boolean;
}) {
  return (
    <Dialog open={isOpen} onOpenChange={onClose}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Release Quarantine</DialogTitle>
          <DialogDescription>
            Are you sure you want to release this node from quarantine? This will allow it to participate in federation again.
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="outline" onClick={() => onClose(false)} disabled={isLoading}>
            Cancel
          </Button>
          <Button onClick={onConfirm} disabled={isLoading}>
            {isLoading && <RefreshCw className="mr-2 h-4 w-4 animate-spin" />}
            Release Quarantine
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

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
      <PageWrapper
        pageKey="federation"
        title="Federation Status"
        description="Cross-node synchronization and health"
        maxWidth="xl"
        contentPadding="default"
      >
        <ErrorRecovery
          error="You do not have permission to view federation status. This page requires federation:view or audit:view permission."
          onRetry={() => window.location.reload()}
        />
      </PageWrapper>
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

  return (
    <PageWrapper
      pageKey="federation"
      title="Federation Status"
      description="Cross-node synchronization and health monitoring"
      maxWidth="xl"
      contentPadding="default"
      headerActions={<DensityControls density={density} onDensityChange={setDensity} />}
    >
      <SectionErrorBoundary sectionName="Federation Status">
        <div className="space-y-6">
          <ControlsCard
            onRefresh={handleRefreshAll}
            lastUpdated={statusLastUpdated}
            isLoading={statusLoading}
          />
          <StatusSummaryCard
            status={federationStatus}
            isLoading={statusLoading}
            error={statusError instanceof Error ? statusError : statusError ? new Error(String(statusError)) : null}
            onRetry={refetchStatus}
            canRelease={canReleaseFederation}
            onReleaseClick={() => setQuarantineDialogOpen(true)}
          />
          {peersError && errorRecoveryTemplates.genericError(peersError.message, refetchPeers)}
          {peersData && (
            <PeerSyncStatusCard
              peers={derivePeerSyncInfoList(peersData.peers)}
              isLoading={peersLoading}
              showTitle={true}
              compact={false}
            />
          )}
          <QuarantineCard
            status={quarantineStatus}
            isLoading={quarantineLoading}
            error={quarantineError instanceof Error ? quarantineError : quarantineError ? new Error(String(quarantineError)) : null}
            onRetry={refetchQuarantine}
            canRelease={canReleaseFederation}
            onReleaseClick={() => setQuarantineDialogOpen(true)}
          />
          <AuditCard
            audit={auditData}
            isLoading={auditLoading}
            error={auditError instanceof Error ? auditError : auditError ? new Error(String(auditError)) : null}
            onRetry={refetchAudit}
          />
        </div>
      </SectionErrorBoundary>
      <ReleaseQuarantineDialog
        isOpen={quarantineDialogOpen}
        onClose={setQuarantineDialogOpen}
        onConfirm={handleReleaseQuarantine}
        isLoading={releasing}
      />
    </PageWrapper>
  );
}

export default function FederationPage() {
  return <FederationPageInner />;
}
