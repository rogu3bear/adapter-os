// Federation management types
// Aligned with backend: crates/adapteros-server-api/src/handlers/federation.rs
// and crates/adapteros-server-api/src/handlers.rs

/**
 * Federation status response
 * Source: FederationStatusResponse in handlers/federation.rs
 */
export interface FederationStatusResponse {
  /** Whether federation is operational */
  operational: boolean;
  /** Whether system is quarantined */
  quarantined: boolean;
  /** Quarantine reason (if quarantined) */
  quarantine_reason?: string;
  /** Latest verification report (JSON string) */
  latest_verification?: string;
  /** Number of registered hosts */
  total_hosts: number;
  /** Timestamp */
  timestamp: string;
}

/**
 * Quarantine details
 * Source: QuarantineDetails in handlers/federation.rs
 */
export interface QuarantineDetails {
  /** Reason for quarantine */
  reason: string;
  /** When quarantine was triggered */
  triggered_at: string;
  /** Violation type */
  violation_type: string;
  /** Control plane ID */
  cpid?: string;
}

/**
 * Quarantine status response
 * Source: QuarantineStatusResponse in handlers/federation.rs
 */
export interface QuarantineStatusResponse {
  /** Whether system is quarantined */
  quarantined: boolean;
  /** Quarantine details */
  details?: QuarantineDetails;
}

/**
 * Release quarantine request
 * Used with POST /v1/federation/release-quarantine
 */
export interface ReleaseQuarantineRequest {
  /** Optional reason for release */
  reason?: string;
}

/**
 * Release quarantine response
 * Response from POST /v1/federation/release-quarantine
 */
export interface ReleaseQuarantineResponse {
  success: boolean;
  message: string;
  timestamp: string;
}

/**
 * Host chain summary for federation audit
 * Source: HostChainSummary in handlers.rs
 */
export interface HostChainSummary {
  host_id: string;
  bundle_count: number;
  latest_bundle?: string;
}

/**
 * Federation audit response
 * Source: FederationAuditResponse in handlers.rs
 */
export interface FederationAuditResponse {
  total_hosts: number;
  total_signatures: number;
  verified_signatures: number;
  quarantined: boolean;
  quarantine_reason?: string;
  host_chains: HostChainSummary[];
  timestamp: string;
}

/**
 * Federation audit query filters
 * Used for filtering audit log queries
 */
export interface FederationAuditFilters {
  event_type?: string;
  node_id?: string;
  host_id?: string;
  status?: 'success' | 'failure';
  start_time?: string;
  end_time?: string;
  limit?: number;
  offset?: number;
}

/**
 * Peer health status
 * Source: PeerHealthStatus in adapteros-federation/src/peer.rs
 */
export type PeerHealthStatus = 'healthy' | 'degraded' | 'unhealthy' | 'isolated';

/**
 * Peer discovery status
 * Source: DiscoveryStatus in adapteros-federation/src/peer.rs
 */
export type PeerDiscoveryStatus = 'registered' | 'discovering' | 'failed';

/**
 * Peer sync status (derived from health and heartbeat data)
 */
export type PeerSyncStatus = 'synced' | 'syncing' | 'error' | 'disconnected';

/**
 * Hardware attestation metadata
 * Source: AttestationMetadata in adapteros-federation/src/peer.rs
 */
export interface AttestationMetadata {
  platform: string;
  secure_enclave_available: boolean;
  tpm_available: boolean;
  attestation_timestamp: number;
  hardware_id?: string;
}

/**
 * Peer information for a federated host
 * Source: PeerInfo in adapteros-federation/src/peer.rs
 */
export interface PeerInfo {
  host_id: string;
  pubkey: string;
  hostname?: string;
  registered_at: string;
  last_seen_at?: string;
  last_heartbeat_at?: string;
  attestation_metadata?: AttestationMetadata;
  active: boolean;
  health_status: PeerHealthStatus;
  discovery_status: PeerDiscoveryStatus;
  failed_heartbeats: number;
}

/**
 * Peer health check record
 * Source: PeerHealthCheck in adapteros-federation/src/peer.rs
 */
export interface PeerHealthCheck {
  host_id: string;
  timestamp: string;
  status: PeerHealthStatus;
  response_time_ms: number;
  error_message?: string;
}

/**
 * Peer sync information (UI-derived from PeerInfo)
 */
export interface PeerSyncInfo {
  host_id: string;
  hostname?: string;
  sync_status: PeerSyncStatus;
  last_sync_at?: string;
  sync_lag_ms?: number;
  health_status: PeerHealthStatus;
  response_time_ms?: number;
  error_message?: string;
  failed_heartbeats: number;
  active: boolean;
}

/**
 * Peer list response
 * GET /v1/federation/peers
 */
export interface PeerListResponse {
  peers: PeerInfo[];
  total_count: number;
  timestamp: string;
}
