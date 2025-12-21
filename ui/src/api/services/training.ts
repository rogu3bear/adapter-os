import type { ApiClient } from '@/api/client';
import * as types from '@/api/types';
import * as trainingTypes from '@/api/training-types';
import { logger, toError } from '@/utils/logger';
import { handleBlobResponse, getFilenameFromResponse, extractArrayFromResponse } from '@/api/helpers';

/**
 * TrainingService
 *
 * Handles all training-related API operations including:
 * - Training job management (start, cancel, monitor)
 * - Training artifacts and metrics
 * - Training templates
 * - Dataset management (create, upload, validate)
 * - Dataset version management and trust overrides
 * - Chat bootstrap from training jobs
 * - Telemetry bundles and logs
 * - Golden runs and baselines
 * - Audit and compliance exports
 *
 * NOTE: Most training-types.ts interfaces use snake_case to match backend conventions.
 * Transformations are only applied where explicitly needed for consistency with
 * other frontend types that use camelCase.
 *
 * Citation: Extracted from client.ts lines 1375-3048
 */
export class TrainingService {
  constructor(private client: ApiClient) {}

  // ============================================================================
  // Training Job Management
  // ============================================================================

  /**
   * List training jobs with optional filters
   *
   * GET /v1/training/jobs
   *
   * @param params - Optional filters for dataset, status, adapter name, template, and pagination
   * @returns List of training jobs with metadata
   */
  async listTrainingJobs(params?: {
    dataset_id?: string;
    status?: string;
    adapter_name?: string;
    template_id?: string;
    page?: number;
    page_size?: number;
  }): Promise<trainingTypes.ListTrainingJobsResponse> {
    const queryParams = new URLSearchParams();
    if (params?.dataset_id) queryParams.append('dataset_id', params.dataset_id);
    if (params?.status) queryParams.append('status', params.status);
    if (params?.adapter_name) queryParams.append('adapter_name', params.adapter_name);
    if (params?.template_id) queryParams.append('template_id', params.template_id);
    if (params?.page) queryParams.append('page', params.page.toString());
    if (params?.page_size) queryParams.append('page_size', params.page_size.toString());

    const queryString = queryParams.toString();
    const url = queryString ? `/v1/training/jobs?${queryString}` : '/v1/training/jobs';
    return this.client.request<trainingTypes.ListTrainingJobsResponse>(url);
  }

  /**
   * Get details for a specific training job
   *
   * GET /v1/training/jobs/:jobId
   *
   * @param jobId - Training job ID
   * @returns Training job details
   */
  async getTrainingJob(jobId: string): Promise<trainingTypes.TrainingJob> {
    return this.client.request<trainingTypes.TrainingJob>(`/v1/training/jobs/${jobId}`);
  }

  /**
   * Get artifacts produced by a training job
   *
   * GET /v1/training/jobs/:jobId/artifacts
   *
   * @param jobId - Training job ID
   * @returns List of training artifacts (model files, checkpoints, etc.)
   */
  async getTrainingArtifacts(jobId: string): Promise<types.TrainingArtifactsResponse> {
    return this.client.request<types.TrainingArtifactsResponse>(`/v1/training/jobs/${jobId}/artifacts`);
  }

  /**
   * Start a new training job
   *
   * POST /v1/training/start
   *
   * @param request - Training configuration
   * @returns Created training job
   */
  async startTraining(request: trainingTypes.StartTrainingRequest): Promise<trainingTypes.TrainingJob> {
    return this.client.request<trainingTypes.TrainingJob>('/v1/training/start', {
      method: 'POST',
      body: JSON.stringify(request),
    });
  }

  /**
   * Cancel a running training job
   *
   * POST /v1/training/jobs/:jobId/cancel
   *
   * @param jobId - Training job ID to cancel
   */
  async cancelTraining(jobId: string): Promise<void> {
    return this.client.request<void>(`/v1/training/jobs/${jobId}/cancel`, {
      method: 'POST',
    });
  }

  /**
   * Get training logs for a job
   *
   * GET /v1/training/jobs/:jobId/logs
   *
   * @param jobId - Training job ID
   * @returns Array of log lines
   */
  async getTrainingLogs(jobId: string): Promise<string[]> {
    return this.client.requestList<string>(`/v1/training/jobs/${jobId}/logs`);
  }

