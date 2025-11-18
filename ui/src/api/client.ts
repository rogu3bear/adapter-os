// API Client for AdapterOS Control Plane
//! 
//! Provides centralized API communication with structured logging and error handling.
//! 
//! # Citations
//! - CONTRIBUTING.md L123: "Use `tracing` for logging (not `println!`)"
//! - Policy Pack #9 (Telemetry): "MUST log events with canonical JSON"
//! - Policy Pack #1 (Egress): "MUST NOT open listening TCP ports; use Unix domain sockets only"

import * as types from './types';
import { logger } from '../utils/logger';

const API_BASE_URL = (import.meta as { env?: { VITE_API_URL?: string } }).env?.VITE_API_URL || '/api';

class ApiClient {
  private baseUrl: string;
  private token: string | null = null;
  private requestLog: Array<{ id: string; method: string; path: string; timestamp: string }> = [];

  constructor(baseUrl: string = API_BASE_URL, retryConfig?: Partial<RetryConfig>) {
    this.baseUrl = baseUrl;
    // Replace: console.log('API Client initialized with base URL:', this.baseUrl);
    logger.info('API Client initialized', { 
      component: 'ApiClient',
      operation: 'constructor',
      baseUrl: this.baseUrl 
    });
    // Load token from localStorage
    this.token = localStorage.getItem('aos_token');
  }

  private async computeRequestId(method: string, path: string, body: string): Promise<string> {
    const canonical = `${method}:${path}:${body}`;
    const encoder = new TextEncoder();
    const data = encoder.encode(canonical);
    const hashBuffer = await crypto.subtle.digest('SHA-256', data);
    const hashArray = Array.from(new Uint8Array(hashBuffer));
    return hashArray.map(b => b.toString(16).padStart(2, '0')).join('').substring(0, 32);
  }

  private logRequest(id: string, method: string, path: string) {
    this.requestLog.push({
      id,
      method,
      path,
      timestamp: new Date().toISOString(),
    });
    // Keep last 1000 requests
    if (this.requestLog.length > 1000) {
      this.requestLog.shift();
    }
  }

  public getRequestLog() {
    return this.requestLog;
  }

  private async computeRequestId(method: string, path: string, body: string): Promise<string> {
    const canonical = `${method}:${path}:${body}`;
    const encoder = new TextEncoder();
    const data = encoder.encode(canonical);
    const hashBuffer = await crypto.subtle.digest('SHA-256', data);
    const hashArray = Array.from(new Uint8Array(hashBuffer));
    return hashArray.map(b => b.toString(16).padStart(2, '0')).join('').substring(0, 32);
  }

  private logRequest(id: string, method: string, path: string) {
    this.requestLog.push({
      id,
      method,
      path,
      timestamp: new Date().toISOString(),
    });
    // Keep last 1000 requests
    if (this.requestLog.length > 1000) {
      this.requestLog.shift();
    }
  }

  public getRequestLog() {
    return this.requestLog;
  }

  private async request<T>(
    path: string,
    options: RequestInit = {},
    skipRetry: boolean = false,
    cancelToken?: AbortSignal,
    allowMutationRetry: boolean = false
  ): Promise<T> {
    const method = options.method || 'GET';

    // Configure retry based on HTTP method and explicit permission
    // GET requests are safe to retry, mutations need explicit permission
    const shouldRetry = !skipRetry && (method === 'GET' || method === 'HEAD' || allowMutationRetry);
    const operationConfig = shouldRetry ? this.retryConfig : {
      ...this.retryConfig,
      maxAttempts: 1 // No retry for mutations unless explicitly enabled
    };

    const operation = async (): Promise<T> => {
      return this.executeRequest(path, options, cancelToken);
    };

    const result = await retryWithBackoff(operation, operationConfig, (attempt, error, delay) => {
      logger.info('Retrying API request', {
        component: 'ApiClient',
        operation: 'request',
        method,
        path,
        attempt,
        delay
      });
    }, `${method} ${path}`);

    if (result.success) {
      return result.value;
    } else {
      throw (result as any).error;
    }
  }

