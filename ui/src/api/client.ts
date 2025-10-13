// API Client for AdapterOS Control Plane

import * as types from './types';

const API_BASE_URL = (import.meta as any).env?.VITE_API_URL || 'http://localhost:8080/api';

class ApiClient {
  private baseUrl: string;
  private token: string | null = null;
  private requestLog: Array<{ id: string; method: string; path: string; timestamp: string }> = [];

  constructor(baseUrl: string = API_BASE_URL) {
    this.baseUrl = baseUrl;
    // Load token from localStorage
    this.token = localStorage.getItem('aos_token');
  }

  setToken(token: string | null) {
    this.token = token;
    if (token) {
      localStorage.setItem('aos_token', token);
    } else {
      localStorage.removeItem('aos_token');
    }
  }

  getToken(): string | null {
    return this.token;
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
    options: RequestInit = {}
  ): Promise<T> {
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

    if (this.token) {
      headers['Authorization'] = `Bearer ${this.token}`;
    }

    // Store in local audit buffer
    this.logRequest(requestId, method, path);

    const response = await fetch(url, {
      ...options,
      headers,
    });
    
    // Validate returned request ID matches
    const returnedId = response.headers.get('X-Request-ID');
    if (returnedId && returnedId !== requestId) {
      console.warn('Request ID mismatch:', { sent: requestId, received: returnedId });
    }

    if (!response.ok) {
      const error: types.ErrorResponse = await response.json().catch(() => ({
        error: `HTTP ${response.status}: ${response.statusText}`,
      }));
      throw new Error(error.error || 'Unknown error');
    }

    // Handle 204 No Content
    if (response.status === 204) {
      return {} as T;
    }

    return response.json();
  }

  // Authentication
  async login(credentials: types.LoginRequest): Promise<types.LoginResponse> {
    const response = await this.request<types.LoginResponse>('/v1/auth/login', {
      method: 'POST',
      body: JSON.stringify(credentials),
    });
    this.setToken(response.token);
    return response;
  }

  async logout(): Promise<void> {
    await this.request('/v1/auth/logout', { method: 'POST' });
    this.setToken(null);
  }

  async getCurrentUser(): Promise<types.UserInfoResponse> {
    return this.request<types.UserInfoResponse>('/v1/auth/me');
  }

  // Health
  async health(): Promise<types.HealthResponse> {
    return this.request<types.HealthResponse>('/healthz');
  }

  async ready(): Promise<types.HealthResponse> {
    return this.request<types.HealthResponse>('/readyz');
  }

  async meta(): Promise<types.MetaResponse> {
    return this.request<types.MetaResponse>('/v1/meta');
  }

  // Tenants
  async listTenants(): Promise<types.Tenant[]> {
    return this.request<types.Tenant[]>('/v1/tenants');
  }