  /**
   * Get training metrics (loss, accuracy, etc.)
   *
   * GET /v1/training/jobs/:jobId/metrics
   *
   * @param jobId - Training job ID
   * @returns Training metrics
   */
  async getTrainingMetrics(jobId: string): Promise<trainingTypes.TrainingMetrics> {
    return this.client.request<trainingTypes.TrainingMetrics>(`/v1/training/jobs/${jobId}/metrics`);
  }

  /**
   * Download a training artifact file
   * Triggers a blob download for the artifact
   *
   * GET /v1/training/jobs/:jobId/artifacts/:artifactId/download
   *
   * @param jobId - Training job ID
   * @param artifactId - Artifact ID to download
   * @param filename - Optional filename override
   */
  async downloadArtifact(jobId: string, artifactId: string, filename?: string): Promise<void> {
    const path = `/v1/training/jobs/${jobId}/artifacts/${artifactId}/download`;
    const url = this.client.buildUrl(path);
    const token = this.client.getToken();

    try {
      const response = await fetch(url, {
        method: 'GET',
        headers: token ? { Authorization: `Bearer ${token}` } : undefined,
        credentials: 'omit',
      });

      // Use helper for blob response with error handling
      const blob = await handleBlobResponse(response, { method: 'GET', path });

      // Get filename from Content-Disposition header or use provided filename
      const downloadFilename = getFilenameFromResponse(response, filename || artifactId);
      const blobUrl = window.URL.createObjectURL(blob);
      const link = document.createElement('a');
      link.href = blobUrl;
      link.download = downloadFilename;
      document.body.appendChild(link);
      link.click();
      document.body.removeChild(link);
      window.URL.revokeObjectURL(blobUrl);

      logger.info('Artifact downloaded', {
        component: 'TrainingService',
        operation: 'downloadArtifact',
        jobId,
        artifactId,
        filename: downloadFilename,
      });
    } catch (error) {
      logger.error('Failed to download artifact', {
        component: 'TrainingService',
        operation: 'downloadArtifact',
        jobId,
        artifactId,
      }, toError(error));
      throw error;
    }
  }

  // ============================================================================
  // Training Templates
  // ============================================================================

  /**
   * List available training templates
   *
   * GET /v1/training/templates
   *
   * @returns Array of training templates
   */
  async listTrainingTemplates(): Promise<types.TrainingTemplate[]> {
    return this.client.requestList<types.TrainingTemplate>('/v1/training/templates');
  }

  /**
   * Get a specific training template
   *
   * GET /v1/training/templates/:templateId
   *
   * @param templateId - Template ID
   * @returns Training template details
   */
  async getTrainingTemplate(templateId: string): Promise<types.TrainingTemplate> {
    return this.client.request<types.TrainingTemplate>(`/v1/training/templates/${templateId}`);
  }

  // ============================================================================
  // Chat Bootstrap from Training
  // ============================================================================

  /**
   * Get chat bootstrap data for a training job
   * Returns the "recipe" for starting a chat from a completed training job
   *
   * GET /v1/training/jobs/:jobId/chat_bootstrap
   *
   * @param jobId - Training job ID
   * @returns Chat bootstrap configuration
   */
  async getChatBootstrap(jobId: string): Promise<trainingTypes.ChatBootstrapResponse> {
    return this.client.request<trainingTypes.ChatBootstrapResponse>(`/v1/training/jobs/${jobId}/chat_bootstrap`);
  }

  /**
   * Create a chat session from a training job
   * Creates a chat session bound to the training job's stack in one call
   *
   * POST /v1/chats/from_training_job
   *
   * @param request - Chat creation request with job ID
   * @returns Created chat session
   */
  async createChatFromTrainingJob(request: trainingTypes.CreateChatFromJobRequest): Promise<trainingTypes.CreateChatFromJobResponse> {
    return this.client.request<trainingTypes.CreateChatFromJobResponse>('/v1/chats/from_training_job', {
      method: 'POST',
      body: JSON.stringify(request),
    });
  }

  // ============================================================================
  // Dataset Management
  // ============================================================================