  private async executeRequest<T>(path: string, options: RequestInit = {}, cancelToken?: AbortSignal): Promise<T> {
    const url = `${this.baseUrl}${path}`;
    
    // Compute deterministic request ID
    const method = options.method || 'GET';
    const body = options.body || '';
    const requestId = await this.computeRequestId(method, path, body.toString());
    
    const headers: HeadersInit = {
      'Content-Type': 'application/json',
      'X-Request-ID': requestId,
      ...options.headers,
    };

    // Store in local audit buffer
    this.logRequest(requestId, method, path);

    let response: Response;
    try {
      response = await fetch(url, {
        ...options,
        headers,
        credentials: 'include', // Send httpOnly cookies
        signal: cancelToken, // Add cancellation support
      });
    } catch (networkError) {
      // Network error (connection failure, timeout, etc.)
      const error = toError(networkError);
      logger.error('API request network error', {
        component: 'ApiClient',
        operation: 'executeRequest',
        method,
        path,
        requestId,
      }, error);
      throw error;
    }
    
    // Validate returned request ID matches
    const returnedId = response.headers.get('X-Request-ID');
    if (returnedId && returnedId !== requestId) {
      logger.warn('Request ID mismatch', {
        component: 'ApiClient',
        operation: 'request_validation',
        sent: requestId,
        received: returnedId
      });
    }

    return result;
  }

  async loadAdapter(adapterId: string): Promise<types.AdapterResponse> {
    return this.request<types.AdapterResponse>(`/v1/adapters/${adapterId}/load`, {
      method: 'POST',
    });
  }

  async unloadAdapter(adapterId: string): Promise<void> {
    return this.request<void>(`/v1/adapters/${adapterId}/unload`, {
      method: 'POST',
    });
  }

  // Training endpoints
  async listTrainingJobs(): Promise<types.TrainingJob[]> {
    return this.request<types.TrainingJob[]>('/v1/training/jobs');
  }

  async getTrainingJob(jobId: string): Promise<types.TrainingJob> {
    return this.request<types.TrainingJob>(`/v1/training/jobs/${jobId}`);
  }

  async getTrainingArtifacts(jobId: string): Promise<types.TrainingArtifactsResponse> {
    return this.request<types.TrainingArtifactsResponse>(`/v1/training/jobs/${jobId}/artifacts`);
  }

  async startTraining(request: types.StartTrainingRequest): Promise<types.TrainingJob> {
    return this.request<types.TrainingJob>('/v1/training/start', {
      method: 'POST',
      body: JSON.stringify(request),
    });
  }

  async cancelTraining(jobId: string): Promise<void> {
    return this.request<void>(`/v1/training/jobs/${jobId}/cancel`, {
      method: 'POST',
    });
  }

  async getTrainingLogs(jobId: string): Promise<string[]> {
    return this.request<string[]>(`/v1/training/jobs/${jobId}/logs`);
  }

  async getTrainingMetrics(jobId: string): Promise<types.TrainingMetrics> {
    return this.request<types.TrainingMetrics>(`/v1/training/jobs/${jobId}/metrics`);
  }

  async listTrainingTemplates(): Promise<types.TrainingTemplate[]> {
    return this.request<types.TrainingTemplate[]>('/v1/training/templates');
  }

  async getTrainingTemplate(templateId: string): Promise<types.TrainingTemplate> {
    return this.request<types.TrainingTemplate>(`/v1/training/templates/${templateId}`);
  }

