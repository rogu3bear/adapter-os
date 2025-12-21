import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { Badge } from '@/components/ui/badge';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import {
  CheckCircle,
  AlertTriangle,
  XCircle,
  Clock,
  Activity,
  Wifi,
  WifiOff,
  Server
} from 'lucide-react';
import {
  PeerSyncInfo,
  PeerSyncStatus,
  PeerHealthStatus
} from '@/api/federation-types';
import { formatRelativeTime, formatDurationMs } from '@/lib/formatters';

interface PeerSyncStatusCardProps {
  peers: PeerSyncInfo[];
  isLoading?: boolean;
  showTitle?: boolean;
  compact?: boolean;
}

export function PeerSyncStatusCard({
  peers,
  isLoading = false,
  showTitle = true,
  compact = false
}: PeerSyncStatusCardProps) {
  const getSyncStatusBadge = (status: PeerSyncStatus) => {
    switch (status) {
      case 'synced':
        return (
          <Badge variant="default" className="flex items-center gap-1">
            <CheckCircle className="h-3 w-3" />
            Synced
          </Badge>
        );
      case 'syncing':
        return (
          <Badge variant="secondary" className="flex items-center gap-1">
            <Clock className="h-3 w-3 animate-pulse" />
            Syncing
          </Badge>
        );
      case 'error':
        return (
          <Badge variant="destructive" className="flex items-center gap-1">
            <XCircle className="h-3 w-3" />
            Error
          </Badge>
        );
      case 'disconnected':
        return (
          <Badge variant="outline" className="flex items-center gap-1 text-muted-foreground">
            <WifiOff className="h-3 w-3" />
            Disconnected
          </Badge>
        );
    }
  };

  const getHealthStatusBadge = (status: PeerHealthStatus) => {
    switch (status) {
      case 'healthy':
        return (
          <Badge variant="success" className="flex items-center gap-1">
            <Activity className="h-3 w-3" />
            Healthy
          </Badge>
        );
      case 'degraded':
        return (
          <Badge variant="warning" className="flex items-center gap-1">
            <AlertTriangle className="h-3 w-3" />
            Degraded
          </Badge>
        );
      case 'unhealthy':
        return (
          <Badge variant="destructive" className="flex items-center gap-1">
            <XCircle className="h-3 w-3" />
            Unhealthy
          </Badge>
        );
      case 'isolated':
        return (
          <Badge variant="outline" className="flex items-center gap-1 border-orange-500 text-orange-700">
            <WifiOff className="h-3 w-3" />
            Isolated
          </Badge>
        );
    }
  };

  const formatTimestamp = (timestamp?: string) => {
    if (!timestamp) return '--';
    return formatRelativeTime(timestamp);
  };

  const formatLatency = (ms?: number) => {
    if (ms === undefined) return '--';
    return formatDurationMs(ms);
  };

  if (isLoading) {
    return (
      <Card>
        {showTitle && (
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Wifi className="h-5 w-5" />
              Peer Sync Status
            </CardTitle>
          </CardHeader>
        )}
        <CardContent>
          <div className="flex justify-center py-8">
            <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary"></div>
          </div>
        </CardContent>
      </Card>
    );
  }

  if (!peers || peers.length === 0) {
    return (
      <Card>
        {showTitle && (
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Wifi className="h-5 w-5" />
              Peer Sync Status
              <GlossaryTooltip termId="peer-sync-status">
                <span className="cursor-help text-muted-foreground">(?)</span>
              </GlossaryTooltip>
            </CardTitle>
          </CardHeader>
        )}
        <CardContent>
          <Alert>
            <Server className="h-4 w-4" />
            <AlertDescription>
              No peer nodes detected. This node is running in standalone mode.
            </AlertDescription>
          </Alert>
        </CardContent>
      </Card>
    );
  }

  const activePeers = peers.filter(p => p.active);
  const syncedPeers = peers.filter(p => p.sync_status === 'synced');
  const errorPeers = peers.filter(p => p.sync_status === 'error');
  const disconnectedPeers = peers.filter(p => p.sync_status === 'disconnected');

  return (
    <Card>
      {showTitle && (
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Wifi className="h-5 w-5" />
            Peer Sync Status
            <GlossaryTooltip termId="peer-sync-status">
              <span className="cursor-help text-muted-foreground">(?)</span>
            </GlossaryTooltip>
          </CardTitle>
        </CardHeader>
      )}
      <CardContent>
        <div className="space-y-4">
          {/* Summary Stats */}
          {!compact && (
            <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
              <div className="p-3 border rounded-lg">
                <div className="text-xs text-muted-foreground mb-1">Total Peers</div>
                <div className="text-2xl font-semibold">{peers.length}</div>
              </div>
              <div className="p-3 border rounded-lg">
                <div className="text-xs text-muted-foreground mb-1">Synced</div>
                <div className="text-2xl font-semibold text-green-600">{syncedPeers.length}</div>
              </div>
              <div className="p-3 border rounded-lg">
                <div className="text-xs text-muted-foreground mb-1">Errors</div>
                <div className="text-2xl font-semibold text-red-600">{errorPeers.length}</div>
              </div>
              <div className="p-3 border rounded-lg">
                <div className="text-xs text-muted-foreground mb-1">Disconnected</div>
                <div className="text-2xl font-semibold text-muted-foreground">
                  {disconnectedPeers.length}
                </div>
              </div>
            </div>
          )}

          {/* Peer Table */}
          <div className="overflow-x-auto">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Host ID</TableHead>
                  {!compact && <TableHead>Hostname</TableHead>}
                  <TableHead>Sync Status</TableHead>
                  <TableHead>Health</TableHead>
                  <TableHead>Last Sync</TableHead>
                  {!compact && <TableHead>Latency</TableHead>}
                  {!compact && <TableHead>Failed Heartbeats</TableHead>}
                </TableRow>
              </TableHeader>
              <TableBody>
                {peers.map((peer) => (
                  <TableRow key={peer.host_id}>
                    <TableCell className="font-mono text-xs">
                      {peer.host_id.substring(0, 12)}...
                    </TableCell>
                    {!compact && (
                      <TableCell>{peer.hostname || '--'}</TableCell>
                    )}
                    <TableCell>{getSyncStatusBadge(peer.sync_status)}</TableCell>
                    <TableCell>{getHealthStatusBadge(peer.health_status)}</TableCell>
                    <TableCell className="text-sm">
                      {formatTimestamp(peer.last_sync_at)}
                    </TableCell>
                    {!compact && (
                      <TableCell className="text-sm">
                        {formatLatency(peer.sync_lag_ms)}
                      </TableCell>
                    )}
                    {!compact && (
                      <TableCell>
                        {peer.failed_heartbeats > 0 ? (
                          <span className="text-destructive font-semibold">
                            {peer.failed_heartbeats}
                          </span>
                        ) : (
                          <span className="text-muted-foreground">0</span>
                        )}
                      </TableCell>
                    )}
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>

          {/* Error Messages */}
          {errorPeers.length > 0 && !compact && (
            <div className="space-y-2">
              <h4 className="text-sm font-semibold">Error Details</h4>
              {errorPeers.map((peer) => (
                peer.error_message && (
                  <Alert key={peer.host_id} variant="destructive">
                    <AlertTriangle className="h-4 w-4" />
                    <AlertDescription>
                      <strong>{peer.host_id.substring(0, 12)}...</strong>: {peer.error_message}
                    </AlertDescription>
                  </Alert>
                )
              ))}
            </div>
          )}
        </div>
      </CardContent>
    </Card>
  );
}