  /**
   * Create a new dataset by uploading files
   *
   * POST /v1/datasets/upload (multipart/form-data)
   *
   * @param request - Dataset creation request with files
   * @returns Created dataset
   */
  async createDataset(request: trainingTypes.CreateDatasetRequest): Promise<trainingTypes.DatasetResponse> {
    // Use FormData for file uploads
    const formData = new FormData();
    formData.append('name', request.name);
    formData.append('source_type', request.source_type);
    formData.append('format', request.format ?? 'jsonl');
    if (request.description) formData.append('description', request.description);
    if (request.language) formData.append('language', request.language);
    if (request.framework) formData.append('framework', request.framework);
    if (request.repository_url) formData.append('repository_url', request.repository_url);
    if (request.branch) formData.append('branch', request.branch);
    if (request.commit_hash) formData.append('commit_hash', request.commit_hash);
    if (request.files) {
      request.files.forEach((file) => {
        formData.append('files', file);
      });
    }

    const url = this.client.buildUrl('/v1/datasets/upload');
    const requestId = await this.client.createRequestId('POST', '/v1/datasets/upload', request.name);
    this.client.recordRequest(requestId, 'POST', '/v1/datasets/upload');
    const token = this.client.getToken();

    const response = await fetch(url, {
      method: 'POST',
      headers: {
        'X-Request-ID': requestId,
        ...(token ? { Authorization: `Bearer ${token}` } : {}),
      },
      body: formData,
      credentials: 'omit',
    });

    if (!response.ok) {
      let errorMessage = `HTTP ${response.status}: ${response.statusText}`;
      try {
        const error = await response.json();
        errorMessage = error.error || errorMessage;
      } catch {
        // Use status text
      }
      throw new Error(errorMessage);
    }

    type UploadDatasetResponse = {
      schema_version?: string;
      dataset_id: string;
      name: string;
      description?: string;
      file_count?: number;
      total_size_bytes?: number;
      format?: string;
      hash?: string;
      storage_path?: string;
      created_at?: string;
    };

    const raw = (await response.json()) as UploadDatasetResponse;
    const createdAt = raw.created_at ?? new Date().toISOString();

    // Return flat DatasetResponse structure (not nested)
    return {
      schema_version: raw.schema_version ?? '1.0',
      dataset_id: raw.dataset_id,
      name: raw.name,
      description: raw.description ?? null,
      file_count: raw.file_count ?? request.files?.length ?? 0,
      total_size_bytes: raw.total_size_bytes ?? 0,
      format: raw.format ?? request.format ?? 'jsonl',
      hash: raw.hash ?? '',
      storage_path: raw.storage_path ?? '',
      validation_status: 'pending' as const,
      validation_errors: null,
      created_by: 'current-user',
      created_at: createdAt,
      updated_at: createdAt,
    };
  }

  /**
   * List all datasets with optional pagination
   *
   * GET /v1/datasets
   *
   * @param params - Pagination parameters
   * @returns List of datasets
   */
  async listDatasets(params?: { page?: number; page_size?: number }): Promise<trainingTypes.ListDatasetsResponse> {
    const queryParams = new URLSearchParams();
    if (params?.page) queryParams.append('page', String(params.page));
    if (params?.page_size) queryParams.append('page_size', String(params.page_size));
    const query = queryParams.toString();

    // Backend returns array directly, but frontend expects wrapped response
    // DEFENSIVE: Use extractArrayFromResponse to handle potential PaginatedResponse migration
    type BackendDataset = {
      dataset_id: string;
      dataset_version_id?: string;
      name: string;
      hash: string;
      total_size_bytes: number;
      file_count: number;
      format: string;
      storage_path: string;
      validation_status: string;
      validation_errors?: string;
      created_by: string;
      created_at: string;
      updated_at: string;
      description?: string;
      trust_state?: string;
      trust_reason?: string;
      overall_safety_status?: string;
      pii_status?: string;
      toxicity_status?: string;
      leak_status?: string;
      anomaly_status?: string;
    };
    const rawResponse = await this.client.request<unknown>(`/v1/datasets${query ? `?${query}` : ''}`);
    const response = extractArrayFromResponse<BackendDataset>(rawResponse);

    // Map backend responses to frontend Dataset type
    const datasets: trainingTypes.Dataset[] = response.map((d) => ({
      id: d.dataset_id,
      dataset_version_id: d.dataset_version_id,
      name: d.name,
      hash_b3: d.hash,
      source_type: 'uploaded_files' as trainingTypes.DatasetSourceType, // Default, parse from metadata_json if needed
      file_count: d.file_count,
      total_size_bytes: d.total_size_bytes,
      total_tokens: 0, // Will be fetched separately if needed
      validation_status: d.validation_status as trainingTypes.DatasetValidationStatus,
      created_at: d.created_at,
      updated_at: d.updated_at,
      format: d.format,
      storage_path: d.storage_path,
      validation_errors: d.validation_errors,
      created_by: d.created_by,
      description: d.description,
      trust_state: (d.trust_state as trainingTypes.TrustState) ?? 'unknown',
      trust_reason: d.trust_reason,
      overall_safety_status: d.overall_safety_status,
      pii_status: d.pii_status,
      toxicity_status: d.toxicity_status,
      leak_status: d.leak_status,
      anomaly_status: d.anomaly_status,
    }));

    return {
      schema_version: '1.0',
      datasets,
      total: datasets.length,
      page: params?.page || 1,
      page_size: params?.page_size || datasets.length,
    };
  }

