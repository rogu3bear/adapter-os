/**
 * Plugins service - handles plugin management, status, and configuration.
 */

import type { ApiClient } from '@/api/client';
import * as pluginTypes from '@/api/plugin-types';
import { logger } from '@/utils/logger';

export class PluginsService {
  constructor(private client: ApiClient) {}

  /**
   * Retrieves all plugins registered in the system with their current status.
   *
   * @returns List of plugins with counts
   */
  async listPlugins(): Promise<pluginTypes.ListPluginsResponse> {
    logger.info('Listing plugins', {
      component: 'PluginsService',
      operation: 'listPlugins',
    });
    return this.client.request<pluginTypes.ListPluginsResponse>('/v1/plugins');
  }

  /**
   * Get plugin details and status
   *
   * Retrieves detailed information about a specific plugin including
   * its current status, enabled tenants, and any error state.
   *
   * @param pluginId - Unique plugin identifier (name)
   * @returns Plugin details with status information
   */
  async getPlugin(pluginId: string): Promise<pluginTypes.PluginStatusResponse> {
    logger.info('Getting plugin details', {
      component: 'PluginsService',
      operation: 'getPlugin',
      pluginId,
    });
    return this.client.request<pluginTypes.PluginStatusResponse>(`/v1/plugins/${encodeURIComponent(pluginId)}`);
  }

  /**
   * Get plugin status (alias for getPlugin)
   *
   * @param pluginId - Unique plugin identifier (name)
   * @returns Plugin status information
   */
  async getPluginStatus(pluginId: string): Promise<pluginTypes.PluginStatusResponse> {
    return this.getPlugin(pluginId);
  }

  /**
   * Enable a plugin
   *
   * Activates a plugin for the specified tenants or globally.
   * Requires appropriate permissions (typically Admin or Operator role).
   *
   * @param pluginId - Unique plugin identifier (name)
   * @param options - Optional enable configuration (tenant_ids, reason, config)
   * @returns Enable operation result
   */
  async enablePlugin(
    pluginId: string,
    options?: pluginTypes.EnablePluginRequest
  ): Promise<pluginTypes.EnablePluginResponse> {
    logger.info('Enabling plugin', {
      component: 'PluginsService',
      operation: 'enablePlugin',
      pluginId,
      tenantIds: options?.tenant_ids,
    });
    return this.client.request<pluginTypes.EnablePluginResponse>(
      `/v1/plugins/${encodeURIComponent(pluginId)}/enable`,
      {
        method: 'POST',
        body: JSON.stringify(options || {}),
      }
    );
  }

  /**
   * Disable a plugin
   *
   * Deactivates a plugin for the specified tenants or globally.
   * Requires appropriate permissions (typically Admin or Operator role).
   *
   * @param pluginId - Unique plugin identifier (name)
   * @param options - Optional disable configuration (tenant_ids, reason, force)
   * @returns Disable operation result with any warnings
   */
  async disablePlugin(
    pluginId: string,
    options?: pluginTypes.DisablePluginRequest
  ): Promise<pluginTypes.DisablePluginResponse> {
    logger.info('Disabling plugin', {
      component: 'PluginsService',
      operation: 'disablePlugin',
      pluginId,
      tenantIds: options?.tenant_ids,
      force: options?.force,
    });
    return this.client.request<pluginTypes.DisablePluginResponse>(
      `/v1/plugins/${encodeURIComponent(pluginId)}/disable`,
      {
        method: 'POST',
        body: JSON.stringify(options || {}),
      }
    );
  }

  /**
   * Get plugin configuration
   *
   * Retrieves the configuration for a specific plugin from the database.
   *
   * @param pluginId - Unique plugin identifier (name)
   * @returns Plugin configuration or null if not configured
   */
  async getPluginConfig(pluginId: string): Promise<pluginTypes.GetPluginConfigResponse> {
    logger.info('Getting plugin configuration', {
      component: 'PluginsService',
      operation: 'getPluginConfig',
      pluginId,
    });
    return this.client.request<pluginTypes.GetPluginConfigResponse>(
      `/v1/plugins/${encodeURIComponent(pluginId)}/config`
    );
  }

  /**
   * Update plugin configuration
   *
   * Updates the configuration JSON and/or enabled status for a plugin.
   *
   * @param pluginId - Unique plugin identifier (name)
   * @param config - Configuration update request
   * @returns Updated plugin configuration
   */
  async updatePluginConfig(
    pluginId: string,
    config: pluginTypes.UpdatePluginConfigRequest
  ): Promise<pluginTypes.UpdatePluginConfigResponse> {
    logger.info('Updating plugin configuration', {
      component: 'PluginsService',
      operation: 'updatePluginConfig',
      pluginId,
      hasConfig: !!config.config_json,
      enabled: config.enabled,
    });
    return this.client.request<pluginTypes.UpdatePluginConfigResponse>(
      `/v1/plugins/${encodeURIComponent(pluginId)}/config`,
      {
        method: 'PUT',
        body: JSON.stringify(config),
      }
    );
  }
}