  async createTenant(data: types.CreateTenantRequest): Promise<types.Tenant> {
    return this.request<types.Tenant>('/v1/tenants', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  // Nodes
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

  // Workers
  async listWorkers(tenantId?: string, nodeId?: string): Promise<types.WorkerResponse[]> {
    const params = new URLSearchParams();
    if (tenantId) params.append('tenant_id', tenantId);
    if (nodeId) params.append('node_id', nodeId);
    const query = params.toString() ? `?${params.toString()}` : '';
    return this.request<types.WorkerResponse[]>(`/v1/workers${query}`);
  }

  async spawnWorker(request: types.SpawnWorkerRequest): Promise<types.WorkerResponse> {
    return this.request<types.WorkerResponse>('/v1/workers/spawn', {
      method: 'POST',
      body: JSON.stringify(request),
    });
  }

  async stopWorker(workerId: string, force: boolean = false): Promise<void> {
    return this.request<void>(`/v1/workers/${workerId}/stop`, {
      method: 'POST',
      body: JSON.stringify({ force }),
    });
  }

  async getWorkerDetails(workerId: string): Promise<types.WorkerDetailsResponse> {
    return this.request<types.WorkerDetailsResponse>(`/v1/workers/${workerId}/details`);
  }

  // Plans
  async listPlans(): Promise<types.Plan[]> {
    return this.request<types.Plan[]>('/v1/plans');
  }

  async buildPlan(data: types.BuildPlanRequest): Promise<types.Plan> {
    return this.request<types.Plan>('/v1/plans/build', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async rebuildPlan(planId: string): Promise<types.Plan> {
    return this.request<types.Plan>(`/v1/plans/${planId}/rebuild`, {
      method: 'POST',
    });
  }

  async comparePlans(planId1: string, planId2: string): Promise<types.PlanComparisonResponse> {
    return this.request<types.PlanComparisonResponse>('/v1/plans/compare', {
      method: 'POST',
      body: JSON.stringify({ plan_id_1: planId1, plan_id_2: planId2 }),
    });
  }

  async exportPlanManifest(planId: string): Promise<Blob> {
    const url = `${this.baseUrl}/v1/plans/${planId}/manifest`;
    const headers: HeadersInit = {};
    if (this.token) {
      headers['Authorization'] = `Bearer ${this.token}`;
    }
    const response = await fetch(url, { headers });
    if (!response.ok) {
      throw new Error(`Failed to export plan manifest: ${response.statusText}`);
    }
    return response.blob();
  }

  // Control Plane
  async promote(data: types.PromotionRequest): Promise<types.PromotionRecord> {
    return this.request<types.PromotionRecord>('/v1/cp/promote', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async getPromotionGates(cpid: string): Promise<types.PromotionGate[]> {
    return this.request<types.PromotionGate[]>(`/v1/cp/promotion-gates/${cpid}`);
  }

  async rollback(): Promise<void> {
    return this.request('/v1/cp/rollback', { method: 'POST' });
  }

  async getPromotion(id: string): Promise<types.PromotionRecord> {
    return this.request<types.PromotionRecord>(`/v1/promotions/${id}`);
  }

  // Policies
  async listPolicies(): Promise<types.Policy[]> {
    return this.request<types.Policy[]>('/v1/policies');
  }

  async getPolicy(cpid: string): Promise<types.Policy> {
    return this.request<types.Policy>(`/v1/policies/${cpid}`);
  }

  async validatePolicy(data: types.ValidatePolicyRequest): Promise<{ valid: boolean; errors?: string[] }> {
    return this.request('/v1/policies/validate', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async applyPolicy(data: types.ApplyPolicyRequest): Promise<types.PolicyPackResponse> {
    return this.request<types.PolicyPackResponse>('/v1/policies/apply', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async createPolicy(cpid: string, content: string): Promise<types.PolicyPackResponse> {
    return this.request<types.PolicyPackResponse>('/v1/policies/apply', {
      method: 'POST',
      body: JSON.stringify({ cpid, content }),
    });
  }

  async updatePolicy(cpid: string, content: string): Promise<types.PolicyPackResponse> {
    return this.request<types.PolicyPackResponse>('/v1/policies/apply', {
      method: 'POST',
      body: JSON.stringify({ cpid, content }),
    });
  }

  // Telemetry
  async listTelemetryBundles(): Promise<types.TelemetryBundle[]> {
    return this.request<types.TelemetryBundle[]>('/v1/telemetry/bundles');
  }

  // Adapters
  async listAdapters(): Promise<types.Adapter[]> {
    return this.request<types.Adapter[]>('/v1/adapters');
  }

  async getAdapter(adapterId: string): Promise<types.Adapter> {
    return this.request<types.Adapter>(`/v1/adapters/${adapterId}`);
  }

  async registerAdapter(data: types.RegisterAdapterRequest): Promise<types.Adapter> {
    return this.request<types.Adapter>('/v1/adapters/register', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async deleteAdapter(adapterId: string): Promise<void> {
    return this.request<void>(`/v1/adapters/${adapterId}`, {
      method: 'DELETE',
    });
  }

  // Training endpoints
  async listTrainingJobs(): Promise<types.TrainingJob[]> {
    return this.request<types.TrainingJob[]>('/v1/training/jobs');
  }

  async getTrainingJob(jobId: string): Promise<types.TrainingJob> {
    return this.request<types.TrainingJob>(`/v1/training/jobs/${jobId}`);
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
  async pinAdapter(adapterId: string, ttlHours?: number, reason?: string): Promise<void> {
    return this.request<void>(`/v1/adapters/${adapterId}/pin`, {
      method: 'POST',
      body: JSON.stringify({ ttl_hours: ttlHours, reason }),
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

  async promoteAdapterState(adapterId: string): Promise<types.AdapterStateResponse> {
    return this.request<types.AdapterStateResponse>(`/v1/adapters/${adapterId}/promote`, {
      method: 'POST',
    });
  }

  async downloadAdapterManifest(adapterId: string): Promise<types.AdapterManifest> {
    return this.request<types.AdapterManifest>(`/v1/adapters/${adapterId}/manifest`);
  }

  async getAdapterHealth(adapterId: string): Promise<types.AdapterHealthResponse> {
    return this.request<types.AdapterHealthResponse>(`/v1/adapters/${adapterId}/health`);
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
  async infer(data: types.InferRequest): Promise<types.InferResponse> {
    return this.request<types.InferResponse>('/v1/infer', {
      method: 'POST',
      body: JSON.stringify(data),
    });
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
  async exportTelemetryBundle(bundleId: string): Promise<types.ExportTelemetryBundleResponse> {
    return this.request<types.ExportTelemetryBundleResponse>(`/v1/telemetry/bundles/${bundleId}/export`);
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
  async registerGitRepository(request: {
    repo_id: string;
    path: string;
    branch?: string;
    description?: string;
  }): Promise<{
    repo_id: string;
    status: string;
    analysis: any;
    evidence_count: number;
  }> {
    return this.request(`/v1/git/repositories`, {
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

  // Monitoring methods
  async listMonitoringRules(tenantId?: string): Promise<types.MonitoringRule[]> {
    const params = new URLSearchParams();
    if (tenantId) params.append('tenant_id', tenantId);
    const query = params.toString() ? `?${params.toString()}` : '';
    return this.request<types.MonitoringRule[]>(`/v1/monitoring/rules${query}`);
  }

  async createMonitoringRule(data: types.CreateMonitoringRuleRequest): Promise<types.MonitoringRule> {
    return this.request<types.MonitoringRule>('/v1/monitoring/rules', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async listAlerts(filters?: types.AlertFilters): Promise<types.Alert[]> {
    const params = new URLSearchParams();
    if (filters?.tenant_id) params.append('tenant_id', filters.tenant_id);
    if (filters?.severity) params.append('severity', filters.severity);
    if (filters?.status) params.append('status', filters.status);
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
