/**
 * MonitoringService
 *
 * Handles alerting and monitoring rule operations including:
 * - Alert listing, acknowledgment, and resolution
 * - Monitoring rule CRUD operations
 */

import { BaseService } from './base';
import type {
  Alert,
  AlertFilters,
  AcknowledgeAlertRequest,
  ResolveAlertRequest,
  UpdateMonitoringRuleRequest,
} from '@/api/api-types';
import type {
  MonitoringRule,
  CreateMonitoringRuleRequest,
} from '@/api/adapter-types';

export class MonitoringService extends BaseService {
  // ============================================================================
  // Alert Operations
  // ============================================================================

  /**
   * List alerts with optional filters
   *
   * GET /v1/monitoring/alerts
   */
  async listAlerts(filters?: AlertFilters): Promise<Alert[]> {
    const qs = new URLSearchParams();
    if (filters?.severity) qs.append('severity', filters.severity);
    if (filters?.status) qs.append('status', filters.status);
    if (filters?.start_time) qs.append('start_time', filters.start_time);
    if (filters?.end_time) qs.append('end_time', filters.end_time);
    if (filters?.limit !== undefined) qs.append('limit', String(filters.limit));
    if (filters?.tenant_id) qs.append('tenant_id', filters.tenant_id);
    if (filters?.worker_id) qs.append('worker_id', filters.worker_id);
    if (filters?.sort) qs.append('sort', filters.sort);
    const query = qs.toString() ? `?${qs.toString()}` : '';
    return this.requestList<Alert>(`/v1/monitoring/alerts${query}`);
  }

  /**
   * Acknowledge an alert
   *
   * POST /v1/monitoring/alerts/:alertId/acknowledge
   */
  async acknowledgeAlert(alertId: string, data?: AcknowledgeAlertRequest): Promise<void> {
    await this.request<void>(`/v1/monitoring/alerts/${alertId}/acknowledge`, {
      method: 'POST',
      body: JSON.stringify(data ?? { alert_id: alertId }),
    });
  }

  /**
   * Resolve an alert
   *
   * POST /v1/monitoring/alerts/:alertId/resolve
   */
  async resolveAlert(alertId: string, data?: ResolveAlertRequest): Promise<void> {
    await this.request<void>(`/v1/monitoring/alerts/${alertId}/resolve`, {
      method: 'POST',
      body: JSON.stringify(data ?? { alert_id: alertId }),
    });
  }

  // ============================================================================
  // Monitoring Rule Operations
  // ============================================================================

  /**
   * List monitoring rules
   *
   * GET /v1/monitoring/rules
   */
  async listMonitoringRules(tenantId?: string): Promise<MonitoringRule[]> {
    const qs = new URLSearchParams();
    if (tenantId) qs.append('tenant_id', tenantId);
    const query = qs.toString() ? `?${qs.toString()}` : '';
    return this.requestList<MonitoringRule>(`/v1/monitoring/rules${query}`);
  }

  /**
   * Create a monitoring rule
   *
   * POST /v1/monitoring/rules
   */
  async createMonitoringRule(data: CreateMonitoringRuleRequest): Promise<MonitoringRule> {
    return this.request<MonitoringRule>('/v1/monitoring/rules', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  /**
   * Update a monitoring rule
   *
   * PATCH /v1/monitoring/rules/:ruleId
   */
  async updateMonitoringRule(ruleId: string, data: UpdateMonitoringRuleRequest): Promise<MonitoringRule> {
    return this.request<MonitoringRule>(`/v1/monitoring/rules/${ruleId}`, {
      method: 'PATCH',
      body: JSON.stringify(data),
    });
  }

  /**
   * Delete a monitoring rule
   *
   * DELETE /v1/monitoring/rules/:ruleId
   */
  async deleteMonitoringRule(ruleId: string): Promise<void> {
    await this.request<void>(`/v1/monitoring/rules/${ruleId}`, {
      method: 'DELETE',
    });
  }
}
