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
