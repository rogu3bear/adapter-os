/**
 * Routing service - handles routing decisions, backend management, and inference operations.
 */

import type { ApiClient } from '@/api/client';
import * as types from '@/api/types';
import { logger } from '@/utils/logger';

// Type extension for ApiClient streaming method (implementation pending)
 
interface ApiClientWithStreaming extends ApiClient {
  streamInfer(
    request: any,
    callbacks: {
      onToken: (token: string, chunk: any) => void;
      onComplete: (text: string, finishReason: string | null, metadata?: any) => void;
      onError: (error: Error) => void;
    },
    cancelToken?: AbortSignal
  ): Promise<void>;
}

export class RoutingService {
  constructor(private client: ApiClient) {}

  /**
   * Debug routing decisions
   *
   * POST /v1/routing/debug
   *
   * @param data - Routing debug request
   * @returns Routing debug response with decision details
   */
  async debugRouting(data: types.RoutingDebugRequest): Promise<types.RoutingDebugResponse> {
    return this.client.request<types.RoutingDebugResponse>('/v1/routing/debug', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  /**
   * Get routing decision history
   *
   * GET /v1/routing/history
   *
   * @param limit - Optional limit on number of decisions to return
   * @returns List of routing decisions
   */
  async getRoutingHistory(limit?: number): Promise<types.RoutingDecision[]> {
    const query = limit ? `?limit=${limit}` : '';
    return this.client.requestList<types.RoutingDecision>(`/v1/routing/history${query}`);
  }

  /**
   * List available backends
   *
   * GET /v1/backends
   *
   * @returns List of available backends
   */
  async listBackends(): Promise<types.BackendListResponse> {
    return this.client.request<types.BackendListResponse>('/v1/backends');
  }

  /**
   * Get backend capabilities
   *
   * GET /v1/backends/capabilities
   *
   * @returns Backend capabilities information
   */
  async getBackendCapabilities(): Promise<types.BackendCapabilitiesResponse> {
    return this.client.request<types.BackendCapabilitiesResponse>('/v1/backends/capabilities');
  }

  /**
   * Get backend status
   *
   * GET /v1/backends/:name/status
   *
   * @param name - Backend name
   * @returns Backend status information
   */
  async getBackendStatus(name: types.BackendName): Promise<types.BackendStatusResponse> {
    return this.client.request<types.BackendStatusResponse>(`/v1/backends/${name}/status`);
  }

  /**
   * Perform inference
   *
   * POST /v1/infer
   *
   * @param data - Inference request
   * @param options - Optional request options
   * @param skipRetry - Whether to skip retry logic
   * @param cancelToken - Optional abort signal for cancellation
   * @returns Inference response
   */
  async infer(data: types.InferRequest, options: RequestInit = {}, skipRetry: boolean = false, cancelToken?: AbortSignal): Promise<types.InferResponse> {
    return this.client.request<types.InferResponse>('/v1/infer', {
      method: 'POST',
      body: JSON.stringify(data),
      ...options,
    }, skipRetry, cancelToken);
  }

  /**
   * Perform batch inference
   *
   * POST /v1/infer/batch
   *
   * @param data - Batch inference request
   * @param cancelToken - Optional abort signal for cancellation
   * @returns Batch inference response
   */
  async batchInfer(data: types.BatchInferRequest, cancelToken?: AbortSignal): Promise<types.BatchInferResponse> {
    logger.info('Batch inference requested', {
      component: 'RoutingService',
      operation: 'batchInfer',
      batchSize: data.requests?.length ?? 0,
    });
    return this.client.request<types.BatchInferResponse>('/v1/infer/batch', {
      method: 'POST',
      body: JSON.stringify(data),
    }, false, cancelToken);
  }

  /**
   * Stream inference using the /v1/infer/stream endpoint with SSE
   *
   * POST /v1/infer/stream
   *
   * Note: This method delegates to ApiClient.streamInfer() because streaming
   * requires direct access to private client internals (baseUrl, token, etc).
   *
   * @param data - The streaming inference request payload
   * @param callbacks - Event callbacks for streaming tokens
   * @param cancelToken - Optional abort signal for cancellation
   * @returns Promise that resolves when stream completes
   */
  async streamInfer(
    data: types.StreamingInferRequest,
    callbacks: {
      onToken: (token: string, chunk: types.StreamingChunk) => void;
      onComplete: (
        fullText: string,
        finishReason: string | null,
        metadata?: {
          request_id?: string;
          unavailable_pinned_adapters?: string[];
          pinned_routing_fallback?: types.InferResponse['pinned_routing_fallback'];
          citations?: types.Citation[];
        }
      ) => void;
      onError: (error: Error) => void;
    },
    cancelToken?: AbortSignal
  ): Promise<void> {
    // Delegate to ApiClient which has access to private members needed for streaming
    // TODO: Implement ApiClient.streamInfer method or move implementation here
    // Type assertion needed because streamInfer method is not yet implemented on ApiClient
    return (this.client as ApiClientWithStreaming).streamInfer(data, callbacks, cancelToken);
  }

  /**
   * Get session router view
   *
   * GET /v1/routing/sessions/:requestId
   *
   * @param requestId - Request ID
   * @returns Session router view response
   */
  async getSessionRouterView(requestId: string): Promise<types.SessionRouterViewResponse> {
    return this.client.request<types.SessionRouterViewResponse>(`/v1/routing/sessions/${requestId}`);
  }

  /**
   * Get router configuration for a tenant
   *
   * GET /v1/tenants/:tenantId/router/config
   *
   * @param tenantId - Tenant ID
   * @returns Router configuration view
   */
  async getRouterConfig(tenantId: string): Promise<types.RouterConfigView> {
    const effectiveTenant = tenantId || 'default';
    return this.client.request<types.RouterConfigView>(
      `/v1/tenants/${effectiveTenant}/router/config`
    );
  }

  /**
   * Get routing decisions with optional filters
   *
   * GET /v1/routing/decisions
   *
   * @param filters - Optional filters for routing decisions
   * @returns List of transformed routing decisions
   */
  async getRoutingDecisions(filters?: types.RoutingDecisionFilters): Promise<types.TransformedRoutingDecision[]> {
    const params = new URLSearchParams();
    // Backend requires 'tenant' parameter (not tenant_id) - see RoutingDecisionsQuery
    const tenantId = filters?.tenant_id || 'default';
    params.append('tenant', tenantId);

    if (filters?.limit) {
      params.append('limit', filters.limit.toString());
    }
    if (filters?.offset) {
      params.append('offset', filters.offset.toString());
    }
    if (filters?.adapter_id) {
      params.append('adapter_id', filters.adapter_id);
    }
    if (filters?.stack_id) {
      params.append('stack_id', filters.stack_id);
    }
    if (filters?.since) {
      params.append('since', filters.since);
    }
    if (filters?.until) {
      params.append('until', filters.until);
    }
    if (filters?.min_entropy !== undefined) {
      params.append('min_entropy', filters.min_entropy.toString());
    }
    if (filters?.max_overhead_pct !== undefined) {
      params.append('max_overhead_pct', filters.max_overhead_pct.toString());
    }
    if (filters?.anomalies_only) {
      params.append('anomalies_only', 'true');
    }
    if (filters?.source_type) {
      params.append('source_type', filters.source_type);
    }

    const query = `?${params.toString()}`;

    logger.debug('Fetching routing decisions', {
      component: 'RoutingService',
      operation: 'getRoutingDecisions',
      query,
      tenant_id: tenantId,
    });

    // Backend returns RoutingDecisionsResponse with 'items' field
    interface BackendRoutingDecision {
      ts: string;
      tenant_id: string;
      adapters_used: string[];
      activations: number[];
      reason: string;
      trace_id: string;
    }

    interface BackendRoutingDecisionsResponse {
      items: BackendRoutingDecision[];
    }

    const response = await this.client.request<BackendRoutingDecisionsResponse>(`/v1/routing/decisions${query}`);

    // Transform backend format to frontend format
    // Must satisfy TransformedRoutingDecision which extends RoutingDecision
    return response.items.map((item, index) => {
      const scores: Record<string, number> = {};
      item.adapters_used.forEach((adapter, i) => {
        scores[adapter] = item.activations[i] || 0;
      });

      // Transform candidates into RouterCandidateInfo objects
      const candidates: types.RouterCandidateInfo[] = item.adapters_used.map((adapter, i) => ({
        adapter_id: adapter,
        adapter_idx: i,
        gate_q15: Math.round((item.activations[i] || 0) * 32767), // Convert float to Q15
        gate_float: item.activations[i] || 0,
        raw_score: item.activations[i] || 0,
        selected: true, // All adapters_used are selected
      }));

      return {
        // Required TransformedRoutingDecision fields
        id: item.trace_id || `decision-${index}`,
        request_id: item.trace_id || `decision-${index}`,
        selected_adapters: item.adapters_used,
        scores,
        timestamp: item.ts,
        latency_ms: 0, // Not provided by backend

        // Additional TransformedRoutingDecision fields
        transformed: true,
        display_adapters: item.adapters_used,

        // Routing inspector fields
        entropy: this.calculateEntropy(item.activations),
        k_value: item.adapters_used.length,
        router_latency_us: undefined, // Not provided by backend
        candidates,

        // Optional fields
        tau: 1.0,
        entropy_floor: 0.0,
        step: index,
      };
    });
  }

  /**
   * Calculate Shannon entropy for a set of values
   *
   * @param values - Array of activation values
   * @returns Shannon entropy
   */
  private calculateEntropy(values: number[]): number {
    if (values.length === 0) return 0;
    // Normalize values to probabilities
    const sum = values.reduce((a, b) => a + b, 0);
    if (sum === 0) return 0;
    const probs = values.map(v => v / sum);
    // Calculate Shannon entropy
    return -probs.reduce((entropy, p) => {
      if (p === 0) return entropy;
      return entropy + p * Math.log2(p);
    }, 0);
  }
}
