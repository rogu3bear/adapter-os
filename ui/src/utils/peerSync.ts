import {
  PeerInfo,
  PeerSyncInfo,
  PeerSyncStatus,
  PeerHealthStatus,
  PeerSyncSummary
} from '@/api/federation-types';

/**
 * Converts PeerInfo from backend to PeerSyncInfo for UI display
 * Derives sync status from health status and heartbeat data
 */
export function derivePeerSyncInfo(peer: PeerInfo): PeerSyncInfo {
  // Determine sync status based on health and activity
  let syncStatus: PeerSyncStatus;

  if (!peer.active) {
    syncStatus = 'disconnected';
  } else if (peer.health_status === 'unhealthy' || peer.health_status === 'isolated') {
    syncStatus = 'error';
  } else if (peer.health_status === 'degraded' || peer.failed_heartbeats > 0) {
    syncStatus = 'syncing';
  } else {
    syncStatus = 'synced';
  }

  // Calculate sync lag (time since last heartbeat)
  let syncLagMs: number | undefined;
  if (peer.last_heartbeat_at) {
    const lastHeartbeat = new Date(peer.last_heartbeat_at);
    const now = new Date();
    syncLagMs = now.getTime() - lastHeartbeat.getTime();
  }

  return {
    host_id: peer.host_id,
    hostname: peer.hostname,
    sync_status: syncStatus,
    last_sync_at: peer.last_heartbeat_at || peer.last_seen_at,
    sync_lag_ms: syncLagMs,
    health_status: peer.health_status,
    failed_heartbeats: peer.failed_heartbeats,
    active: peer.active,
  };
}

/**
 * Converts an array of PeerInfo to PeerSyncInfo
 */
export function derivePeerSyncInfoList(peers: PeerInfo[]): PeerSyncInfo[] {
  return peers.map(derivePeerSyncInfo);
}

/**
 * Converts PeerSyncSummary from sync-status endpoint to PeerSyncInfo
 */
export function derivePeerSyncInfoFromSummary(summary: PeerSyncSummary): PeerSyncInfo {
  const sync_status: PeerSyncStatus = summary.in_sync ? 'synced' : 'syncing';
  const health_status: PeerHealthStatus = summary.in_sync ? 'healthy' : 'degraded';
  const lastSync = summary.last_sync_at;
  const sync_lag_ms = lastSync ? Date.now() - new Date(lastSync).getTime() : undefined;

  return {
    host_id: summary.peer_id,
    hostname: summary.host,
    sync_status,
    last_sync_at: summary.last_sync_at,
    sync_lag_ms,
    health_status,
    failed_heartbeats: summary.in_sync ? 0 : 1,
    active: true,
  };
}

export function derivePeerSyncInfoListFromSummaries(peers: PeerSyncSummary[]): PeerSyncInfo[] {
  return peers.map(derivePeerSyncInfoFromSummary);
}
