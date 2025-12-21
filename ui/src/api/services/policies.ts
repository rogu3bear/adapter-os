/**
 * Policies service - handles policy management, validation, audit, and promotion operations.
 */

import type { ApiClient } from '@/api/client';
import * as adapterTypes from '@/api/adapter-types';
import * as apiTypes from '@/api/api-types';
import { logger } from '@/utils/logger';
import { extractArrayFromResponse } from '@/api/helpers';

/**
 * Helper function to parse audit log metadata JSON
 */
function parseAuditMetadata(metadata?: string | null): Record<string, unknown> | undefined {
  if (!metadata) {
    return undefined;
  }

  try {
    return JSON.parse(metadata);
  } catch (error) {
    logger.debug('Failed to parse audit log metadata', {
      component: 'PoliciesService',
      operation: 'parseAuditMetadata',
      metadata,
      error: error instanceof Error ? error.message : String(error),
    });
    return undefined;
  }
}

export class PoliciesService {
  constructor(private client: ApiClient) {}

  // ===== Policy Management =====
  async listPolicies(): Promise<adapterTypes.Policy[]> {
    return this.client.requestList<adapterTypes.Policy>('/v1/policies');
  }

  async getPolicy(cpid: string): Promise<adapterTypes.Policy> {
    return this.client.request<adapterTypes.Policy>(`/v1/policies/${cpid}`);
  }

  async validatePolicy(data: adapterTypes.ValidatePolicyRequest): Promise<{ valid: boolean; errors?: string[] }> {
    return this.client.request('/v1/policies/validate', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async applyPolicy(data: adapterTypes.ApplyPolicyRequest): Promise<apiTypes.PolicyPackResponse> {
    return this.client.request<apiTypes.PolicyPackResponse>('/v1/policies/apply', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async createPolicy(cpid: string, content: string): Promise<apiTypes.PolicyPackResponse> {
    return this.client.request<apiTypes.PolicyPackResponse>('/v1/policies/apply', {
      method: 'POST',
      body: JSON.stringify({ cpid, content }),
    });
  }

  async updatePolicy(cpid: string, content: string): Promise<apiTypes.PolicyPackResponse> {
    return this.client.request<apiTypes.PolicyPackResponse>('/v1/policies/apply', {
      method: 'POST',
      body: JSON.stringify({ cpid, content }),
    });
  }

  // ===== Policy Operations =====
  async signPolicy(cpid: string): Promise<apiTypes.SignPolicyResponse> {
    return this.client.request<apiTypes.SignPolicyResponse>(`/v1/policies/${cpid}/sign`, {
      method: 'POST',
    });
  }

  async comparePolicies(cpid1: string, cpid2: string): Promise<apiTypes.PolicyComparisonResponse> {
    return this.client.request<apiTypes.PolicyComparisonResponse>('/v1/policies/compare', {
      method: 'POST',
      body: JSON.stringify({ cpid_1: cpid1, cpid_2: cpid2 }),
    });
  }

  async exportPolicy(cpid: string): Promise<apiTypes.ExportPolicyResponse> {
    return this.client.request<apiTypes.ExportPolicyResponse>(`/v1/policies/${cpid}/export`);
  }

  // ===== Promotion Execution =====
  async dryRunPromotion(cpid: string): Promise<apiTypes.DryRunPromotionResponse> {
    return this.client.request<apiTypes.DryRunPromotionResponse>('/v1/cp/promote/dry-run', {
      method: 'POST',
      body: JSON.stringify({ cpid }),
    });
  }

  async getPromotionHistory(): Promise<apiTypes.PromotionHistoryEntry[]> {
    return this.client.requestList<apiTypes.PromotionHistoryEntry>('/v1/cp/promotions');
  }

  // ===== Audit Operations =====
  async queryAuditLogs(filters?: apiTypes.AuditLogFilters): Promise<apiTypes.AuditLog[]> {
    const params = new URLSearchParams();
    if (filters?.action) params.append('action', filters.action);
    if (filters?.user_id) params.append('user_id', filters.user_id);
    if (filters?.resource) params.append('resource', filters.resource);
    if (filters?.status) params.append('status', filters.status);
    if (filters?.start_time) params.append('start_time', filters.start_time);
    if (filters?.end_time) params.append('end_time', filters.end_time);
    if (filters?.limit) params.append('limit', filters.limit.toString());
    if (filters?.offset) params.append('offset', filters.offset.toString());
    if (filters?.tenant_id) params.append('tenant_id', filters.tenant_id);
    const query = params.toString() ? `?${params.toString()}` : '';

    // DEFENSIVE: Use extractArrayFromResponse to handle potential PaginatedResponse migration
    const rawResponse = await this.client.request<unknown>(`/v1/audit/logs${query}`);
    const logs = extractArrayFromResponse<apiTypes.AuditLogEntry>(rawResponse);
    return logs.map((log) => ({
      id: log.id,
      user_id: log.user_id,
      action: log.action,
      resource: log.resource_type,
      resource_id: log.resource_id,
      status: log.status,
      timestamp: log.timestamp,
      ip_address: log.ip_address,
      user_agent: undefined,
      details: parseAuditMetadata(log.metadata_json),
      tenant_id: log.tenant_id,
      session_id: undefined,
      user_role: log.user_role,
      error_message: log.error_message,
      metadata_json: log.metadata_json,
    }));
  }

  async getPolicyAuditDecisions(params?: {
    tenantId?: string;
    limit?: number;
    offset?: number;
  }): Promise<apiTypes.PolicyAuditDecision[]> {
    const search = new URLSearchParams();
    if (params?.tenantId) search.append('tenant_id', params.tenantId);
    if (params?.limit) search.append('limit', params.limit.toString());
    if (params?.offset) search.append('offset', params.offset.toString());
    const query = search.toString();
    return this.client.requestList<apiTypes.PolicyAuditDecision>(
      `/v1/audit/policy-decisions${query ? `?${query}` : ''}`,
    );
  }

  async verifyPolicyAuditChain(tenantId?: string): Promise<apiTypes.PolicyAuditChainVerification> {
    const query = tenantId ? `?tenant_id=${encodeURIComponent(tenantId)}` : '';
    return this.client.request<apiTypes.PolicyAuditChainVerification>(
      `/v1/audit/policy-decisions/verify-chain${query}`,
    );
  }

  async triggerAuditDivergence(tenantId?: string): Promise<apiTypes.DivergeAuditChainResponse> {
    return this.client.request<apiTypes.DivergeAuditChainResponse>('/v1/testkit/audit/diverge', {
      method: 'POST',
      body: JSON.stringify(tenantId ? { tenant_id: tenantId } : {}),
    });
  }
}