  /**
   * Get a specific dataset by ID
   *
   * GET /v1/datasets/:datasetId
   *
   * @param datasetId - Dataset ID
   * @returns Dataset details
   */
  async getDataset(datasetId: string): Promise<trainingTypes.Dataset> {
    const response = await this.client.request<{
      dataset_id: string;
      dataset_version_id?: string;
      name: string;
      hash: string;
      total_size_bytes: number;
      file_count: number;
      format: string;
      storage_path: string;
      validation_status: string;
      validation_errors?: string;
      created_by: string;
      created_at: string;
      updated_at: string;
      description?: string;
      trust_state?: string;
      trust_reason?: string;
      overall_safety_status?: string;
      pii_status?: string;
      toxicity_status?: string;
      leak_status?: string;
      anomaly_status?: string;
    }>(`/v1/datasets/${datasetId}`);

    // Try to get statistics for total_tokens
    let totalTokens = 0;
    try {
      const stats = await this.client.request<{ total_tokens: number }>(`/v1/datasets/${datasetId}/statistics`).catch((error) => {
        logger.warn('Failed to fetch statistics', { datasetId, error });
        return null;
      });
      if (stats) {
        totalTokens = stats.total_tokens;
      }
    } catch {
      // Statistics not available, use 0
    }

    // Parse metadata_json for source_type if available
    let sourceType: trainingTypes.DatasetSourceType = 'uploaded_files';
    try {
      // Try to infer from format or other fields
      // For now, default to uploaded_files
    } catch {
      // Use default
    }

    // Map backend response to frontend Dataset type
    return {
      id: response.dataset_id,
      dataset_version_id: response.dataset_version_id,
      name: response.name,
      hash_b3: response.hash,
      source_type: sourceType,
      file_count: response.file_count,
      total_size_bytes: response.total_size_bytes,
      total_tokens: totalTokens,
      validation_status: response.validation_status as trainingTypes.DatasetValidationStatus,
      created_at: response.created_at,
      updated_at: response.updated_at,
      format: response.format,
      storage_path: response.storage_path,
      validation_errors: response.validation_errors,
      created_by: response.created_by,
      description: response.description,
      trust_state: (response.trust_state as trainingTypes.TrustState) ?? 'unknown',
      trust_reason: response.trust_reason,
      overall_safety_status: response.overall_safety_status,
      pii_status: response.pii_status,
      toxicity_status: response.toxicity_status,
      leak_status: response.leak_status,
      anomaly_status: response.anomaly_status,
    };
  }

  /**
   * List all versions of a dataset
   *
   * GET /v1/datasets/:datasetId/versions
   *
   * @param datasetId - Dataset ID
   * @returns List of dataset versions
   */
  async listDatasetVersions(datasetId: string): Promise<trainingTypes.DatasetVersionListResponse> {
    const response = await this.client.request<trainingTypes.DatasetVersionListResponse>(
      `/v1/datasets/${encodeURIComponent(datasetId)}/versions`,
    );

    return {
      ...response,
      versions: (response.versions || []).map((v) => ({
        ...v,
        trust_state: (v.trust_state as trainingTypes.TrustState) ?? 'unknown',
      })),
    };
  }

