import type { ApiClient } from '@/api/client';
import type { Adapter, AdapterStack } from '@/api/types';
import * as types from '@/api/types';
import * as adapterTypes from '@/api/adapter-types';
import * as policyTypes from '@/api/policyTypes';
import { isCoremlPackageUiEnabled } from '@/config/featureFlags';
import { enhanceError } from '@/utils/errorMessages';

/**
 * AdaptersService
 *
 * Handles all adapter-related API operations including:
 * - Adapter listing and filtering
 * - Adapter lifecycle management (load, unload, pin, unpin)
 * - CoreML package operations (export, verification)
 * - Adapter details and metadata (health, stats, usage, lineage)
 * - Adapter registration and import
 * - Adapter state and policy management
 * - Category policies
 * - Domain adapters
 * - Memory and eviction management
 * - Training session management
 * - Behavior event tracking
 *
 * Citation: Extracted from client.ts lines 896-1044, 1277-1370, 1829-1918, 2523-2537, 2842-2927, 6263-6330
 */
export class AdaptersService {
  constructor(private client: ApiClient) {}

  // ============================================================================
  // Core Adapter Operations
  // ============================================================================

  /**
   * List adapters with optional filtering
   *
   * GET /v1/adapters
   *
   * @param params - Optional filters (tier, framework)
   * @returns Array of adapters
   */
  async listAdapters(params?: { tier?: string; framework?: string }): Promise<Adapter[]> {
    const qs = new URLSearchParams();
    if (params?.tier !== undefined) qs.append('tier', params.tier);
    if (params?.framework) qs.append('framework', params.framework);
    const query = qs.toString() ? `?${qs.toString()}` : '';
    return this.client.requestList<Adapter>(`/v1/adapters${query}`);
  }

  /**
   * Get adapter by ID
   *
   * GET /v1/adapters/:adapterId
   *
   * @param adapterId - The adapter ID
   * @returns Adapter information
   */
  async getAdapter(adapterId: string): Promise<Adapter> {
    return this.client.request<Adapter>(`/v1/adapters/${adapterId}`);
  }

  /**
   * Get detailed adapter information
   *
   * GET /v1/adapters/:adapterId/detail
   *
   * @param adapterId - The adapter ID
   * @returns Detailed adapter information
   */
  async getAdapterDetail(adapterId: string): Promise<types.AdapterDetailResponse> {
    return this.client.request<types.AdapterDetailResponse>(`/v1/adapters/${adapterId}/detail`);
  }

  /**
   * Activate an adapter for a workspace
   *
   * POST /v1/adapters/:adapterId/activate
   *
   * @param adapterId - The adapter ID
   * @param workspace_id - Optional workspace scope
   */
  async activateAdapter(
    adapterId: string,
    payload?: { workspace_id?: string }
  ): Promise<unknown> {
    return this.client.request<unknown>(`/v1/adapters/${encodeURIComponent(adapterId)}/activate`, {
      method: 'POST',
      body: JSON.stringify({ workspace_id: payload?.workspace_id }),
    });
  }

  /**
   * Update adapter strength
   *
   * PATCH /v1/adapters/:adapterId/strength
   *
   * @param adapterId - The adapter ID
   * @param loraStrength - New LoRA strength value
   * @returns Updated adapter details
   */
  async updateAdapterStrength(adapterId: string, loraStrength: number): Promise<types.AdapterDetailResponse> {
    return this.client.request<types.AdapterDetailResponse>(`/v1/adapters/${adapterId}/strength`, {
      method: 'PATCH',
      body: JSON.stringify({ lora_strength: loraStrength }),
    });
  }

