/**
 * Federation service - handles federation management, quarantine, audit, and peer operations.
 *
 * Aligned with backend:
 * - crates/adapteros-server-api/src/handlers/federation.rs
 * - crates/adapteros-server-api/src/handlers.rs
 */

import type { ApiClient } from '@/api/client';
import * as federationTypes from '@/api/federation-types';

export class FederationService {
  constructor(private client: ApiClient) {}

  /**
   * Get overall federation status including node health and sync status
   * GET /v1/federation/status
   */
  async getFederationStatus(): Promise<federationTypes.FederationStatusResponse> {
    return this.client.request<federationTypes.FederationStatusResponse>('/v1/federation/status');
  }

  /**
   * Get current quarantine status for all nodes
   * GET /v1/federation/quarantine
   */
  async getQuarantineStatus(): Promise<federationTypes.QuarantineStatusResponse> {
    return this.client.request<federationTypes.QuarantineStatusResponse>('/v1/federation/quarantine');
  }

  /**
   * Release a node from quarantine
   * POST /v1/federation/release-quarantine
   *
   * @param request - Release quarantine request with optional reason
   * @returns Release quarantine response with success status
   */
  async releaseQuarantine(
    request: federationTypes.ReleaseQuarantineRequest
  ): Promise<federationTypes.ReleaseQuarantineResponse> {
    return this.client.request<federationTypes.ReleaseQuarantineResponse>(
      '/v1/federation/release-quarantine',
      {
        method: 'POST',
        body: JSON.stringify(request),
      },
      false,
      undefined,
      true // allowMutationRetry: true for safety
    );
  }

  /**
   * Get federation audit logs with optional filters
   * GET /v1/audit/federation
   *
   * @param filters - Optional filters for event type, node ID, status, time range, pagination
   * @returns Federation audit response with host chains and signature verification
   */
  async getFederationAudit(
    filters?: federationTypes.FederationAuditFilters
  ): Promise<federationTypes.FederationAuditResponse> {
    const params = new URLSearchParams();
    if (filters?.event_type) params.append('event_type', filters.event_type);
    if (filters?.node_id) params.append('node_id', filters.node_id);
    if (filters?.status) params.append('status', filters.status);
    if (filters?.start_time) params.append('start_time', filters.start_time);
    if (filters?.end_time) params.append('end_time', filters.end_time);
    if (filters?.limit) params.append('limit', filters.limit.toString());
    if (filters?.offset) params.append('offset', filters.offset.toString());
    const query = params.toString() ? `?${params.toString()}` : '';
    return this.client.request<federationTypes.FederationAuditResponse>(
      `/v1/audit/federation${query}`
    );
  }

  /**
   * Get list of federated peers with sync status
   * GET /v1/federation/peers
   *
   * @returns Peer list response with detailed peer information including health and attestation
   */
  async getFederationPeers(): Promise<federationTypes.PeerListResponse> {
    return this.client.request<federationTypes.PeerListResponse>('/v1/federation/peers');
  }
}