  /**
   * Apply or update a dataset trust override for the latest version
   *
   * POST /v1/datasets/:datasetId/trust_override
   *
   * @param datasetId - Dataset ID
   * @param payload - Trust override configuration
   * @returns Updated trust state
   */
  async applyDatasetTrustOverride(
    datasetId: string,
    payload: trainingTypes.DatasetTrustOverrideRequest
  ): Promise<trainingTypes.DatasetTrustOverrideResponse> {
    return this.client.request<trainingTypes.DatasetTrustOverrideResponse>(
      `/v1/datasets/${encodeURIComponent(datasetId)}/trust_override`,
      {
        method: 'POST',
        body: JSON.stringify(payload),
      }
    );
  }

  /**
   * Get lineage graph for a dataset version
   *
   * GET /v1/lineage/dataset_versions/:datasetVersionId
   *
   * @param datasetVersionId - Dataset version ID
   * @param params - Lineage query parameters
   * @returns Lineage graph
   */
  async getDatasetVersionLineage(
    datasetVersionId: string,
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
    return this.client.request<types.LineageGraphResponse>(`/v1/lineage/dataset_versions/${datasetVersionId}${query}`);
  }

  /**
   * Validate a dataset
   *
   * POST /v1/datasets/:datasetId/validate
   *
   * @param datasetId - Dataset ID
   * @returns Validation results
   */
  async validateDataset(datasetId: string): Promise<trainingTypes.DatasetValidationResult> {
    return this.client.request<trainingTypes.DatasetValidationResult>(`/v1/datasets/${datasetId}/validate`, {
      method: 'POST',
    });
  }

  /**
   * Delete a dataset
   *
   * DELETE /v1/datasets/:datasetId
   *
   * @param datasetId - Dataset ID to delete
   */
  async deleteDataset(datasetId: string): Promise<void> {
    return this.client.request<void>(`/v1/datasets/${datasetId}`, {
      method: 'DELETE',
    });
  }

  /**
   * Create a training dataset from existing documents or a document collection
   * Converts RAG documents into JSONL training format
   *
   * POST /v1/datasets/from-documents
   *
   * @param params - Document IDs or collection ID to convert
   * @returns Created dataset
   */
  async createDatasetFromDocuments(params: {
    document_ids?: string[];
    documentId?: string;
    collection_id?: string;
    collectionId?: string;
    name?: string;
    description?: string;
  }): Promise<trainingTypes.CreateDatasetFromDocumentsResponse> {
    return this.client.request<trainingTypes.CreateDatasetFromDocumentsResponse>('/v1/datasets/from-documents', {
      method: 'POST',
      body: JSON.stringify({
        document_ids: params.document_ids,
        document_id: params.documentId,
        collection_id: params.collection_id ?? params.collectionId,
        name: params.name,
        description: params.description,
      }),
    });
  }

  // ============================================================================
  // Telemetry and Logs
  // ============================================================================

  /**
   * List telemetry bundles
   *
   * GET /v1/telemetry/bundles
   *
   * @returns Array of telemetry bundles
   */
  async listTelemetryBundles(): Promise<types.TelemetryBundle[]> {
    return this.client.requestList<types.TelemetryBundle>('/v1/telemetry/bundles');
  }

  /**
   * Get telemetry logs with optional filters
   *
   * GET /v1/telemetry/logs
   *
   * @param filters - Category, limit, and offset filters
   * @returns Array of telemetry events
   */
  async getTelemetryLogs(filters?: { category?: string; limit?: number; offset?: number }): Promise<types.TelemetryEvent[]> {
    const params = new URLSearchParams();
    if (filters?.category) params.append('category', filters.category);
    if (filters?.limit) params.append('limit', filters.limit.toString());
    if (filters?.offset) params.append('offset', filters.offset.toString());
    const query = params.toString() ? `?${params.toString()}` : '';
    return this.client.requestList<types.TelemetryEvent>(`/v1/telemetry/logs${query}`);
  }

  /**
   * Export a telemetry bundle
   *
   * GET /v1/telemetry/bundles/:bundleId/export
   *
   * @param bundleId - Bundle ID to export
   * @returns Export response with bundle data
   */
  async exportTelemetryBundle(bundleId: string): Promise<types.ExportTelemetryBundleResponse> {
    return this.client.request<types.ExportTelemetryBundleResponse>(`/v1/telemetry/bundles/${bundleId}/export`);
  }

