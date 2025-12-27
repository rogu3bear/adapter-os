/**
 * System service - handles health checks, readiness, metadata, metrics, and capacity.
 */

import type { ApiClient } from '@/api/client';
import * as types from '@/api/types';
import * as apiTypes from '@/api/api-types';
import * as ownerTypes from '@/api/owner-types';
import * as pilotStatusTypes from '@/api/pilot-status-types';
import * as systemStateTypes from '@/api/system-state-types';
import * as documentTypes from '@/api/document-types';

export class SystemService {
  constructor(private client: ApiClient) {}

  // Health endpoints
  async health(): Promise<types.HealthResponse> {
    return this.client.request<types.HealthResponse>('/healthz');
  }

  async getHealthz(): Promise<types.HealthResponse> {
    return this.health();
  }

  async getHealthzAll(): Promise<apiTypes.SystemHealthResponse> {
    return this.client.request<apiTypes.SystemHealthResponse>('/healthz/all');
  }

  async getComponentHealth(component: string): Promise<types.ComponentHealth> {
    return this.client.request<types.ComponentHealth>(`/healthz/${component}`);
  }

  async ready(): Promise<types.ReadyzResponse> {
    return this.client.request<types.ReadyzResponse>('/readyz');
  }

  async getReadyz(): Promise<types.ReadyzResponse> {
    return this.ready();
  }

  async getSystemReady(): Promise<apiTypes.SystemReadyResponse> {
    return this.client.request<apiTypes.SystemReadyResponse>('/system/ready');
  }

  async restartSystem(): Promise<void> {
    return this.client.request<void>('/system/restart', { method: 'POST' });
  }

  async stopSystem(): Promise<void> {
    return this.client.request<void>('/system/stop', { method: 'POST' });
  }

  // Metadata
  async meta(): Promise<types.MetaResponse> {
    return this.client.request<types.MetaResponse>('/v1/meta');
  }

  async getMeta(): Promise<types.MetaResponse> {
    return this.meta();
  }

  // Metrics
  async getSystemMetrics(): Promise<types.SystemMetrics> {
    return this.client.request<types.SystemMetrics>('/v1/metrics/system');
  }

  async getTenantStorageUsage(): Promise<apiTypes.TenantStorageUsageResponse> {
    return this.client.request<apiTypes.TenantStorageUsageResponse>('/v1/storage/tenant-usage');
  }

  async getQualityMetrics(): Promise<types.QualityMetrics> {
    return this.client.request<types.QualityMetrics>('/v1/metrics/quality');
  }

  async getAdapterMetrics(): Promise<types.AdapterMetrics[]> {
    return this.client.requestList<types.AdapterMetrics>('/v1/metrics/adapters');
  }

  // System overview
  async getSystemOverview(): Promise<ownerTypes.SystemOverview> {
    return this.client.request('/v1/system/overview');
  }

  async getPilotStatus(): Promise<pilotStatusTypes.PilotStatusResponse> {
    return this.client.request<pilotStatusTypes.PilotStatusResponse>('/v1/system/pilot-status');
  }

  async getSystemState(
    params?: systemStateTypes.SystemStateQuery
  ): Promise<systemStateTypes.SystemStateResponse> {
    const queryParams = new URLSearchParams();
    if (params?.include_adapters !== undefined) {
      queryParams.set('include_adapters', String(params.include_adapters));
    }
    if (params?.top_adapters !== undefined) {
      queryParams.set('top_adapters', String(params.top_adapters));
    }
    if (params?.tenant_id) {
      queryParams.set('tenant_id', params.tenant_id);
    }
    const query = queryParams.toString() ? `?${queryParams.toString()}` : '';
    return this.client.request<systemStateTypes.SystemStateResponse>(`/v1/system/state${query}`);
  }

  // Capacity and diagnostics
  async getCapacity(): Promise<types.CapacityResponse> {
    return this.client.request<types.CapacityResponse>('/v1/system/capacity');
  }

  async getDeterminismStatus(): Promise<types.DeterminismStatusResponse> {
    return this.client.request<types.DeterminismStatusResponse>('/v1/diagnostics/determinism');
  }

  async getDiagnosticsQuarantineStatus(): Promise<types.AdapterQuarantineStatusResponse> {
    return this.client.request<types.AdapterQuarantineStatusResponse>('/v1/diagnostics/quarantine-status');
  }

  // Settings
  async getSettings(): Promise<documentTypes.SystemSettings> {
    return this.client.request<documentTypes.SystemSettings>('/v1/settings');
  }

  async updateSettings(
    request: documentTypes.UpdateSettingsRequest
  ): Promise<documentTypes.SettingsUpdateResponse> {
    return this.client.request<documentTypes.SettingsUpdateResponse>('/v1/settings', {
      method: 'PUT',
      body: JSON.stringify(request),
    });
  }

  // Wait for healthy (utility)
  async waitForHealthy(timeout: number = 30000): Promise<boolean> {
    const startTime = Date.now();
    while (Date.now() - startTime < timeout) {
      try {
        const health = await this.health();
        // 'ok' for backward compatibility with older backend versions
        if (health.status === 'healthy' || (health.status as string) === 'ok') {
          return true;
        }
      } catch {
        // Continue waiting
      }
      await new Promise(resolve => setTimeout(resolve, 1000));
    }
    return false;
  }

  // Security and anomaly detection
  async getAnomalyDetectionStatus(): Promise<apiTypes.AnomalyDetectionStatus> {
    return this.client.request<apiTypes.AnomalyDetectionStatus>('/v1/system/anomaly-detection');
  }

  async getAccessPatterns(tenantId: string): Promise<apiTypes.AccessPattern[]> {
    return this.client.requestList<apiTypes.AccessPattern>(`/v1/system/access-patterns?tenant_id=${encodeURIComponent(tenantId)}`);
  }

  async runIsolationTest(scenarioId: string, tenantId: string): Promise<apiTypes.IsolationTestResult> {
    return this.client.request<apiTypes.IsolationTestResult>('/v1/system/isolation-test', {
      method: 'POST',
      body: JSON.stringify({ scenario_id: scenarioId, tenant_id: tenantId }),
    });
  }
}
