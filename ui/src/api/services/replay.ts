/**
 * Replay service - handles replay sessions, verification, and deterministic inference replay.
 */

import type { ApiClient } from '@/api/client';
import * as types from '@/api/types';
import * as apiTypes from '@/api/api-types';
import * as replayTypes from '@/api/replay-types';

export class ReplayService {
  constructor(private client: ApiClient) {}

  /**
   * List replay sessions
   *
   * GET /v1/replay/sessions
   *
   * @param tenantId - Optional tenant ID to filter sessions
   * @returns List of replay sessions
   */
  async listReplaySessions(tenantId?: string): Promise<apiTypes.ReplaySession[]> {
    const query = tenantId ? `?tenant_id=${tenantId}` : '';
    return this.client.requestList<apiTypes.ReplaySession>(`/v1/replay/sessions${query}`);
  }

  /**
   * Get a specific replay session
   *
   * GET /v1/replay/sessions/:sessionId
   *
   * @param sessionId - ID of the replay session
   * @returns Replay session details
   */
  async getReplaySession(sessionId: string): Promise<apiTypes.ReplaySession> {
    return this.client.request<apiTypes.ReplaySession>(`/v1/replay/sessions/${sessionId}`);
  }

  /**
   * Create a new replay session
   *
   * POST /v1/replay/sessions
   *
   * @param data - Replay session creation request
   * @returns Created replay session
   */
  async createReplaySession(data: apiTypes.CreateReplaySessionRequest): Promise<apiTypes.ReplaySession> {
    return this.client.request<apiTypes.ReplaySession>('/v1/replay/sessions', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  /**
   * Verify a replay session
   *
   * POST /v1/replay/sessions/:sessionId/verify
   *
   * @param sessionId - ID of the replay session to verify
   * @returns Verification results
   */
  async verifyReplaySession(sessionId: string): Promise<apiTypes.ReplayVerificationResponse> {
    return this.client.request<apiTypes.ReplayVerificationResponse>(`/v1/replay/sessions/${sessionId}/verify`, {
      method: 'POST',
    });
  }

  /**
   * Verify a trace receipt
   *
   * POST /v1/replay/verify/trace
   *
   * @param traceId - ID of the trace to verify
   * @returns Receipt verification result
   */
  async verifyTraceReceipt(traceId: string): Promise<apiTypes.ReceiptVerificationResult> {
    return this.client.request<apiTypes.ReceiptVerificationResult>('/v1/replay/verify/trace', {
      method: 'POST',
      body: JSON.stringify({ trace_id: traceId }),
    });
  }

  /**
   * Verify an evidence bundle
   *
   * POST /v1/replay/verify/bundle
   *
   * @param file - Evidence bundle file to verify
   * @returns Receipt verification result
   */
  async verifyEvidenceBundle(file: File): Promise<apiTypes.ReceiptVerificationResult> {
    const formData = new FormData();
    formData.append('bundle', file);

    return this.client.request<apiTypes.ReceiptVerificationResult>('/v1/replay/verify/bundle', {
      method: 'POST',
      body: formData,
      headers: {}, // Let the browser set multipart boundaries
    });
  }

  /**
   * Delete a replay session
   *
   * DELETE /v1/replay/sessions/:sessionId
   *
   * @param sessionId - ID of the replay session to delete
   */
  async deleteReplaySession(sessionId: string): Promise<void> {
    return this.client.request<void>(`/v1/replay/sessions/${sessionId}`, {
      method: 'DELETE',
    });
  }

  /**
   * Check if an inference can be replayed deterministically
   *
   * GET /v1/replay/check/:inferenceId
   *
   * @param inferenceId - ID of the inference to check
   * @returns Replay availability information
   */
  async checkReplayAvailability(inferenceId: string): Promise<replayTypes.ReplayAvailabilityResponse> {
    return this.client.request<replayTypes.ReplayAvailabilityResponse>(
      `/v1/replay/check/${inferenceId}`
    );
  }

  /**
   * Execute a deterministic replay
   *
   * POST /v1/replay
   *
   * @param request - Replay execution request
   * @returns Replay execution result
   */
  async executeReplay(request: replayTypes.ReplayRequest): Promise<replayTypes.ReplayResponse> {
    return this.client.request<replayTypes.ReplayResponse>('/v1/replay', {
      method: 'POST',
      body: JSON.stringify(request),
    });
  }

  /**
   * Get replay execution history for an inference
   *
   * GET /v1/replay/history/:inferenceId
   *
   * @param inferenceId - ID of the inference
   * @returns History of replay executions
   */
  async getReplayHistory(inferenceId: string): Promise<replayTypes.ReplayHistoryResponse> {
    return this.client.request<replayTypes.ReplayHistoryResponse>(
      `/v1/replay/history/${inferenceId}`
    );
  }
}