  /**
   * Generate a new telemetry bundle
   *
   * POST /v1/telemetry/bundles/generate
   *
   * @returns Generated bundle metadata
   */
  async generateTelemetryBundle(): Promise<{ id: string; cpid: string; event_count: number; size_bytes: number; created_at: string }> {
    return this.client.request('/v1/telemetry/bundles/generate', { method: 'POST' });
  }

  /**
   * Verify a bundle signature
   *
   * POST /v1/telemetry/bundles/:bundleId/verify
   *
   * @param bundleId - Bundle ID to verify
   * @returns Verification result
   */
  async verifyBundleSignature(bundleId: string): Promise<types.VerifyBundleSignatureResponse> {
    return this.client.request<types.VerifyBundleSignatureResponse>(`/v1/telemetry/bundles/${bundleId}/verify`, {
      method: 'POST',
    });
  }

  /**
   * Purge old telemetry bundles
   *
   * POST /v1/telemetry/bundles/purge
   *
   * @param keepCount - Number of bundles to keep per CPID
   * @returns Purge result with count of deleted bundles
   */
  async purgeOldBundles(keepCount: number): Promise<types.PurgeOldBundlesResponse> {
    return this.client.request<types.PurgeOldBundlesResponse>('/v1/telemetry/bundles/purge', {
      method: 'POST',
      body: JSON.stringify({ keep_bundles_per_cpid: keepCount }),
    });
  }

  /**
   * Get telemetry events with filters
   *
   * GET /v1/telemetry/events/recent
   *
   * @param filters - Event type, tenant, user, and time filters
   * @returns Array of telemetry events
   */
  async getTelemetryEvents(filters?: {
    limit?: number;
    tenantId?: string;
    userId?: string;
    startTime?: string;
    endTime?: string;
    eventType?: string;
    eventTypes?: string[];
    level?: string;
  }): Promise<types.TelemetryEvent[]> {
    const params = new URLSearchParams();
    if (filters?.limit) params.append('limit', filters.limit.toString());

    const normalizedEventTypes =
      filters?.eventTypes?.length ? filters.eventTypes : filters?.eventType ? [filters.eventType] : [];
    normalizedEventTypes.forEach((evt) => params.append('event_types[]', evt));

    const queryString = params.toString();
    return this.client.requestList<types.TelemetryEvent>(
      `/v1/telemetry/events/recent${queryString ? `?${queryString}` : ''}`,
    );
  }

  /**
   * Query logs with filters
   *
   * GET /v1/logs/query
   *
   * @param filters - Tenant, event type, level, component, and trace filters
   * @returns Array of unified telemetry events
   */
  async queryLogs(filters?: {
    limit?: number;
    tenant_id?: string;
    event_type?: string;
    level?: string;
    component?: string;
    trace_id?: string;
  }): Promise<types.UnifiedTelemetryEvent[]> {
    const params = new URLSearchParams();
    if (filters?.limit) params.append('limit', filters.limit.toString());
    if (filters?.tenant_id) params.append('tenant_id', filters.tenant_id);
    if (filters?.event_type) params.append('event_type', filters.event_type);
    if (filters?.level) params.append('level', filters.level);
    if (filters?.component) params.append('component', filters.component);
    if (filters?.trace_id) params.append('trace_id', filters.trace_id);

    const queryString = params.toString();
    return this.client.requestList<types.UnifiedTelemetryEvent>(`/v1/logs/query${queryString ? `?${queryString}` : ''}`);
  }

  /**
   * Get current metrics snapshot
   *
   * GET /v1/metrics/snapshot
   *
   * @returns Current system metrics
   */
  async getMetricsSnapshot(): Promise<types.MetricsSnapshotResponse> {
    return this.client.request<types.MetricsSnapshotResponse>('/v1/metrics/snapshot');
  }

