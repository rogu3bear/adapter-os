import {
  PeerInfo,
  PeerSyncInfo,
  PeerSyncStatus,
  PeerHealthStatus
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