  // Adapter lifecycle management
  // Supports both boolean and advanced pinning modes
  async pinAdapter(adapterId: string, pinnedOrTtlHours: boolean | number, reason?: string): Promise<void> {
    // If boolean, use simple pin/unpin API
    if (typeof pinnedOrTtlHours === 'boolean') {
      if (pinnedOrTtlHours) {
        return this.request<void>(`/v1/adapters/${adapterId}/pin`, {
          method: 'POST',
          body: JSON.stringify({}),
        });
      } else {
        return this.unpinAdapter(adapterId);
      }
    }
    // Otherwise use advanced API with TTL
    return this.request<void>(`/v1/adapters/${adapterId}/pin`, {
      method: 'POST',
      body: JSON.stringify({ ttl_hours: pinnedOrTtlHours, reason }),
    });
  }

  async unpinAdapter(adapterId: string): Promise<void> {
    return this.request<void>(`/v1/adapters/${adapterId}/unpin`, {
      method: 'POST',
    });
  }

  async swapAdapters(add: string[], remove: string[], commit: boolean = false): Promise<void> {
    return this.request<void>('/v1/adapters/swap', {
      method: 'POST',
      body: JSON.stringify({ add, remove, commit }),
    });
  }

  async getAdapterStats(adapterId: string): Promise<types.AdapterStats> {
    return this.request<types.AdapterStats>(`/v1/adapters/${adapterId}/stats`);
  }

  async getAdapterActivations(adapterId: string): Promise<types.AdapterActivation[]> {
    return this.request<types.AdapterActivation[]>(`/v1/adapters/${adapterId}/activations`);
  }

  async promoteAdapterState(adapterId: string, options: RequestInit = {}, skipRetry: boolean = false, cancelToken?: AbortSignal): Promise<types.AdapterStateResponse> {
    return this.request<types.AdapterStateResponse>(`/v1/adapters/${adapterId}/promote`, {
      method: 'POST',
      ...options,
    }, skipRetry, cancelToken);
  }

  async updateAdapterPolicy(adapterId: string, req: types.UpdateAdapterPolicyRequest): Promise<types.UpdateAdapterPolicyResponse> {
    return this.request<types.UpdateAdapterPolicyResponse>(`/v1/adapters/${adapterId}/policy`, {
      method: 'PUT',
      body: JSON.stringify(req),
    });
  }

  async downloadAdapterManifest(adapterId: string): Promise<types.AdapterManifest> {
    return this.request<types.AdapterManifest>(`/v1/adapters/${adapterId}/manifest`);
  }

  async getAdapterHealth(adapterId: string): Promise<types.AdapterHealthResponse> {
    return this.request<types.AdapterHealthResponse>(`/v1/adapters/${adapterId}/health`);
  }

  // Category Policies
  async getCategoryPolicies(): Promise<Record<types.AdapterCategory, types.CategoryPolicy>> {
    return this.request<Record<types.AdapterCategory, types.CategoryPolicy>>('/v1/adapters/category-policies');
  }

  async getCategoryPolicy(category: types.AdapterCategory): Promise<types.CategoryPolicy> {
    return this.request<types.CategoryPolicy>(`/v1/adapters/category-policies/${category}`);
  }

  async updateCategoryPolicy(category: types.AdapterCategory, policy: types.CategoryPolicy): Promise<types.CategoryPolicy> {
    return this.request<types.CategoryPolicy>(`/v1/adapters/category-policies/${category}`, {
      method: 'PUT',
      body: JSON.stringify(policy),
    });
  }

  // Repositories
  async listRepositories(): Promise<types.Repository[]> {
    return this.request<types.Repository[]>('/v1/repositories');
  }