  /**
   * Get time-series metrics data
   *
   * GET /v1/metrics/series
   *
   * @param params - Series name and time range
   * @returns Array of metric series data points
   */
  async getMetricsSeries(params?: {
    series_name?: string;
    start_ms?: number;
    end_ms?: number;
  }): Promise<types.MetricsSeriesResponse[]> {
    const queryParams = new URLSearchParams();
    if (params?.series_name) queryParams.append('series_name', params.series_name);
    if (params?.start_ms) queryParams.append('start_ms', params.start_ms.toString());
    if (params?.end_ms) queryParams.append('end_ms', params.end_ms.toString());

    const queryString = queryParams.toString();
    return this.client.requestList<types.MetricsSeriesResponse>(`/v1/metrics/series${queryString ? `?${queryString}` : ''}`);
  }

  /**
   * Search traces with filters
   *
   * GET /v1/traces/search
   *
   * @param params - Span name, status, and time filters
   * @returns Array of trace IDs
   */
  async searchTraces(params?: {
    span_name?: string;
    status?: string;
    start_time_ns?: number;
    end_time_ns?: number;
  }): Promise<string[]> {
    const queryParams = new URLSearchParams();
    if (params?.span_name) queryParams.append('span_name', params.span_name);
    if (params?.status) queryParams.append('status', params.status);
    if (params?.start_time_ns) queryParams.append('start_time_ns', params.start_time_ns.toString());
    if (params?.end_time_ns) queryParams.append('end_time_ns', params.end_time_ns.toString());

    const queryString = queryParams.toString();
    return this.client.requestList<string>(`/v1/traces/search${queryString ? `?${queryString}` : ''}`);
  }

  /**
   * Get a specific trace by ID
   *
   * GET /v1/traces/:traceId
   *
   * @param traceId - Trace ID
   * @param tenantId - Optional tenant ID filter
   * @returns Trace data or null if not found
   */
  async getTrace(traceId: string, tenantId?: string): Promise<types.TraceResponseV1 | types.Trace | null> {
    const query = tenantId ? `?tenant_id=${encodeURIComponent(tenantId)}` : '';
    return this.client.request<types.TraceResponseV1 | types.Trace | null>(`/v1/traces/${traceId}${query}`);
  }

  /**
   * Export audit logs
   *
   * GET /v1/audits/export
   *
   * @param params - Format, time range, tenant, event type, and level filters
   * @returns Blob containing exported audit logs
   */
  async exportAuditLogs(params?: {
    format?: 'csv' | 'json';
    startTime?: string;
    endTime?: string;
    tenantId?: string;
    eventType?: string;
    level?: string;
  }): Promise<Blob> {
    const queryParams = new URLSearchParams();
    if (params?.format) queryParams.append('format', params.format);
    if (params?.startTime) queryParams.append('start_time', params.startTime);
    if (params?.endTime) queryParams.append('end_time', params.endTime);
    if (params?.tenantId) queryParams.append('tenant_id', params.tenantId);
    if (params?.eventType) queryParams.append('event_type', params.eventType);
    if (params?.level) queryParams.append('level', params.level);

    const queryString = queryParams.toString();
    const path = `/v1/audits/export${queryString ? `?${queryString}` : ''}`;
    const url = this.client.buildUrl(path);
    const token = this.client.getToken();

    const response = await fetch(url, {
      headers: token ? { Authorization: `Bearer ${token}` } : undefined,
      credentials: 'omit',
    });

    return handleBlobResponse(response, { method: 'GET', path });
  }

  /**
   * Get compliance audit report
   * Returns compliance controls and policy violations from policy_quarantine table
   *
   * GET /v1/compliance/audit
   *
   * @returns Compliance audit data
   */
  async getComplianceAudit(): Promise<types.ComplianceAuditResponse> {
    return this.client.request<types.ComplianceAuditResponse>('/v1/compliance/audit');
  }

  // ============================================================================
  // Golden Baselines and Contacts
  // ============================================================================

  /**
   * List contacts for a tenant
   *
   * GET /v1/contacts
   *
   * @param tenantId - Tenant ID
   * @returns Array of contacts
   */
  async listContacts(tenantId: string): Promise<types.Contact[]> {
    const params = new URLSearchParams({ tenant_id: tenantId });
    return this.client.requestList<types.Contact>(`/v1/contacts?${params.toString()}`);
  }

  /**
   * List golden runs (baseline inference runs)
   *
   * GET /v1/golden/runs
   *
   * @returns Array of golden run names
   */
  async listGoldenRuns(): Promise<string[]> {
    return this.client.requestList<string>('/v1/golden/runs');
  }