  /**
   * Register a new adapter
   *
   * POST /v1/adapters/register
   *
   * @param data - Adapter registration data
   * @returns Registered adapter
   */
  async registerAdapter(data: types.RegisterAdapterRequest): Promise<Adapter> {
    return this.client.request<Adapter>('/v1/adapters/register', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  /**
   * Import an adapter from file
   *
   * POST /v1/adapters/import
   *
   * @param file - Adapter file to import
   * @param load - Whether to load the adapter after import
   * @param options - Additional request options
   * @param skipRetry - Whether to skip retry logic
   * @param cancelToken - Abort signal for cancellation
   * @returns Imported adapter
   */
  async importAdapter(
    file: File,
    load?: boolean,
    options: RequestInit = {},
    skipRetry: boolean = false,
    cancelToken?: AbortSignal
  ): Promise<Adapter> {
    const formData = new FormData();
    formData.append('file', file);

    const params = new URLSearchParams();
    if (load) params.append('load', 'true');

    const query = params.toString() ? `?${params.toString()}` : '';
    return this.client.request<Adapter>(
      `/v1/adapters/import${query}`,
      {
        method: 'POST',
        body: formData,
        headers: {}, // Let browser set Content-Type for FormData
        ...options,
      },
      skipRetry,
      cancelToken
    );
  }

  /**
   * Delete an adapter
   *
   * DELETE /v1/adapters/:adapterId
   *
   * @param adapterId - The adapter ID
   */
  async deleteAdapter(adapterId: string): Promise<void> {
    return this.client.request<void>(`/v1/adapters/${adapterId}`, {
      method: 'DELETE',
    });
  }

  /**
   * Upsert adapter directory
   *
   * POST /v1/adapters/directory/upsert
   *
   * @param data - Directory upsert data
   * @returns Adapter ID
   */
  async upsertAdapterDirectory(data: {
    tenant_id: string;
    root: string;
    path: string;
    activate: boolean;
  }): Promise<{ adapter_id: string }> {
    return this.client.request<{ adapter_id: string }>('/v1/adapters/directory/upsert', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  /**
   * Validate adapter name
   *
   * POST /v1/adapters/validate-name
   *
   * @param request - Name validation request
   * @returns Validation response
   */
  async validateAdapterName(request: { name: string }): Promise<types.ValidateAdapterNameResponse> {
    return this.client.request<types.ValidateAdapterNameResponse>('/v1/adapters/validate-name', {
      method: 'POST',
      body: JSON.stringify(request),
    });
  }

  // ============================================================================
  // Adapter Lifecycle Management
  // ============================================================================

  /**
   * Load an adapter
   *
   * POST /v1/adapters/:adapterId/load
   *
   * @param adapterId - The adapter ID
   * @returns Loaded adapter information
   */
  async loadAdapter(adapterId: string): Promise<Adapter> {
    return this.client.request<Adapter>(`/v1/adapters/${adapterId}/load`, {
      method: 'POST',
    });
  }

  /**
   * Unload an adapter
   *
   * POST /v1/adapters/:adapterId/unload
   *
   * @param adapterId - The adapter ID
   */
  async unloadAdapter(adapterId: string): Promise<void> {
    return this.client.request<void>(`/v1/adapters/${adapterId}/unload`, {
      method: 'POST',
    });
  }

  /**
   * Preflight check for adapter load/unload operation
   *
   * POST /v1/adapters/:adapterId/load/preflight
   *
   * @param adapterId - The adapter ID
   * @param operation - The operation to preflight ('load' or 'unload')
   * @returns Preflight response with policy checks
   */
  async preflightAdapterLoad(
    adapterId: string,
    operation: 'load' | 'unload' = 'load'
  ): Promise<policyTypes.PolicyPreflightResponse> {
    return this.client.request<policyTypes.PolicyPreflightResponse>(
      `/v1/adapters/${encodeURIComponent(adapterId)}/load/preflight`,
      {
        method: 'POST',
        body: JSON.stringify({ operation, includeDetails: true }),
      }
    );
  }

  /**
   * Pin adapter to memory
   *
   * Supports both boolean and advanced pinning modes:
   * - Boolean true: Simple pin without TTL
   * - Boolean false: Unpin adapter
   * - Number: Pin with TTL in hours
   *
   * POST /v1/adapters/:adapterId/pin
   *
   * @param adapterId - The adapter ID
   * @param pinnedOrTtlHours - Pin status (boolean) or TTL hours (number)
   * @param reason - Optional reason for pinning
   */
  async pinAdapter(adapterId: string, pinnedOrTtlHours: boolean | number, reason?: string): Promise<void> {
    // If boolean, use simple pin/unpin API
    if (typeof pinnedOrTtlHours === 'boolean') {
      if (pinnedOrTtlHours) {
        return this.client.request<void>(`/v1/adapters/${adapterId}/pin`, {
          method: 'POST',
          body: JSON.stringify({}),
        });
      } else {
        return this.unpinAdapter(adapterId);
      }
    }
    // Otherwise use advanced API with TTL
    return this.client.request<void>(`/v1/adapters/${adapterId}/pin`, {
      method: 'POST',
      body: JSON.stringify({ ttl_hours: pinnedOrTtlHours, reason }),
    });
  }

  /**
   * Unpin adapter from memory
   *
   * DELETE /v1/adapters/:adapterId/pin
   *
   * @param adapterId - The adapter ID
   */
  async unpinAdapter(adapterId: string): Promise<void> {
    return this.client.request<void>(`/v1/adapters/${adapterId}/pin`, {
      method: 'DELETE',
    });
  }

  /**
   * Swap adapters in memory
   *
   * POST /v1/adapters/swap
   *
   * @param add - Adapter IDs to add
   * @param remove - Adapter IDs to remove
   * @param commit - Whether to commit the swap
   */
  async swapAdapters(add: string[], remove: string[], commit: boolean = false): Promise<void> {
    return this.client.request<void>('/v1/adapters/swap', {
      method: 'POST',
      body: JSON.stringify({ add, remove, commit }),
    });
  }

  /**
   * Promote adapter lifecycle state
   *
   * POST /v1/adapters/:adapterId/lifecycle/promote
   *
   * @param adapterId - The adapter ID
   * @param reason - Reason for promotion
   * @returns Lifecycle transition response
   */
  async promoteAdapterLifecycle(adapterId: string, reason: string): Promise<types.LifecycleTransitionResponse> {
    return this.client.request<types.LifecycleTransitionResponse>(`/v1/adapters/${adapterId}/lifecycle/promote`, {
      method: 'POST',
      body: JSON.stringify({ reason }),
    });
  }

  /**
   * Demote adapter lifecycle state
   *
   * POST /v1/adapters/:adapterId/lifecycle/demote
   *
   * @param adapterId - The adapter ID
   * @param reason - Reason for demotion
   * @returns Lifecycle transition response
   */
  async demoteAdapterLifecycle(adapterId: string, reason: string): Promise<types.LifecycleTransitionResponse> {
    return this.client.request<types.LifecycleTransitionResponse>(`/v1/adapters/${adapterId}/lifecycle/demote`, {
      method: 'POST',
      body: JSON.stringify({ reason }),
    });
  }

  /**
   * Promote adapter state
   *
   * POST /v1/adapters/:adapterId/state/promote
   *
   * @param adapterId - The adapter ID
   * @param options - Additional request options
   * @param skipRetry - Whether to skip retry logic
   * @param cancelToken - Abort signal for cancellation
   * @returns Adapter state response
   */
  async promoteAdapterState(
    adapterId: string,
    options: RequestInit = {},
    skipRetry: boolean = false,
    cancelToken?: AbortSignal
  ): Promise<types.AdapterStateResponse> {
    return this.client.request<types.AdapterStateResponse>(
      `/v1/adapters/${adapterId}/state/promote`,
      {
        method: 'POST',
        ...options,
      },
      skipRetry,
      cancelToken
    );
  }

  // ============================================================================
  // CoreML Package Operations
  // ============================================================================

  /**
   * Get CoreML package status for an adapter
   *
   * GET /v1/adapters/:adapterId/coreml/status
   *
   * @param adapterId - The adapter ID
   * @param modelId - Optional model ID filter
   * @returns CoreML package status
   */
  async getCoremlPackageStatus(
    adapterId: string,
    modelId?: string
  ): Promise<adapterTypes.CoremlPackageStatus> {
    if (!isCoremlPackageUiEnabled()) {
      return { supported: false, export_available: false, verification_status: 'unsupported' };
    }
    if (!adapterId) {
      return { supported: false, export_available: false, verification_status: 'unknown' };
    }
    const qs = new URLSearchParams();
    if (modelId) {
      qs.append('model_id', modelId);
    }
    const query = qs.toString() ? `?${qs.toString()}` : '';
    try {
      const resp = await this.client.request<adapterTypes.CoremlPackageStatusResponse>(
        `/v1/adapters/${encodeURIComponent(adapterId)}/coreml/status${query}`
      );
      return resp.status ?? (resp as unknown as adapterTypes.CoremlPackageStatus);
    } catch (error) {
      const apiErr = error as { status?: number; detail?: string };
      if (apiErr?.status === 404 || apiErr?.status === 501) {
        return {
          supported: false,
          export_available: false,
          verification_status: 'unsupported',
          notes: apiErr?.detail ? [apiErr.detail] : undefined,
        };
      }
      throw enhanceError(apiErr as Error, {
        operation: 'coreml_status',
        adapterId,
        modelId,
      });
    }
  }

  /**
   * Trigger CoreML export for an adapter
   *
   * POST /v1/adapters/:adapterId/coreml/export
   *
   * @param adapterId - The adapter ID
   * @param modelId - Optional model ID
   * @returns CoreML package action response
   */
  async triggerCoremlExport(
    adapterId: string,
    modelId?: string
  ): Promise<adapterTypes.CoremlPackageActionResponse> {
    if (!isCoremlPackageUiEnabled()) {
      return {
        message: 'CoreML export is disabled in this build',
        status: { supported: false, export_available: false, verification_status: 'unsupported' },
      };
    }
    const qs = new URLSearchParams();
    if (modelId) {
      qs.append('model_id', modelId);
    }
    const query = qs.toString() ? `?${qs.toString()}` : '';
    try {
      return await this.client.request<adapterTypes.CoremlPackageActionResponse>(
        `/v1/adapters/${encodeURIComponent(adapterId)}/coreml/export${query}`,
        { method: 'POST' },
        false,
        undefined,
        true
      );
    } catch (error) {
      const apiErr = error as { status?: number; detail?: string; message?: string };
      const detail =
        apiErr?.detail ||
        apiErr?.message ||
        (apiErr?.status === 404 || apiErr?.status === 501
          ? 'CoreML export not supported by server'
          : 'CoreML export request failed');
      const err = enhanceError(apiErr as Error, {
        operation: 'coreml_export',
        adapterId,
        modelId,
        detail,
      });
      throw err;
    }
  }

  /**
   * Trigger CoreML verification for an adapter
   *
   * POST /v1/adapters/:adapterId/coreml/verify
   *
   * @param adapterId - The adapter ID
   * @returns CoreML package action response
   */
  async triggerCoremlVerification(
    adapterId: string
  ): Promise<adapterTypes.CoremlPackageActionResponse> {
    if (!isCoremlPackageUiEnabled()) {
      return {
        message: 'CoreML verification is disabled in this build',
        status: { supported: false, export_available: false, verification_status: 'unsupported' },
      };
    }
    try {
      return await this.client.request<adapterTypes.CoremlPackageActionResponse>(
        `/v1/adapters/${encodeURIComponent(adapterId)}/coreml/verify`,
        { method: 'POST' },
        false,
        undefined,
        true
      );
    } catch (error) {
      const apiErr = error as { status?: number; detail?: string; message?: string };
      const detail =
        apiErr?.detail ||
        apiErr?.message ||
        (apiErr?.status === 404 || apiErr?.status === 501
          ? 'CoreML verification not supported by server'
          : 'CoreML verification request failed');
      const err = enhanceError(apiErr as Error, {
        operation: 'coreml_verification',
        adapterId,
        detail,
      });
      throw err;
    }
  }

  // ============================================================================
  // Adapter Metadata and Statistics
  // ============================================================================

  /**
   * Get adapter statistics
   *
   * GET /v1/adapters/:adapterId/stats
   *
   * @param adapterId - The adapter ID
   * @returns Adapter statistics
   */
  async getAdapterStats(adapterId: string): Promise<types.AdapterStats> {
    return this.client.request<types.AdapterStats>(`/v1/adapters/${adapterId}/stats`);
  }

  /**
   * Get adapter usage information
   *
   * GET /v1/adapters/:adapterId/usage
   *
   * @param adapterId - The adapter ID
   * @returns Adapter usage information
   */
  async getAdapterUsage(adapterId: string): Promise<types.AdapterUsageResponse> {
    return this.client.request<types.AdapterUsageResponse>(`/v1/adapters/${adapterId}/usage`);
  }

  /**
   * Get adapter activations
   *
   * GET /v1/adapters/:adapterId/activations
   *
   * @param adapterId - The adapter ID
   * @returns Array of adapter activations
   */
  async getAdapterActivations(adapterId: string): Promise<types.AdapterActivation[]> {
    return this.client.requestList<types.AdapterActivation>(`/v1/adapters/${adapterId}/activations`);
  }

  /**
   * Get adapter health status
   *
   * GET /v1/adapters/:adapterId/health
   *
   * @param adapterId - The adapter ID
   * @returns Adapter health response
   */
  async getAdapterHealth(adapterId: string): Promise<types.AdapterHealthResponse> {
    return this.client.request<types.AdapterHealthResponse>(`/v1/adapters/${adapterId}/health`);
  }

  /**
   * Download adapter manifest
   *
   * GET /v1/adapters/:adapterId/manifest
   *
   * @param adapterId - The adapter ID
   * @returns Adapter manifest
   */
  async downloadAdapterManifest(adapterId: string): Promise<types.AdapterManifest> {
    return this.client.request<types.AdapterManifest>(`/v1/adapters/${adapterId}/manifest`);
  }

  /**
   * Get adapter lineage
   *
   * GET /v1/adapters/:adapterId/lineage
   *
   * @param adapterId - The adapter ID
   * @returns Adapter lineage response
   */
  async getAdapterLineage(adapterId: string): Promise<types.AdapterLineageResponse> {
    return this.client.request<types.AdapterLineageResponse>(`/v1/adapters/${adapterId}/lineage`);
  }

  /**
   * Get adapter version lineage
   *
   * GET /v1/lineage/adapter_versions/:adapterVersionId
   *
   * @param adapterVersionId - The adapter version ID
   * @param params - Optional lineage query parameters
   * @returns Lineage graph response
   */
  async getAdapterVersionLineage(
    adapterVersionId: string,
    params?: types.LineageQueryParams
  ): Promise<types.LineageGraphResponse> {
    const queryParams = new URLSearchParams();
    if (params?.direction) queryParams.set('direction', params.direction);
    if (params?.include_evidence !== undefined) {
      queryParams.set('include_evidence', String(params.include_evidence));
    }
    if (params?.limit_per_level !== undefined) {
      queryParams.set('limit_per_level', String(params.limit_per_level));
    }
    if (params?.cursors) {
      Object.entries(params.cursors).forEach(([level, cursor]) => {
        queryParams.append(`cursor[${level}]`, cursor);
      });
    }
    const query = queryParams.toString() ? `?${queryParams.toString()}` : '';
    return this.client.request<types.LineageGraphResponse>(`/v1/lineage/adapter_versions/${adapterVersionId}${query}`);
  }

  /**
   * Get journey information
   *
   * GET /v1/journeys/:journeyType/:journeyId
   *
   * @param journeyType - The type of journey
   * @param journeyId - The journey ID
   * @returns Journey response
   */
  async getJourney(journeyType: string, journeyId: string): Promise<types.JourneyResponse> {
    return this.client.request<types.JourneyResponse>(`/v1/journeys/${journeyType}/${journeyId}`);
  }

  // ============================================================================
  // Adapter Policy Management
  // ============================================================================

  /**
   * Update adapter policy
   *
   * PUT /v1/adapters/:adapterId/policy
   *
   * @param adapterId - The adapter ID
   * @param req - Policy update request
   * @returns Policy update response
   */
  async updateAdapterPolicy(
    adapterId: string,
    req: types.UpdateAdapterPolicyRequest
  ): Promise<types.UpdateAdapterPolicyResponse> {
    return this.client.request<types.UpdateAdapterPolicyResponse>(`/v1/adapters/${adapterId}/policy`, {
      method: 'PUT',
      body: JSON.stringify(req),
    });
  }

  /**
   * Get all category policies
   *
   * GET /v1/adapters/category-policies
   *
   * @returns Record of category policies
   */
  async getCategoryPolicies(): Promise<Record<types.AdapterCategory, types.CategoryPolicy>> {
    return this.client.request<Record<types.AdapterCategory, types.CategoryPolicy>>('/v1/adapters/category-policies');
  }

  /**
   * Get category policy for a specific category
   *
   * GET /v1/adapters/category-policies/:category
   *
   * @param category - The adapter category
   * @returns Category policy
   */
  async getCategoryPolicy(category: types.AdapterCategory): Promise<types.CategoryPolicy> {
    return this.client.request<types.CategoryPolicy>(`/v1/adapters/category-policies/${category}`);
  }

  /**
   * Update category policy
   *
   * PUT /v1/adapters/category-policies/:category
   *
   * @param category - The adapter category
   * @param policy - Updated category policy
   * @returns Updated category policy
   */
  async updateCategoryPolicy(
    category: types.AdapterCategory,
    policy: types.CategoryPolicy
  ): Promise<types.CategoryPolicy> {
    return this.client.request<types.CategoryPolicy>(`/v1/adapters/category-policies/${category}`, {
      method: 'PUT',
      body: JSON.stringify(policy),
    });
  }

  // ============================================================================
  // Domain Adapters
  // ============================================================================

  /**
   * List domain adapters
   *
   * GET /v1/domain-adapters
   *
   * @returns Array of domain adapters
   */
  async listDomainAdapters(): Promise<types.DomainAdapter[]> {
    return this.client.requestList<types.DomainAdapter>('/v1/domain-adapters');
  }

  /**
   * Test a domain adapter
   *
   * POST /v1/domain-adapters/:adapterId/test
   *
   * @param adapterId - The adapter ID
   * @param inputData - Test input data
   * @param expectedOutput - Optional expected output
   * @param iterations - Number of test iterations (default: 100)
   * @returns Test results
   */
  async testDomainAdapter(
    adapterId: string,
    inputData: string,
    expectedOutput?: string,
    iterations?: number
  ): Promise<types.TestDomainAdapterResponse> {
    return this.client.request<types.TestDomainAdapterResponse>(`/v1/domain-adapters/${adapterId}/test`, {
      method: 'POST',
      body: JSON.stringify({
        adapter_id: adapterId,
        input_data: inputData,
        expected_output: expectedOutput,
        iterations: iterations || 100,
      }),
    });
  }

  // ============================================================================
  // Memory and Eviction Management
  // ============================================================================

  /**
   * Get memory usage information
   *
   * GET /v1/memory/usage
   *
   * @returns Memory usage statistics
   */
  async getMemoryUsage(): Promise<{
    adapters: Array<{
      id: string;
      name: string;
      memory_usage_mb: number;
      state: string;
      pinned: boolean;
      category: string;
    }>;
    total_memory_mb: number;
    available_memory_mb: number;
    memory_pressure_level: 'low' | 'medium' | 'high' | 'critical';
  }> {
    return this.client.request('/v1/memory/usage');
  }

  /**
   * Evict adapter from memory
   *
   * POST /v1/memory/adapters/:adapterId/evict
   *
   * @param adapterId - The adapter ID
   * @returns Eviction result
   */
  async evictAdapter(adapterId: string): Promise<{ success: boolean; message: string }> {
    return this.client.request(`/v1/memory/adapters/${adapterId}/evict`, {
      method: 'POST',
    });
  }

  // ============================================================================
  // Training Session Management
  // ============================================================================

  /**
   * Start adapter training session
   *
   * POST /v1/training/sessions
   *
   * @param data - Training session data
   * @returns Training session response
   */
  async startAdapterTraining(data: {
    repository_path: string;
    adapter_name: string;
    description: string;
    training_config: Record<string, unknown>;
    tenant_id: string;
  }): Promise<{ session_id: string; status: string; created_at: string }> {
    return this.client.request<{ session_id: string; status: string; created_at: string }>('/v1/training/sessions', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  /**
   * Get training session details
   *
   * GET /v1/training/sessions/:sessionId
   *
   * @param sessionId - The training session ID
   * @returns Training session details
   */
  async getTrainingSession(sessionId: string): Promise<{
    session_id: string;
    status: 'pending' | 'running' | 'completed' | 'failed';
    progress: number;
    adapter_name: string;
    repository_path: string;
    created_at: string;
    updated_at: string;
    error_message?: string;
  }> {
    return this.client.request(`/v1/training/sessions/${sessionId}`);
  }

  /**
   * List training sessions
   *
   * GET /v1/training/sessions
   *
   * @param tenantId - Optional tenant ID filter
   * @returns Array of training sessions
   */
  async listTrainingSessions(tenantId?: string): Promise<
    Array<{
      session_id: string;
      status: string;
      adapter_name: string;
      repository_path: string;
      created_at: string;
      updated_at: string;
    }>
  > {
    const params = new URLSearchParams();
    if (tenantId) params.append('tenant_id', tenantId);

    const queryString = params.toString();
    return this.client.request(`/v1/training/sessions${queryString ? `?${queryString}` : ''}`);
  }

  /**
   * Pause training session
   *
   * Note: Not supported in current build
   *
   * @param sessionId - The training session ID
   * @returns Training session status
   */
  async pauseTrainingSession(sessionId: string): Promise<{
    session_id: string;
    status: 'running' | 'cancelled';
    message: string;
  }> {
    return Promise.reject(new Error('Training pause/resume is not supported in this build'));
  }

  /**
   * Resume training session
   *
   * Note: Not supported in current build
   *
   * @param sessionId - The training session ID
   * @returns Training session status
   */
  async resumeTrainingSession(sessionId: string): Promise<{
    session_id: string;
    status: 'running';
    message: string;
  }> {
    return Promise.reject(new Error('Training pause/resume is not supported in this build'));
  }

  // ============================================================================
  // Behavior Event Tracking
  // ============================================================================

  /**
   * Get behavior events
   *
   * GET /v1/behavior-events
   *
   * @param filters - Optional filters for events
   * @returns Array of behavior events
   */
  async getBehaviorEvents(filters?: adapterTypes.BehaviorEventFilters): Promise<adapterTypes.BehaviorEvent[]> {
    const params = new URLSearchParams();
    if (filters) {
      Object.entries(filters).forEach(([key, value]) => {
        if (value !== undefined && value !== null) {
          params.append(key, String(value));
        }
      });
    }
    const queryString = params.toString();
    return this.client.requestList<adapterTypes.BehaviorEvent>(
      `/v1/behavior-events${queryString ? `?${queryString}` : ''}`
    );
  }

  /**
   * Get behavior event statistics
   *
   * GET /v1/behavior-events/stats
   *
   * @param tenantId - Optional tenant ID filter
   * @returns Behavior statistics
   */
  async getBehaviorStats(tenantId?: string): Promise<adapterTypes.BehaviorStats> {
    const params = new URLSearchParams();
    if (tenantId) {
      params.append('tenant_id', tenantId);
    }
    const queryString = params.toString();
    return this.client.request<adapterTypes.BehaviorStats>(
      `/v1/behavior-events/stats${queryString ? `?${queryString}` : ''}`
    );
  }

  /**
   * Export behavior data to JSONL format
   *
   * POST /v1/behavior-events/export
   *
   * @param request - Export configuration
   * @returns Blob of JSONL data
   */
  async exportBehaviorData(request: adapterTypes.BehaviorExportRequest): Promise<Blob> {
    const token = this.client.getToken();
    const baseUrl = this.client['baseUrl']; // Access private baseUrl

    const response = await fetch(`${baseUrl}/v1/behavior-events/export`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(token && { Authorization: `Bearer ${token}` }),
      },
      body: JSON.stringify(request),
    });

    if (!response.ok) {
      throw new Error(`Export failed: ${response.statusText}`);
    }

    return response.blob();
  }
}
