/**
 * Admin service - handles admin-level operations including tenant management,
 * nodes, workers, plans, promotions, users, workspaces, services, and diagnostics.
 */

import type { ApiClient } from '@/api/client';
import * as types from '@/api/types';
import * as authTypes from '@/api/auth-types';
import type {
  AdapterRepositorySummary,
  AdapterRepositoryPolicy,
  UpdateAdapterRepositoryPolicyRequest,
} from '@/api/repo-types';
import type { ContractSamplesResponse } from '@/api/api-types';
import { handleBlobResponse } from '@/api/helpers';
import { logger } from '@/utils/logger';

export class AdminService {
  constructor(private client: ApiClient) {}

  // ===== Tenants =====

  async listTenants(): Promise<types.Tenant[]> {
    return this.client.requestList<types.Tenant>('/v1/tenants');
  }

  async createTenant(data: types.CreateTenantRequest): Promise<types.Tenant> {
    return this.client.request<types.Tenant>('/v1/tenants', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async updateTenant(tenantId: string, name: string): Promise<types.TenantResponse> {
    return this.client.request<types.TenantResponse>(`/v1/tenants/${tenantId}`, {
      method: 'PUT',
      body: JSON.stringify({ name }),
    });
  }

  async pauseTenant(tenantId: string): Promise<void> {
    return this.client.request<void>(`/v1/tenants/${tenantId}/pause`, {
      method: 'POST',
    });
  }

  async archiveTenant(tenantId: string): Promise<void> {
    return this.client.request<void>(`/v1/tenants/${tenantId}/archive`, {
      method: 'POST',
    });
  }

  async assignTenantPolicies(tenantId: string, cpids: string[]): Promise<types.AssignPoliciesResponse> {
    return this.client.request<types.AssignPoliciesResponse>(`/v1/tenants/${tenantId}/policies`, {
      method: 'POST',
      body: JSON.stringify({ cpids }),
    });
  }

  async assignTenantAdapters(tenantId: string, adapterIds: string[]): Promise<types.AssignAdaptersResponse> {
    return this.client.request<types.AssignAdaptersResponse>(`/v1/tenants/${tenantId}/adapters`, {
      method: 'POST',
      body: JSON.stringify({ adapter_ids: adapterIds }),
    });
  }

  async getTenantUsage(tenantId: string): Promise<types.TenantUsageResponse> {
    return this.client.request<types.TenantUsageResponse>(`/v1/tenants/${tenantId}/usage`);
  }

  // ===== Nodes =====

  async listNodes(): Promise<types.Node[]> {
    return this.client.requestList<types.Node>('/v1/nodes');
  }

  async registerNode(data: types.RegisterNodeRequest): Promise<types.Node> {
    return this.client.request<types.Node>('/v1/nodes/register', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async testNodeConnection(nodeId: string): Promise<types.NodePingResponse> {
    return this.client.request<types.NodePingResponse>(`/v1/nodes/${nodeId}/ping`, {
      method: 'POST',
    });
  }

  async markNodeOffline(nodeId: string): Promise<void> {
    return this.client.request<void>(`/v1/nodes/${nodeId}/offline`, {
      method: 'POST',
    });
  }

  async evictNode(nodeId: string): Promise<void> {
    return this.client.request<void>(`/v1/nodes/${nodeId}`, {
      method: 'DELETE',
    });
  }

  async getNodeDetails(nodeId: string): Promise<types.NodeDetailsResponse> {
    return this.client.request<types.NodeDetailsResponse>(`/v1/nodes/${nodeId}/details`);
  }

  // ===== Workers =====

  async listWorkers(tenantId?: string, nodeId?: string): Promise<types.WorkerResponse[]> {
    const params = new URLSearchParams();
    if (tenantId) params.append('tenant_id', tenantId);
    if (nodeId) params.append('node_id', nodeId);
    const query = params.toString() ? `?${params.toString()}` : '';
    return this.client.requestList<types.WorkerResponse>(`/v1/workers${query}`);
  }

  async spawnWorker(request: types.SpawnWorkerRequest): Promise<types.WorkerResponse> {
    return this.client.request<types.WorkerResponse>('/v1/workers/spawn', {
      method: 'POST',
      body: JSON.stringify(request),
    });
  }

  async stopWorker(workerId: string, force: boolean = false): Promise<void> {
    return this.client.request<void>(`/v1/workers/${workerId}/stop`, {
      method: 'POST',
      body: JSON.stringify({ force }),
    });
  }

  async getWorkerDetails(workerId: string): Promise<types.WorkerDetailsResponse> {
    return this.client.request<types.WorkerDetailsResponse>(`/v1/workers/${workerId}/details`);
  }

  // ===== Worker Diagnostics =====

  async getProcessLogs(workerId: string, filters?: types.ProcessLogFilters): Promise<types.ProcessLog[]> {
    const params = new URLSearchParams();
    if (filters?.level) params.append('level', filters.level);
    if (filters?.limit) params.append('limit', filters.limit.toString());
    if (filters?.start_time) params.append('start_time', filters.start_time);
    if (filters?.end_time) params.append('end_time', filters.end_time);
    const query = params.toString() ? `?${params.toString()}` : '';
    return this.client.requestList<types.ProcessLog>(`/v1/workers/${workerId}/logs${query}`);
  }

  async getProcessCrashes(workerId: string): Promise<types.ProcessCrash[]> {
    return this.client.requestList<types.ProcessCrash>(`/v1/workers/${workerId}/crashes`);
  }

  async startDebugSession(workerId: string, config: types.DebugSessionConfig): Promise<types.DebugSession> {
    return this.client.request<types.DebugSession>(`/v1/workers/${workerId}/debug`, {
      method: 'POST',
      body: JSON.stringify(config),
    });
  }

  async runTroubleshootingStep(workerId: string, step: types.TroubleshootingStep): Promise<types.TroubleshootingResult> {
    return this.client.request<types.TroubleshootingResult>(`/v1/workers/${workerId}/troubleshoot`, {
      method: 'POST',
      body: JSON.stringify(step),
    });
  }

  async getWorkerIncidents(workerId: string, limit?: number): Promise<types.WorkerIncident[]> {
    const params = new URLSearchParams();
    if (limit !== undefined) params.append('limit', limit.toString());
    const query = params.toString() ? `?${params.toString()}` : '';
    return this.client.requestList<types.WorkerIncident>(`/v1/workers/${workerId}/incidents${query}`);
  }

  async getWorkersHealthSummary(): Promise<types.WorkerHealthSummary[]> {
    return this.client.requestList<types.WorkerHealthSummary>('/v1/workers/health/summary');
  }

  // ===== Plans =====

  async listPlans(): Promise<types.Plan[]> {
    return this.client.requestList<types.Plan>('/v1/plans');
  }

  async buildPlan(data: types.BuildPlanRequest): Promise<types.Plan> {
    return this.client.request<types.Plan>('/v1/plans/build', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async rebuildPlan(planId: string): Promise<types.Plan> {
    return this.client.request<types.Plan>(`/v1/plans/${planId}/rebuild`, {
      method: 'POST',
    });
  }

  async comparePlans(planId1: string, planId2: string): Promise<types.PlanComparisonResponse> {
    return this.client.request<types.PlanComparisonResponse>('/v1/plans/compare', {
      method: 'POST',
      body: JSON.stringify({ plan_id_1: planId1, plan_id_2: planId2 }),
    });
  }

  async deletePlan(planId: string): Promise<void> {
    return this.client.request<void>(`/v1/plans/${planId}`, {
      method: 'DELETE',
    });
  }

  async exportPlanManifest(planId: string): Promise<Blob> {
    const path = `/v1/plans/${planId}/manifest`;
    const url = `${this.client['baseUrl']}${path}`;
    const token = this.client.getToken();
    const response = await fetch(url, {
      headers: token ? { Authorization: `Bearer ${token}` } : undefined,
      credentials: 'omit',
    });
    return handleBlobResponse(response, { method: 'GET', path });
  }

  // ===== Control Plane / Promotions =====

  async promote(data: types.PromotionRequest): Promise<types.PromotionRecord> {
    return this.client.request<types.PromotionRecord>('/v1/cp/promote', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async getPromotionGates(cpid: string): Promise<types.PromotionGate[]> {
    return this.client.requestList<types.PromotionGate>(`/v1/cp/promotion-gates/${cpid}`);
  }

  async rollback(): Promise<void> {
    return this.client.request('/v1/cp/rollback', { method: 'POST' });
  }

  async getPromotion(id: string): Promise<types.PromotionRecord> {
    return this.client.request<types.PromotionRecord>(`/v1/promotions/${id}`);
  }

  // ===== Workspaces =====

  async listWorkspaces(): Promise<types.Workspace[]> {
    return this.client.requestList<types.Workspace>('/v1/workspaces');
  }

  async listUserWorkspaces(): Promise<types.Workspace[]> {
    return this.client.requestList<types.Workspace>('/v1/workspaces/my');
  }

  async createWorkspace(data: types.CreateWorkspaceRequest): Promise<types.Workspace> {
    return this.client.request<types.Workspace>('/v1/workspaces', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async getWorkspace(workspaceId: string): Promise<types.Workspace> {
    return this.client.request<types.Workspace>(`/v1/workspaces/${workspaceId}`);
  }

  async updateWorkspace(workspaceId: string, data: { name?: string; description?: string }): Promise<types.Workspace> {
    return this.client.request<types.Workspace>(`/v1/workspaces/${workspaceId}`, {
      method: 'PUT',
      body: JSON.stringify(data),
    });
  }

  async deleteWorkspace(workspaceId: string): Promise<void> {
    return this.client.request<void>(`/v1/workspaces/${workspaceId}`, {
      method: 'DELETE',
    });
  }

  async listWorkspaceMembers(workspaceId: string): Promise<types.WorkspaceMember[]> {
    return this.client.requestList<types.WorkspaceMember>(`/v1/workspaces/${workspaceId}/members`);
  }

  async addWorkspaceMember(workspaceId: string, data: types.AddWorkspaceMemberRequest): Promise<{ id: string }> {
    return this.client.request<{ id: string }>(`/v1/workspaces/${workspaceId}/members`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async updateWorkspaceMember(workspaceId: string, memberId: string, data: { role: string }): Promise<void> {
    return this.client.request<void>(`/v1/workspaces/${workspaceId}/members/${memberId}`, {
      method: 'PUT',
      body: JSON.stringify(data),
    });
  }

  async removeWorkspaceMember(workspaceId: string, memberId: string): Promise<void> {
    return this.client.request<void>(`/v1/workspaces/${workspaceId}/members/${memberId}`, {
      method: 'DELETE',
    });
  }

  async listWorkspaceResources(workspaceId: string): Promise<types.WorkspaceResource[]> {
    return this.client.requestList<types.WorkspaceResource>(`/v1/workspaces/${workspaceId}/resources`);
  }

  async shareWorkspaceResource(workspaceId: string, data: { resource_type: string; resource_id: string }): Promise<{ id: string }> {
    return this.client.request<{ id: string }>(`/v1/workspaces/${workspaceId}/resources`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async unshareWorkspaceResource(workspaceId: string, resourceId: string, resourceType: string): Promise<void> {
    const params = new URLSearchParams({ resource_type: resourceType });
    return this.client.request<void>(`/v1/workspaces/${workspaceId}/resources/${resourceId}?${params.toString()}`, {
      method: 'DELETE',
    });
  }

  // ===== Messaging =====

  async listWorkspaceMessages(workspaceId: string, params?: { limit?: number; offset?: number }): Promise<types.Message[]> {
    const queryParams = new URLSearchParams();
    if (params?.limit) queryParams.append('limit', params.limit.toString());
    if (params?.offset) queryParams.append('offset', params.offset.toString());
    const query = queryParams.toString() ? `?${queryParams.toString()}` : '';
    return this.client.requestList<types.Message>(`/v1/workspaces/${workspaceId}/messages${query}`);
  }

  async createMessage(workspaceId: string, data: types.CreateMessageRequest): Promise<types.Message> {
    return this.client.request<types.Message>(`/v1/workspaces/${workspaceId}/messages`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async editMessage(workspaceId: string, messageId: string, data: types.CreateMessageRequest): Promise<types.Message> {
    return this.client.request<types.Message>(`/v1/workspaces/${workspaceId}/messages/${messageId}`, {
      method: 'PUT',
      body: JSON.stringify(data),
    });
  }

  async getMessageThread(workspaceId: string, threadId: string): Promise<types.Message[]> {
    return this.client.requestList<types.Message>(`/v1/workspaces/${workspaceId}/messages/${threadId}/thread`);
  }

  async markMessageRead(workspaceId: string, messageId: string): Promise<types.Message> {
    return this.client.request<types.Message>(`/v1/workspaces/${workspaceId}/messages/${messageId}/read`, {
      method: 'POST',
    });
  }

  async deleteMessage(workspaceId: string, messageId: string): Promise<void> {
    return this.client.request<void>(`/v1/workspaces/${workspaceId}/messages/${messageId}`, {
      method: 'DELETE',
    });
  }

  // ===== Notifications =====

  async listNotifications(params?: {
    workspace_id?: string;
    type?: string;
    unread_only?: boolean;
    limit?: number;
    offset?: number;
  }): Promise<types.Notification[]> {
    const queryParams = new URLSearchParams();
    if (params?.workspace_id) queryParams.append('workspace_id', params.workspace_id);
    if (params?.type) queryParams.append('type', params.type);
    if (params?.unread_only !== undefined) queryParams.append('unread_only', params.unread_only.toString());
    if (params?.limit) queryParams.append('limit', params.limit.toString());
    if (params?.offset) queryParams.append('offset', params.offset.toString());
    const query = queryParams.toString() ? `?${queryParams.toString()}` : '';
    return this.client.requestList<types.Notification>(`/v1/notifications${query}`);
  }

  async getNotificationSummary(workspaceId?: string): Promise<types.NotificationSummary> {
    const queryParams = new URLSearchParams();
    if (workspaceId) queryParams.append('workspace_id', workspaceId);
    const query = queryParams.toString() ? `?${queryParams.toString()}` : '';
    return this.client.request<types.NotificationSummary>(`/v1/notifications/summary${query}`);
  }

  async markNotificationRead(notificationId: string): Promise<void> {
    return this.client.request<void>(`/v1/notifications/${notificationId}/read`, {
      method: 'POST',
    });
  }

  async markAllNotificationsRead(workspaceId?: string): Promise<{ count: number }> {
    const queryParams = new URLSearchParams();
    if (workspaceId) queryParams.append('workspace_id', workspaceId);
    const query = queryParams.toString() ? `?${queryParams.toString()}` : '';
    return this.client.request<{ count: number }>(`/v1/notifications/read-all${query}`, {
      method: 'POST',
    });
  }

  // ===== Tutorials =====

  async listTutorials(): Promise<types.Tutorial[]> {
    return this.client.requestList<types.Tutorial>('/v1/tutorials');
  }

  async markTutorialCompleted(tutorialId: string): Promise<void> {
    return this.client.request<void>(`/v1/tutorials/${tutorialId}/complete`, {
      method: 'POST',
    });
  }

  async unmarkTutorialCompleted(tutorialId: string): Promise<void> {
    return this.client.request<void>(`/v1/tutorials/${tutorialId}/complete`, {
      method: 'DELETE',
    });
  }

  async markTutorialDismissed(tutorialId: string): Promise<void> {
    return this.client.request<void>(`/v1/tutorials/${tutorialId}/dismiss`, {
      method: 'POST',
    });
  }

  async unmarkTutorialDismissed(tutorialId: string): Promise<void> {
    return this.client.request<void>(`/v1/tutorials/${tutorialId}/dismiss`, {
      method: 'DELETE',
    });
  }

  // ===== Activity Events =====

  async listActivityEvents(params?: {
    workspace_id?: string;
    user_id?: string;
    tenant_id?: string;
    event_type?: string;
    limit?: number;
    offset?: number;
  }): Promise<types.ActivityEvent[]> {
    const queryParams = new URLSearchParams();
    if (params?.workspace_id) queryParams.append('workspace_id', params.workspace_id);
    if (params?.user_id) queryParams.append('user_id', params.user_id);
    if (params?.tenant_id) queryParams.append('tenant_id', params.tenant_id);
    if (params?.event_type) queryParams.append('event_type', params.event_type);
    if (params?.limit) queryParams.append('limit', params.limit.toString());
    if (params?.offset) queryParams.append('offset', params.offset.toString());
    const query = queryParams.toString() ? `?${queryParams.toString()}` : '';
    return this.client.requestList<types.ActivityEvent>(`/v1/activity${query}`);
  }

  async getRecentActivityEvents(params?: { event_types?: string[]; limit?: number }): Promise<types.RecentActivityEvent[]> {
    const queryParams = new URLSearchParams();
    if (params?.limit) {
      queryParams.append('limit', params.limit.toString());
    }
    params?.event_types?.forEach((eventType) => {
      queryParams.append('event_types[]', eventType);
    });
    const query = queryParams.toString() ? `?${queryParams.toString()}` : '';
    return this.client.requestList<types.RecentActivityEvent>(`/v1/telemetry/events/recent${query}`);
  }

  async createActivityEvent(data: types.CreateActivityEventRequest): Promise<types.ActivityEvent> {
    return this.client.request<types.ActivityEvent>('/v1/activity', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async listUserWorkspaceActivity(limit?: number): Promise<types.ActivityEvent[]> {
    const queryParams = new URLSearchParams();
    if (limit) queryParams.append('limit', limit.toString());
    const query = queryParams.toString() ? `?${queryParams.toString()}` : '';
    return this.client.requestList<types.ActivityEvent>(`/v1/activity/my${query}`);
  }

  // ===== Service Control =====

  /**
   * Get current system status including service information
   * Citation: crates/adapteros-server/src/status_writer.rs L135-144
   */
  async getStatus(): Promise<types.AdapterOSStatus> {
    return this.client.request<types.AdapterOSStatus>('/v1/status', {
      method: 'GET',
    });
  }

  /**
   * Start a service
   * Citation: crates/adapteros-server-api/src/handlers/services.rs
   */
  async startService(serviceId: string): Promise<{ success: boolean; message: string }> {
    logger.info('Starting service', {
      component: 'AdminService',
      operation: 'startService',
      serviceId,
    });

    return this.client.request(`/v1/services/${serviceId}/start`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
    }, false, undefined, false); // No retry for start operations
  }

  /**
   * Stop a service
   * Citation: crates/adapteros-server-api/src/handlers/services.rs
   */
  async stopService(serviceId: string): Promise<{ success: boolean; message: string }> {
    logger.info('Stopping service', {
      component: 'AdminService',
      operation: 'stopService',
      serviceId,
    });

    return this.client.request(`/v1/services/${serviceId}/stop`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
    }, false, undefined, false); // No retry for stop operations
  }

  /**
   * Restart a service
   * Citation: crates/adapteros-server-api/src/handlers/services.rs
   */
  async restartService(serviceId: string): Promise<{ success: boolean; message: string }> {
    logger.info('Restarting service', {
      component: 'AdminService',
      operation: 'restartService',
      serviceId,
    });

    return this.client.request(`/v1/services/${serviceId}/restart`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
    }, false, undefined, false); // No retry for restart operations
  }

  /**
   * Start all essential services
   * Citation: crates/adapteros-server-api/src/handlers/services.rs
   */
  async startEssentialServices(): Promise<{ success: boolean; message: string }> {
    logger.info('Starting all essential services', {
      component: 'AdminService',
      operation: 'startEssentialServices',
    });

    return this.client.request('/v1/services/essential/start', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
    }, false, undefined, false); // No retry for start operations
  }

  /**
   * Stop all essential services
   * Citation: crates/adapteros-server-api/src/handlers/services.rs
   */
  async stopEssentialServices(): Promise<{ success: boolean; message: string }> {
    logger.info('Stopping all essential services', {
      component: 'AdminService',
      operation: 'stopEssentialServices',
    });

    return this.client.request('/v1/services/essential/stop', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
    }, false, undefined, false); // No retry for stop operations
  }

  /**
   * Get service logs
   * Citation: crates/adapteros-server-api/src/handlers/services.rs
   */
  async getServiceLogs(serviceId: string, lines: number = 100): Promise<string[]> {
    logger.info('Fetching service logs', {
      component: 'AdminService',
      operation: 'getServiceLogs',
      serviceId,
      lines,
    });

    return this.client.requestList<string>(`/v1/services/${serviceId}/logs?lines=${lines}`, {
      method: 'GET',
    });
  }

  // ===== Dashboard Configuration =====

  async getDashboardConfig(): Promise<types.DashboardConfig> {
    return this.client.request<types.DashboardConfig>('/v1/dashboard/config');
  }

  async updateDashboardConfig(config: types.UpdateDashboardConfigRequest): Promise<types.UpdateDashboardConfigResponse> {
    return this.client.request<types.UpdateDashboardConfigResponse>('/v1/dashboard/config', {
      method: 'PUT',
      body: JSON.stringify(config),
    });
  }

  async resetDashboardConfig(): Promise<types.ResetDashboardConfigResponse> {
    return this.client.request<types.ResetDashboardConfigResponse>('/v1/dashboard/config', {
      method: 'DELETE',
    });
  }

  // ===== Orchestration =====

  /**
   * Get orchestration configuration
   *
   * Retrieves the current prompt orchestration configuration including routing
   * strategy, adapter settings, and custom rules.
   *
   * @returns OrchestrationConfig or null if endpoint not available
   */
  async getOrchestrationConfig(): Promise<types.OrchestrationConfig | null> {
    logger.info('Fetching orchestration config', {
      component: 'AdminService',
      operation: 'getOrchestrationConfig',
    });

    try {
      return await this.client.request<types.OrchestrationConfig>('/v1/orchestration/config');
    } catch (error) {
      // Gracefully handle 404 - endpoint may not be implemented yet
      if (error instanceof Error && (error.message.includes('404') || error.message.includes('Not Found'))) {
        logger.info('Orchestration config endpoint not available', {
          component: 'AdminService',
          operation: 'getOrchestrationConfig',
        });
        return null;
      }
      throw error;
    }
  }

  /**
   * Save orchestration configuration
   *
   * Updates the prompt orchestration configuration with new settings.
   *
   * @param config - New orchestration configuration
   * @returns Updated configuration
   * @throws Error if endpoint not available or validation fails
   */
  async saveOrchestrationConfig(config: types.OrchestrationConfig): Promise<types.OrchestrationConfig> {
    logger.info('Saving orchestration config', {
      component: 'AdminService',
      operation: 'saveOrchestrationConfig',
      routing_strategy: config.routing_strategy,
      enabled: config.enabled,
    });

    try {
      return await this.client.request<types.OrchestrationConfig>('/v1/orchestration/config', {
        method: 'PUT',
        body: JSON.stringify(config),
      });
    } catch (error) {
      // Provide friendly error for 404
      if (error instanceof Error && (error.message.includes('404') || error.message.includes('Not Found'))) {
        logger.warn('Orchestration config endpoint not available for saving', {
          component: 'AdminService',
          operation: 'saveOrchestrationConfig',
        });
        throw new Error('Orchestration configuration endpoint is not available. The backend may not support this feature yet.');
      }
      throw error;
    }
  }

  /**
   * Analyze a prompt for routing recommendations
   *
   * Sends a prompt to the orchestration service for analysis, returning
   * intent detection, complexity scoring, and adapter recommendations.
   *
   * @param prompt - The prompt text to analyze
   * @returns PromptAnalysis with recommendations
   * @throws Error if endpoint not available or analysis fails
   */
  async analyzePrompt(prompt: string): Promise<types.PromptAnalysis> {
    logger.info('Analyzing prompt', {
      component: 'AdminService',
      operation: 'analyzePrompt',
      promptLength: prompt.length,
    });

    try {
      return await this.client.request<types.PromptAnalysis>('/v1/orchestration/analyze', {
        method: 'POST',
        body: JSON.stringify({ prompt }),
      });
    } catch (error) {
      // Provide friendly error for 404
      if (error instanceof Error && (error.message.includes('404') || error.message.includes('Not Found'))) {
        logger.warn('Orchestration analyze endpoint not available', {
          component: 'AdminService',
          operation: 'analyzePrompt',
        });
        throw new Error('Prompt analysis endpoint is not available. The backend may not support this feature yet.');
      }
      throw error;
    }
  }

  /**
   * Get orchestration metrics
   *
   * Retrieves metrics about orchestration performance including request counts,
   * latency percentiles, cache hit rates, and adapter usage statistics.
   *
   * @returns OrchestrationMetrics or null if endpoint not available
   */
  async getOrchestrationMetrics(): Promise<types.OrchestrationMetrics | null> {
    logger.info('Fetching orchestration metrics', {
      component: 'AdminService',
      operation: 'getOrchestrationMetrics',
    });

    try {
      return await this.client.request<types.OrchestrationMetrics>('/v1/orchestration/metrics');
    } catch (error) {
      // Gracefully handle 404 - endpoint may not be implemented yet
      if (error instanceof Error && (error.message.includes('404') || error.message.includes('Not Found'))) {
        logger.info('Orchestration metrics endpoint not available', {
          component: 'AdminService',
          operation: 'getOrchestrationMetrics',
        });
        return null;
      }
      throw error;
    }
  }

  // ===== User Management =====

  /**
   * List all users with optional pagination and filters.
   * Requires Admin role.
   *
   * @param params - Optional pagination and filter parameters
   * @returns List of users with pagination metadata
   */
  async listUsers(params?: {
    page?: number;
    page_size?: number;
    role?: authTypes.UserRole;
    tenant_id?: string;
  }): Promise<authTypes.ListUsersResponse> {
    logger.info('Listing users', {
      component: 'AdminService',
      operation: 'listUsers',
      params,
    });
    const queryParams = new URLSearchParams();
    if (params?.page !== undefined) queryParams.append('page', String(params.page));
    if (params?.page_size !== undefined) queryParams.append('page_size', String(params.page_size));
    if (params?.role) queryParams.append('role', params.role);
    if (params?.tenant_id) queryParams.append('tenant_id', params.tenant_id);
    const query = queryParams.toString() ? `?${queryParams.toString()}` : '';
    return this.client.request<authTypes.ListUsersResponse>(`/v1/admin/users${query}`);
  }

  /**
   * Get user by ID
   *
   * Retrieves detailed information about a specific user.
   * Requires Admin role.
   *
   * @param userId - User ID
   * @returns User details
   */
  async getUser(userId: string): Promise<authTypes.User> {
    logger.info('Getting user', {
      component: 'AdminService',
      operation: 'getUser',
      userId,
    });
    return this.client.request<authTypes.User>(`/v1/admin/users/${encodeURIComponent(userId)}`);
  }

  /**
   * Create a new user
   *
   * Registers a new user in the system.
   * Requires Admin role.
   *
   * @param data - User registration data
   * @returns Created user
   */
  async createUser(data: authTypes.RegisterUserRequest): Promise<authTypes.User> {
    logger.info('Creating user', {
      component: 'AdminService',
      operation: 'createUser',
      email: data.email,
      role: data.role,
    });
    return this.client.request<authTypes.User>('/v1/admin/users', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  /**
   * Update an existing user
   *
   * Updates user details including role assignment.
   * Requires Admin role.
   *
   * @param userId - User ID to update
   * @param data - User update data
   * @returns Updated user
   */
  async updateUser(userId: string, data: authTypes.UpdateUserRequest): Promise<authTypes.User> {
    logger.info('Updating user', {
      component: 'AdminService',
      operation: 'updateUser',
      userId,
      updates: data,
    });
    return this.client.request<authTypes.User>(`/v1/admin/users/${encodeURIComponent(userId)}`, {
      method: 'PUT',
      body: JSON.stringify(data),
    });
  }

  /**
   * Delete a user
   *
   * Removes a user from the system.
   * Requires Admin role.
   *
   * @param userId - User ID to delete
   */
  async deleteUser(userId: string): Promise<void> {
    logger.info('Deleting user', {
      component: 'AdminService',
      operation: 'deleteUser',
      userId,
    });
    return this.client.request<void>(`/v1/admin/users/${encodeURIComponent(userId)}`, {
      method: 'DELETE',
    });
  }

  /**
   * Assign role to a user
   *
   * Updates the role of an existing user.
   * Requires Admin role.
   *
   * @param userId - User ID
   * @param role - New role to assign
   * @returns Updated user
   */
  async assignUserRole(userId: string, role: authTypes.UserRole): Promise<authTypes.User> {
    logger.info('Assigning user role', {
      component: 'AdminService',
      operation: 'assignUserRole',
      userId,
      role,
    });
    return this.client.request<authTypes.User>(`/v1/admin/users/${encodeURIComponent(userId)}/role`, {
      method: 'PUT',
      body: JSON.stringify({ role }),
    });
  }

  /**
   * Reset user password (Admin)
   *
   * Sends a password reset email to the user.
   * Requires Admin role.
   *
   * @param userId - User ID
   */
  async resetUserPassword(userId: string): Promise<void> {
    logger.info('Resetting user password', {
      component: 'AdminService',
      operation: 'resetUserPassword',
      userId,
    });
    return this.client.request<void>(`/v1/admin/users/${encodeURIComponent(userId)}/reset-password`, {
      method: 'POST',
    });
  }

  /**
   * Activate or deactivate a user
   *
   * Enables or disables user access to the system.
   * Requires Admin role.
   *
   * @param userId - User ID
   * @param isActive - Whether the user should be active
   * @returns Updated user
   */
  async setUserActive(userId: string, isActive: boolean): Promise<authTypes.User> {
    logger.info('Setting user active status', {
      component: 'AdminService',
      operation: 'setUserActive',
      userId,
      isActive,
    });
    return this.client.request<authTypes.User>(`/v1/admin/users/${encodeURIComponent(userId)}`, {
      method: 'PUT',
      body: JSON.stringify({ is_active: isActive }),
    });
  }

  // ===== Adapter Repository Management =====

  /**
   * List adapter repositories for policy management
   *
   * GET /v1/admin/adapter-repositories
   */
  async listAdapterRepositories(): Promise<AdapterRepositorySummary[]> {
    return this.client.requestList<AdapterRepositorySummary>('/v1/admin/adapter-repositories');
  }

  /**
   * Update adapter repository policy
   *
   * PATCH /v1/admin/adapter-repositories/:repoId/policy
   */
  async updateAdapterRepositoryPolicy(
    repoId: string,
    data: UpdateAdapterRepositoryPolicyRequest
  ): Promise<AdapterRepositoryPolicy> {
    return this.client.request<AdapterRepositoryPolicy>(
      `/v1/admin/adapter-repositories/${encodeURIComponent(repoId)}/policy`,
      {
        method: 'PATCH',
        body: JSON.stringify(data),
      }
    );
  }

  // ===== Dev/Contract Tools =====

  /**
   * Get contract samples for dev tools
   *
   * GET /v1/dev/contracts
   */
  async getContractSamples(): Promise<ContractSamplesResponse> {
    return this.client.request<ContractSamplesResponse>('/v1/dev/contracts');
  }

  // ===== Owner CLI =====

  /**
   * Run an owner CLI command
   *
   * POST /v1/owner/cli
   */
  async runOwnerCli(command: string): Promise<{ output: string; exit_code: number }> {
    logger.info('Running owner CLI command', {
      component: 'AdminService',
      operation: 'runOwnerCli',
      commandLength: command.length,
    });

    return this.client.request<{ output: string; exit_code: number }>('/v1/owner/cli', {
      method: 'POST',
      body: JSON.stringify({ command }),
    });
  }
}
