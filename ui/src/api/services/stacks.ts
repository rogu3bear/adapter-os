/**
 * Stacks service - handles adapter stack management, lifecycle, and publishing.
 */

import type { ApiClient, ApiError } from '@/api/client';
import * as types from '@/api/types';
import * as policyTypes from '@/api/policyTypes';
import { extractArrayFromResponse } from '@/api/helpers';

export class StacksService {
  constructor(private client: ApiClient) {}

  async listAdapterStacks(): Promise<types.AdapterStack[]> {
    // Backend returns StackResponse[] with adapter_ids, map to AdapterStack
    // DEFENSIVE: Use extractArrayFromResponse to handle potential PaginatedResponse migration
    type BackendStack = {
      id: string;
      name: string;
      adapter_ids: string[];
      created_at: string;
      updated_at: string;
      lifecycle_state: string;
      tenant_id: string;
      version: number;
      is_active: boolean;
      description?: string;
      determinism_mode?: string;
      is_default?: boolean;
      routing_determinism_mode?: string;
      warnings?: string[];
      workflow_type?: string;
    };
    const rawResponse = await this.client.request<unknown>('/v1/adapter-stacks');
    const backendStacks = extractArrayFromResponse<BackendStack>(rawResponse);

    return backendStacks.map(stack => ({
      id: stack.id,
      name: stack.name,
      adapter_ids: stack.adapter_ids,
      created_at: stack.created_at,
      updated_at: stack.updated_at,
      lifecycle_state: stack.lifecycle_state,
      tenant_id: stack.tenant_id,
      version: stack.version,
      is_active: stack.is_active,
      description: stack.description,
      determinism_mode: stack.determinism_mode,
      is_default: stack.is_default,
      routing_determinism_mode: stack.routing_determinism_mode,
      warnings: stack.warnings,
      workflow_type: stack.workflow_type as 'Parallel' | 'UpstreamDownstream' | 'Sequential' | undefined,
    }));
  }

  async createAdapterStack(stack: types.CreateAdapterStackRequest): Promise<types.AdapterStackResponse> {
    const response = await this.client.request<types.AdapterStackResponse>('/v1/adapter-stacks', {
      method: 'POST',
      body: JSON.stringify(stack),
    });
    return response;
  }

  async getAdapterStack(id: string): Promise<types.AdapterStack> {
    const response = await this.client.request<types.AdapterStackResponse>(`/v1/adapter-stacks/${id}`);
    return response.stack;
  }

  async deleteAdapterStack(id: string): Promise<void> {
    return this.client.request<void>(`/v1/adapter-stacks/${id}`, {
      method: 'DELETE',
    });
  }

  async getAdapterStackHistory(id: string): Promise<types.LifecycleHistoryEvent[]> {
    return this.client.requestList<types.LifecycleHistoryEvent>(`/v1/adapter-stacks/${id}/history`);
  }

  async updateAdapterStack(id: string, data: types.UpdateAdapterStackRequest): Promise<types.AdapterStack> {
    const response = await this.client.request<types.AdapterStackResponse>(`/v1/adapter-stacks/${id}`, {
      method: 'PUT',
      body: JSON.stringify(data),
    });
    return response.stack;
  }

  /**
   * Run preflight policy checks before activating an adapter stack
   * 【2025-11-25†ui†stack-preflight-checks】
   */
  async preflightStackActivation(stackId: string): Promise<types.PolicyPreflightResponse> {
    return this.client.request<types.PolicyPreflightResponse>(
      `/v1/adapter-stacks/${encodeURIComponent(stackId)}/activate/preflight`,
      { method: 'POST' }
    );
  }

  async activateAdapterStack(id: string): Promise<types.AdapterStack> {
    const response = await this.client.request<types.AdapterStackResponse>(`/v1/adapter-stacks/${id}/activate`, {
      method: 'POST',
    });
    return response.stack;
  }

  async deactivateAdapterStack(): Promise<void> {
    return this.client.request<void>('/v1/adapter-stacks/deactivate', {
      method: 'POST',
    });
  }

  async clearStackAdapters(stackId: string): Promise<types.ClearStackAdaptersResponse> {
    return this.client.request<types.ClearStackAdaptersResponse>(
      `/v1/adapter-stacks/${encodeURIComponent(stackId)}/clear-adapters`,
      { method: 'POST' }
    );
  }

  /**
   * Get policies assigned to a stack with compliance summary
   * Stack-Policy API
   */
  async getStackPolicies(stackId: string): Promise<policyTypes.StackPoliciesResponse> {
    return this.client.request<policyTypes.StackPoliciesResponse>(
      `/v1/adapter-stacks/${encodeURIComponent(stackId)}/policies`
    );
  }

  async getDefaultAdapterStack(tenantId: string = 'default'): Promise<types.AdapterStack | null> {
    try {
      const response = await this.client.request<types.DefaultStackResponse>(`/v1/tenants/${tenantId}/default-stack`);
      if (response.stack_id) {
        return await this.getAdapterStack(response.stack_id);
      }
      return null;
    } catch (error: unknown) {
      if (error instanceof Error && 'status' in error && (error as ApiError).status === 404) {
        return null;
      }
      throw error;
    }
  }

  async setDefaultAdapterStack(stackId: string, tenantId: string = 'default'): Promise<void> {
    return this.client.request<void>(`/v1/tenants/${tenantId}/default-stack`, {
      method: 'PUT',
      body: JSON.stringify({ stack_id: stackId }),
    });
  }

  async clearDefaultAdapterStack(tenantId: string = 'default'): Promise<void> {
    return this.client.request<void>(`/v1/tenants/${tenantId}/default-stack`, {
      method: 'DELETE',
    });
  }

  // ============================================================================
  // Adapter Publish + Attach Modes v1
  // ============================================================================

  /**
   * Publish an adapter version with attach mode configuration.
   * Makes the adapter available for use in inference stacks.
   */
  async publishAdapterVersion(
    repoId: string,
    versionId: string,
    request: types.PublishAdapterRequest
  ): Promise<types.PublishAdapterResponse> {
    return this.client.request<types.PublishAdapterResponse>(
      `/v1/training/repos/${encodeURIComponent(repoId)}/versions/${encodeURIComponent(versionId)}/publish`,
      {
        method: 'POST',
        body: JSON.stringify(request),
      }
    );
  }

  /**
   * Archive an adapter version.
   * Archived versions are hidden from normal use but retained for audit.
   */
  async archiveAdapterVersion(
    versionId: string,
    reason?: string
  ): Promise<types.ArchiveAdapterResponse> {
    return this.client.request<types.ArchiveAdapterResponse>(
      `/v1/adapter-versions/${encodeURIComponent(versionId)}/archive`,
      {
        method: 'POST',
        body: JSON.stringify({ reason }),
      }
    );
  }

  /**
   * Unarchive an adapter version.
   * Restores visibility of an archived version.
   */
  async unarchiveAdapterVersion(versionId: string): Promise<types.ArchiveAdapterResponse> {
    return this.client.request<types.ArchiveAdapterResponse>(
      `/v1/adapter-versions/${encodeURIComponent(versionId)}/unarchive`,
      {
        method: 'POST',
      }
    );
  }

  async validateStackName(name: string): Promise<types.ValidateStackNameResponse> {
    return this.client.request<types.ValidateStackNameResponse>('/v1/stacks/validate-name', {
      method: 'POST',
      body: JSON.stringify({ name }),
    });
  }
}
