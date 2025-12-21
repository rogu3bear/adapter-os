import type { ApiClient } from '@/api/client';
import * as types from '@/api/types';
import * as apiTypes from '@/api/api-types';
import { extractArrayFromResponse } from '@/api/helpers';

/**
 * ModelsService
 *
 * Handles all model-related API operations including:
 * - Model status queries (base model, all models)
 * - Model listing and querying
 * - Model lifecycle operations (import, load, unload)
 * - Model validation and downloading
 * - Cursor configuration
 *
 * Citation: Extracted from client.ts lines 2010-2112
 */
export class ModelsService {
  constructor(private client: ApiClient) {}

  /**
   * Get base model status
   *
   * @param tenantId - Optional tenant ID for scoped query
   * @returns Base model status information
   */
  async getBaseModelStatus(tenantId?: string): Promise<types.BaseModelStatus> {
    const query = tenantId ? `?tenant_id=${tenantId}` : '';
    return this.client.request<types.BaseModelStatus>(`/v1/models/status${query}`);
  }

  /**
   * Get all loaded models status
   *
   * @param tenantId - Optional tenant ID for scoped query
   * @returns Status information for all loaded models
   */
  async getAllModelsStatus(tenantId?: string): Promise<types.AllModelsStatusResponse> {
    const query = tenantId ? `?tenant_id=${tenantId}` : '';
    return this.client.request<types.AllModelsStatusResponse>(`/v1/models/status/all${query}`);
  }

  /**
   * List models with stats for ModelSelector
   *
   * DEFENSIVE: Use extractArrayFromResponse to handle potential PaginatedResponse migration
   *
   * @returns Array of models with statistics
   */
  async listModels(): Promise<apiTypes.ModelWithStatsResponse[]> {
    const resp = await this.client.request<unknown>(`/v1/models`);
    return extractArrayFromResponse<apiTypes.ModelWithStatsResponse>(resp);
  }

  /**
   * Helper: list models with optional runtime status data.
   * Falls back gracefully if status endpoint is not accessible to the user.
   *
   * @param tenantId - Optional tenant ID for scoped query
   * @returns Array of models with statistics and optional status
   */
  async listModelsWithStatus(
    tenantId?: string
  ): Promise<
    Array<apiTypes.ModelWithStatsResponse & { status?: types.BaseModelStatus }>
  > {
    const [models, statusResp] = await Promise.all([
      this.listModels(),
      this.getAllModelsStatus(tenantId).catch(() => null),
    ]);

    const statusModels: types.BaseModelStatus[] = statusResp?.models ?? [];

    const statusById = statusModels.reduce<Record<string, types.BaseModelStatus>>(
      (acc, s) => {
        acc[s.model_id] = s;
        return acc;
      },
      {},
    );

    return models.map((model) => ({
      ...model,
      status: statusById[model.id],
    }));
  }

  /**
   * Import a model into the system
   *
   * Citation: IMPLEMENTATION_PLAN.md Phase 2
   *
   * @param data - Model import request data
   * @param options - Additional request options
   * @param skipRetry - Whether to skip retry logic
   * @param cancelToken - Abort signal for cancellation
   * @returns Import model response with job information
   */
  async importModel(
    data: types.ImportModelRequest,
    options: RequestInit = {},
    skipRetry: boolean = false,
    cancelToken?: AbortSignal
  ): Promise<types.ImportModelResponse> {
    return this.client.request<types.ImportModelResponse>(
      '/v1/models/import',
      {
        method: 'POST',
        body: JSON.stringify(data),
        ...options,
      },
      skipRetry,
      cancelToken
    );
  }

  /**
   * Load a base model
   *
   * @param modelId - The model ID to load
   * @returns Model status response
   */
  async loadBaseModel(modelId: string): Promise<types.ModelStatusResponse> {
    return this.client.request<types.ModelStatusResponse>(`/v1/models/${modelId}/load`, {
      method: 'POST',
    });
  }

  /**
   * Unload a base model
   *
   * @param modelId - The model ID to unload
   */
  async unloadBaseModel(modelId: string): Promise<void> {
    return this.client.request<void>(`/v1/models/${modelId}/unload`, {
      method: 'POST',
    });
  }

  /**
   * Get model status by ID
   *
   * @param modelId - The model ID to query
   * @returns Model status response
   */
  async getModelStatus(modelId: string): Promise<types.ModelStatusResponse> {
    return this.client.request<types.ModelStatusResponse>(
      `/v1/models/${encodeURIComponent(modelId)}/status`
    );
  }

  /**
   * Get model import status
   *
   * @param importId - The import job ID
   * @returns Import model response with status
   */
  async getModelImportStatus(importId: string): Promise<types.ImportModelResponse> {
    return this.client.request<types.ImportModelResponse>(`/v1/models/imports/${importId}`);
  }

  /**
   * Get cursor configuration
   *
   * @returns Cursor configuration response
   */
  async getCursorConfig(): Promise<types.CursorConfigResponse> {
    return this.client.request<types.CursorConfigResponse>('/v1/models/cursor-config');
  }

  /**
   * Validate a model
   *
   * @param modelId - The model ID to validate
   * @returns Model validation response
   */
  async validateModel(modelId: string): Promise<types.ModelValidationResponse> {
    return this.client.request<types.ModelValidationResponse>(`/v1/models/${modelId}/validate`);
  }

  /**
   * Start downloading a model from HuggingFace
   * Returns immediately with a job ID that can be polled for progress
   *
   * @param modelId - The model ID to download
   * @returns Download job response with job ID
   */
  async downloadModel(modelId: string): Promise<types.DownloadJobResponse> {
    return this.client.request<types.DownloadJobResponse>(
      `/v1/models/${encodeURIComponent(modelId)}/download`,
      {
        method: 'POST',
      }
    );
  }

  /**
   * Get the status of a model download job
   *
   * @param modelId - The model ID
   * @param jobId - The download job ID
   * @returns Download job response with status
   */
  async getDownloadStatus(modelId: string, jobId: string): Promise<types.DownloadJobResponse> {
    return this.client.request<types.DownloadJobResponse>(
      `/v1/models/${encodeURIComponent(modelId)}/download/${jobId}`
    );
  }
}