  /**
   * Get details for a golden run
   *
   * GET /v1/golden/runs/:name
   *
   * @param name - Golden run name
   * @returns Golden run summary
   */
  async getGoldenRun(name: string): Promise<types.GoldenRunSummary> {
    return this.client.request<types.GoldenRunSummary>(`/v1/golden/runs/${encodeURIComponent(name)}`);
  }

  /**
   * Compare two golden runs
   *
   * POST /v1/golden/compare
   *
   * @param runA - First run name
   * @param runB - Second run name
   * @returns Comparison results
   */
  async compareGoldenRuns(runA: string, runB: string): Promise<types.GoldenCompareResult> {
    return this.client.request<types.GoldenCompareResult>('/v1/golden/compare', {
      method: 'POST',
      body: JSON.stringify({ run_a: runA, run_b: runB }),
    });
  }

  /**
   * Compare golden runs with detailed request
   *
   * POST /v1/golden/compare
   *
   * @param req - Golden compare request
   * @returns Verification report
   */
  async goldenCompare(req: types.GoldenCompareRequest): Promise<types.VerificationReport> {
    return this.client.request<types.VerificationReport>('/v1/golden/compare', {
      method: 'POST',
      body: JSON.stringify(req),
    });
  }

  /**
   * Request promotion of a golden run to a target stage
   *
   * POST /v1/golden/:runId/promote
   *
   * @param runId - Golden run ID
   * @param targetStage - Target stage (dev, staging, production)
   * @returns Promotion response
   */
  async requestGoldenPromotion(runId: string, targetStage: string): Promise<types.PromotionResponse> {
    return this.client.request<types.PromotionResponse>(`/v1/golden/${encodeURIComponent(runId)}/promote`, {
      method: 'POST',
      body: JSON.stringify({ target_stage: targetStage }),
    });
  }

  /**
   * Get promotion status for a golden run
   *
   * GET /v1/golden/:runId/promotion
   *
   * @param runId - Golden run ID
   * @returns Promotion status
   */
  async getGoldenPromotionStatus(runId: string): Promise<types.PromotionStatusResponse> {
    return this.client.request<types.PromotionStatusResponse>(`/v1/golden/${encodeURIComponent(runId)}/promotion`);
  }

  /**
   * Approve a golden run promotion
   *
   * POST /v1/golden/:runId/approve
   *
   * @param runId - Golden run ID
   * @param stageId - Stage ID
   * @param notes - Approval notes
   * @returns Approval response
   */
  async approveGoldenPromotion(runId: string, stageId: string, notes: string): Promise<types.ApproveResponse> {
    return this.client.request<types.ApproveResponse>(`/v1/golden/${encodeURIComponent(runId)}/approve`, {
      method: 'POST',
      body: JSON.stringify({ stage_id: stageId, approved: true, notes }),
    });
  }

  /**
   * Reject a golden run promotion
   *
   * POST /v1/golden/:runId/approve
   *
   * @param runId - Golden run ID
   * @param stageId - Stage ID
   * @param notes - Rejection notes
   * @returns Approval response
   */
  async rejectGoldenPromotion(runId: string, stageId: string, notes: string): Promise<types.ApproveResponse> {
    return this.client.request<types.ApproveResponse>(`/v1/golden/${encodeURIComponent(runId)}/approve`, {
      method: 'POST',
      body: JSON.stringify({ stage_id: stageId, approved: false, notes }),
    });
  }

  /**
   * Get gate status for a golden run
   *
   * GET /v1/golden/:runId/gates
   *
   * @param runId - Golden run ID
   * @returns Array of gate statuses
   */
  async getGoldenGateStatus(runId: string): Promise<types.GateStatus[]> {
    return this.client.requestList<types.GateStatus>(`/v1/golden/${encodeURIComponent(runId)}/gates`);
  }

  /**
   * Rollback a golden promotion for a stage
   *
   * POST /v1/golden/:stage/rollback
   *
   * @param stage - Stage name to rollback
   * @returns Rollback response
   */
  async rollbackGoldenPromotion(stage: string): Promise<types.RollbackResponse> {
    return this.client.request<types.RollbackResponse>(`/v1/golden/${encodeURIComponent(stage)}/rollback`, {
      method: 'POST',
    });
  }
}