  async registerRepository(data: types.RegisterRepositoryRequest): Promise<types.Repository> {
    return this.request<types.Repository>('/v1/repositories/register', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async triggerRepositoryScan(repoId: string): Promise<void> {
    return this.request(`/v1/repositories/${repoId}/scan`, {
      method: 'POST',
    });
  }

  async getRepositoryStatus(repoId: string): Promise<types.ScanStatusResponse> {
    return this.request<types.ScanStatusResponse>(
      `/v1/repositories/${repoId}/status`
    );
  }

  // Commits
  async listCommits(repoId?: string): Promise<types.Commit[]> {
    const query = repoId ? `?repo_id=${repoId}` : '';
    return this.request<types.Commit[]>(`/v1/commits${query}`);
  }

  async getCommit(sha: string): Promise<types.Commit> {
    return this.request<types.Commit>(`/v1/commits/${sha}`);
  }

  async getCommitDiff(sha: string): Promise<types.CommitDiff> {
    return this.request<types.CommitDiff>(`/v1/commits/${sha}/diff`);
  }

  // Metrics
  async getSystemMetrics(): Promise<types.SystemMetrics> {
    return this.request<types.SystemMetrics>('/v1/metrics/system');
  }

  async getQualityMetrics(): Promise<types.QualityMetrics> {
    return this.request<types.QualityMetrics>('/v1/metrics/quality');
  }

  async getAdapterMetrics(): Promise<types.AdapterMetrics[]> {
    return this.request<types.AdapterMetrics[]>('/v1/metrics/adapters');
  }

  // Base Model Status
  async getBaseModelStatus(tenantId?: string): Promise<types.BaseModelStatus> {
    const query = tenantId ? `?tenant_id=${tenantId}` : '';
    return this.request<types.BaseModelStatus>(`/v1/models/status${query}`);
  }

  // Routing
  async debugRouting(data: types.RoutingDebugRequest): Promise<types.RoutingDebugResponse> {
    return this.request<types.RoutingDebugResponse>('/v1/routing/debug', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async getRoutingHistory(limit?: number): Promise<types.RoutingDecision[]> {
    const query = limit ? `?limit=${limit}` : '';
    return this.request<types.RoutingDecision[]>(`/v1/routing/history${query}`);
  }

  // Inference
  async infer(data: types.InferRequest, options: RequestInit = {}, skipRetry: boolean = false, cancelToken?: AbortSignal): Promise<types.InferResponse> {
    return this.request<types.InferResponse>('/v1/infer', {
      method: 'POST',
      body: JSON.stringify(data),
      ...options,
    }, skipRetry, cancelToken);
  }

  // ===== Phase 6: Policy Operations =====
  async signPolicy(cpid: string): Promise<types.SignPolicyResponse> {
    return this.request<types.SignPolicyResponse>(`/v1/policies/${cpid}/sign`, {
      method: 'POST',
    });
  }

  async comparePolicies(cpid1: string, cpid2: string): Promise<types.PolicyComparisonResponse> {
    return this.request<types.PolicyComparisonResponse>('/v1/policies/compare', {
      method: 'POST',
      body: JSON.stringify({ cpid_1: cpid1, cpid_2: cpid2 }),
    });
  }

  async exportPolicy(cpid: string): Promise<types.ExportPolicyResponse> {
    return this.request<types.ExportPolicyResponse>(`/v1/policies/${cpid}/export`);
  }

  // ===== Phase 7: Promotion Execution =====
  async dryRunPromotion(cpid: string): Promise<types.DryRunPromotionResponse> {
    return this.request<types.DryRunPromotionResponse>('/v1/cp/promote/dry-run', {
      method: 'POST',
      body: JSON.stringify({ cpid }),
    });
  }

  async getPromotionHistory(): Promise<types.PromotionHistoryEntry[]> {
    return this.request<types.PromotionHistoryEntry[]>('/v1/cp/promotions');
  }

  // ===== Phase 8: Telemetry Operations =====
  // (duplicate method removed; see canonical definition above returning TelemetryBundle[])
  async exportTelemetryBundle(bundleId: string): Promise<types.ExportTelemetryBundleResponse> {
    return this.request<types.ExportTelemetryBundleResponse>(`/v1/telemetry/bundles/${bundleId}/export`);
  }

  async generateTelemetryBundle(): Promise<{ id: string; cpid: string; event_count: number; size_bytes: number; created_at: string }> {
    return this.request('/v1/telemetry/bundles/generate', { method: 'POST' });
  }

  async verifyBundleSignature(bundleId: string): Promise<types.VerifyBundleSignatureResponse> {
    return this.request<types.VerifyBundleSignatureResponse>(`/v1/telemetry/bundles/${bundleId}/verify`, {
      method: 'POST',
    });
  }

  async purgeOldBundles(keepCount: number): Promise<types.PurgeOldBundlesResponse> {
    return this.request<types.PurgeOldBundlesResponse>('/v1/telemetry/bundles/purge', {
      method: 'POST',
      body: JSON.stringify({ keep_bundles_per_cpid: keepCount }),
    });
  }

  // ===== Phase 9: Code Intelligence =====
  async getRepositoryReport(repoId: string): Promise<types.RepositoryReportResponse> {
    return this.request<types.RepositoryReportResponse>(`/v1/repositories/${repoId}/report`);
  }

  async unregisterRepository(repoId: string): Promise<void> {
    return this.request<void>(`/v1/repositories/${repoId}`, {
      method: 'DELETE',
    });
  }

  // ===== Phase 10: Tenant Management =====
  async updateTenant(tenantId: string, name: string): Promise<types.TenantResponse> {
    return this.request<types.TenantResponse>(`/v1/tenants/${tenantId}`, {
      method: 'PUT',
      body: JSON.stringify({ name }),
    });
  }

  async pauseTenant(tenantId: string): Promise<void> {
    return this.request<void>(`/v1/tenants/${tenantId}/pause`, {
      method: 'POST',
    });
  }

  async archiveTenant(tenantId: string): Promise<void> {
    return this.request<void>(`/v1/tenants/${tenantId}/archive`, {
      method: 'POST',
    });
  }

  async assignTenantPolicies(tenantId: string, cpids: string[]): Promise<types.AssignPoliciesResponse> {
    return this.request<types.AssignPoliciesResponse>(`/v1/tenants/${tenantId}/policies`, {
      method: 'POST',
      body: JSON.stringify({ cpids }),
    });
  }

  async assignTenantAdapters(tenantId: string, adapterIds: string[]): Promise<types.AssignAdaptersResponse> {
    return this.request<types.AssignAdaptersResponse>(`/v1/tenants/${tenantId}/adapters`, {
      method: 'POST',
      body: JSON.stringify({ adapter_ids: adapterIds }),
    });
  }

  async getTenantUsage(tenantId: string): Promise<types.TenantUsageResponse> {
    return this.request<types.TenantUsageResponse>(`/v1/tenants/${tenantId}/usage`);
  }

  // Git Repository API
  async registerGitRepository(request: types.RegisterGitRepositoryRequest): Promise<types.RegisterGitRepositoryResponse> {
    return this.request<types.RegisterGitRepositoryResponse>(`/v1/git/repositories`, {
      method: 'POST',
      body: JSON.stringify(request),
    });
  }

  async getRepositoryAnalysis(repoId: string): Promise<any> {
    return this.request(`/v1/git/repositories/${repoId}/analysis`);
  }

  async trainRepositoryAdapter(repoId: string, config: any): Promise<{
    training_id: string;
    status: string;
    estimated_duration: string;
    evidence_count: number;
  }> {
    return this.request(`/v1/git/repositories/${repoId}/train`, {
      method: 'POST',
      body: JSON.stringify({ config }),
    });
  }

  // Domain Adapter API
  async listDomainAdapters(): Promise<types.DomainAdapter[]> {
    return this.request<types.DomainAdapter[]>('/v1/domain-adapters');
  }

  async getDomainAdapter(adapterId: string): Promise<types.DomainAdapter> {
    return this.request<types.DomainAdapter>(`/v1/domain-adapters/${adapterId}`);
  }

  async createDomainAdapter(data: types.CreateDomainAdapterRequest): Promise<types.DomainAdapter> {
    return this.request<types.DomainAdapter>('/v1/domain-adapters', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async loadDomainAdapter(adapterId: string, config?: Record<string, any>): Promise<types.DomainAdapter> {
    return this.request<types.DomainAdapter>(`/v1/domain-adapters/${adapterId}/load`, {
      method: 'POST',
      body: JSON.stringify({ adapter_id: adapterId, executor_config: config }),
    });
  }

  async unloadDomainAdapter(adapterId: string): Promise<types.DomainAdapter> {
    return this.request<types.DomainAdapter>(`/v1/domain-adapters/${adapterId}/unload`, {
      method: 'POST',
    });
  }

  async testDomainAdapter(adapterId: string, inputData: string, expectedOutput?: string, iterations?: number): Promise<types.TestDomainAdapterResponse> {
    return this.request<types.TestDomainAdapterResponse>(`/v1/domain-adapters/${adapterId}/test`, {
      method: 'POST',
      body: JSON.stringify({
        adapter_id: adapterId,
        input_data: inputData,
        expected_output: expectedOutput,
        iterations: iterations || 100,
      }),
    });
  }

  async getDomainAdapterManifest(adapterId: string): Promise<types.DomainAdapterManifest> {
    return this.request<types.DomainAdapterManifest>(`/v1/domain-adapters/${adapterId}/manifest`);
  }

  async executeDomainAdapter(adapterId: string, inputData: any): Promise<types.DomainAdapterExecutionResponse> {
    return this.request<types.DomainAdapterExecutionResponse>(`/v1/domain-adapters/${adapterId}/execute`, {
      method: 'POST',
      body: JSON.stringify(inputData),
    });
  }

  async deleteDomainAdapter(adapterId: string): Promise<void> {
    return this.request<void>(`/v1/domain-adapters/${adapterId}`, {
      method: 'DELETE',
    });
  }

  // Monitoring API
  async listMonitoringRules(tenantId?: string): Promise<types.MonitoringRule[]> {
    const query = tenantId ? `?tenant_id=${tenantId}` : '';
    return this.request<types.MonitoringRule[]>(`/v1/monitoring/rules${query}`);
  }

  async createMonitoringRule(data: types.CreateMonitoringRuleRequest): Promise<types.MonitoringRule> {
    return this.request<types.MonitoringRule>('/v1/monitoring/rules', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async deleteMonitoringRule(ruleId: string): Promise<void> {
    return this.request<void>(`/v1/monitoring/rules/${ruleId}`, {
      method: 'DELETE',
    });
  }

  async listAlerts(filters?: types.AlertFilters): Promise<types.Alert[]> {
    const params = new URLSearchParams();
    if (filters?.tenant_id) params.append('tenant_id', filters.tenant_id);
    if (filters?.worker_id) params.append('worker_id', filters.worker_id);
    if (filters?.status) params.append('status', filters.status);
    if (filters?.severity) params.append('severity', filters.severity);
    if (filters?.limit) params.append('limit', filters.limit.toString());
    const query = params.toString() ? `?${params.toString()}` : '';
    return this.request<types.Alert[]>(`/v1/monitoring/alerts${query}`);
  }

  async acknowledgeAlert(alertId: string, data: types.AcknowledgeAlertRequest): Promise<types.Alert> {
    return this.request<types.Alert>(`/v1/monitoring/alerts/${alertId}/acknowledge`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async listHealthMetrics(tenantId?: string): Promise<types.HealthMetric[]> {
    const query = tenantId ? `?tenant_id=${tenantId}` : '';
    return this.request<types.HealthMetric[]>(`/v1/monitoring/health-metrics${query}`);
  }

  // Replay API
  async listReplaySessions(tenantId?: string): Promise<types.ReplaySession[]> {
    const query = tenantId ? `?tenant_id=${tenantId}` : '';
    return this.request<types.ReplaySession[]>(`/v1/replay/sessions${query}`);
  }

  async getReplaySession(sessionId: string): Promise<types.ReplaySession> {
    return this.request<types.ReplaySession>(`/v1/replay/sessions/${sessionId}`);
  }

  async createReplaySession(data: types.CreateReplaySessionRequest): Promise<types.ReplaySession> {
    return this.request<types.ReplaySession>('/v1/replay/sessions', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async verifyReplaySession(sessionId: string): Promise<types.ReplayVerificationResponse> {
    return this.request<types.ReplayVerificationResponse>(`/v1/replay/sessions/${sessionId}/verify`, {
      method: 'POST',
    });
  }

  // Memory management methods
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
    return this.request('/v1/memory/usage');
  }

  async evictAdapter(adapterId: string): Promise<{ success: boolean; message: string }> {
    return this.request(`/v1/memory/adapters/${adapterId}/evict`, {
      method: 'POST',
    });
  }

  // Note: pinAdapter method is consolidated above in Adapter lifecycle management section

  // Training methods
  async startAdapterTraining(data: {
    repository_path: string;
    adapter_name: string;
    description: string;
    training_config: Record<string, string | number | boolean>;
    tenant_id: string;
  }): Promise<{ session_id: string; status: string; created_at: string }> {
    return this.request<{ session_id: string; status: string; created_at: string }>('/v1/training/sessions', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

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
    return this.request(`/v1/training/sessions/${sessionId}`);
  }

  async listTrainingSessions(tenantId?: string): Promise<Array<{
    session_id: string;
    status: string;
    adapter_name: string;
    repository_path: string;
    created_at: string;
    updated_at: string;
  }>> {
    const params = new URLSearchParams();
    if (tenantId) params.append('tenant_id', tenantId);
    
    const queryString = params.toString();
    return this.request(`/v1/training/sessions${queryString ? `?${queryString}` : ''}`);
  }

  // Telemetry methods
  async getTelemetryEvents(filters?: {
    limit?: number;
    tenantId?: string;
    userId?: string;
    startTime?: string;
    endTime?: string;
    eventType?: string;
    level?: string;
  }): Promise<types.TelemetryEvent[]> {
    const params = new URLSearchParams();
    if (filters?.limit) params.append('limit', filters.limit.toString());
    if (filters?.tenantId) params.append('tenant_id', filters.tenantId);
    if (filters?.userId) params.append('user_id', filters.userId);
    if (filters?.startTime) params.append('start_time', filters.startTime);
    if (filters?.endTime) params.append('end_time', filters.endTime);
    if (filters?.eventType) params.append('event_type', filters.eventType);
    if (filters?.level) params.append('level', filters.level);

    const queryString = params.toString();
    return this.request<types.TelemetryEvent[]>(`/v1/telemetry/events${queryString ? `?${queryString}` : ''}`);
  }

  // Process debugging methods
  async getProcessLogs(workerId: string, filters?: types.ProcessLogFilters): Promise<types.ProcessLog[]> {
    const params = new URLSearchParams();
    if (filters?.level) params.append('level', filters.level);
    if (filters?.limit) params.append('limit', filters.limit.toString());
    if (filters?.start_time) params.append('start_time', filters.start_time);
    if (filters?.end_time) params.append('end_time', filters.end_time);
    const query = params.toString() ? `?${params.toString()}` : '';
    return this.request<types.ProcessLog[]>(`/v1/workers/${workerId}/logs${query}`);
  }

  async getProcessCrashes(workerId: string): Promise<types.ProcessCrash[]> {
    return this.request<types.ProcessCrash[]>(`/v1/workers/${workerId}/crashes`);
  }

  async startDebugSession(workerId: string, config: types.DebugSessionConfig): Promise<types.DebugSession> {
    return this.request<types.DebugSession>(`/v1/workers/${workerId}/debug`, {
      method: 'POST',
      body: JSON.stringify(config),
    });
  }

  async runTroubleshootingStep(workerId: string, step: types.TroubleshootingStep): Promise<types.TroubleshootingResult> {
    return this.request<types.TroubleshootingResult>(`/v1/workers/${workerId}/troubleshoot`, {
      method: 'POST',
      body: JSON.stringify(step),
    });
  }

  // Node management
  async listNodes(): Promise<types.Node[]> {
    return this.request<types.Node[]>('/v1/nodes');
  }

  async registerNode(data: types.RegisterNodeRequest): Promise<types.Node> {
    return this.request<types.Node>('/v1/nodes/register', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async testNodeConnection(nodeId: string): Promise<types.NodePingResponse> {
    return this.request<types.NodePingResponse>(`/v1/nodes/${nodeId}/ping`, {
      method: 'POST',
    });
  }

  async markNodeOffline(nodeId: string): Promise<void> {
    return this.request<void>(`/v1/nodes/${nodeId}/offline`, {
      method: 'POST',
    });
  }

  async evictNode(nodeId: string): Promise<void> {
    return this.request<void>(`/v1/nodes/${nodeId}`, {
      method: 'DELETE',
    });
  }

  async getNodeDetails(nodeId: string): Promise<types.NodeDetailsResponse> {
    return this.request<types.NodeDetailsResponse>(`/v1/nodes/${nodeId}/details`);
  }

  // Cluster operations
  async getClusterStatus(): Promise<types.ClusterStatus> {
    const [nodes, workers] = await Promise.all([
      this.listNodes(),
      this.listWorkers()
    ]);

    const healthyNodes = nodes.filter(n => n.status === 'active').length;
    const offlineNodes = nodes.filter(n => n.status === 'offline').length;
    const maintenanceNodes = nodes.filter(n => n.status === 'maintenance').length;
    const activeWorkers = workers.filter(w => w.status === 'serving' || w.status === 'starting').length;

    return {
      cluster_id: 'default',
      total_nodes: nodes.length,
      healthy_nodes: healthyNodes,
      offline_nodes: offlineNodes,
      maintenance_nodes: maintenanceNodes,
      total_workers: workers.length,
      active_workers: activeWorkers,
      aggregate_memory_gb: nodes.reduce((sum, n) => sum + (n.memory_gb || 0), 0),
      aggregate_gpu_count: nodes.reduce((sum, n) => sum + 1, 0),
    };
  }

  async getNodesWithMetrics(): Promise<types.NodeWithMetrics[]> {
    const [nodes, allWorkers] = await Promise.all([
      this.listNodes(),
      this.listWorkers()
    ]);

    return Promise.all(nodes.map(async (node) => {
      const workers = allWorkers.filter(w => w.node_id === node.id);
      return {
        ...node,
        workers,
        cpu_utilization: undefined,
        memory_utilization: undefined,
        gpu_utilization: undefined,
        uptime_seconds: undefined,
      };
    }));
  }

  // Worker management
  async listWorkers(tenantId?: string, nodeId?: string): Promise<types.WorkerResponse[]> {
    const params = new URLSearchParams();
    if (tenantId) params.append('tenant_id', tenantId);
    if (nodeId) params.append('node_id', nodeId);
    const query = params.toString() ? `?${params.toString()}` : '';
    return this.request<types.WorkerResponse[]>(`/v1/workers${query}`);
  }

  // Routing methods
  async getRoutingDecisions(filters?: types.RoutingDecisionFilters): Promise<types.RoutingDecision[]> {
    const params = new URLSearchParams();
    if (filters?.limit) params.append('limit', filters.limit.toString());
    if (filters?.adapter_id) params.append('adapter_id', filters.adapter_id);
    if (filters?.start_time) params.append('start_time', filters.start_time);
    if (filters?.end_time) params.append('end_time', filters.end_time);
    const query = params.toString() ? `?${params.toString()}` : '';
    return this.request<types.RoutingDecision[]>(`/v1/routing/decisions${query}`);
  }
}

// Export singleton instance
export const apiClient = new ApiClient();
export default apiClient;
