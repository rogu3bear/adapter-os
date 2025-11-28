/**
 * Plugin Management Types
 *
 * Type definitions for plugin-related API requests and responses.
 * Provides type safety for plugin enablement, disablement, and status tracking.
 *
 * # Citations
 * - CONTRIBUTING.md: "Use TypeScript for UI code"
 * - docs/RBAC.md: "Plugin management requires appropriate permissions"
 */

/**
 * Plugin configuration and metadata
 */
export interface PluginInfo {
  /** Unique plugin identifier */
  name: string;

  /** Human-readable plugin name */
  display_name: string;

  /** Plugin description */
  description: string;

  /** Current plugin version */
  version: string;

  /** Plugin author */
  author?: string;

  /** Plugin status: enabled or disabled */
  status: 'enabled' | 'disabled';

  /** Tenants that have this plugin enabled */
  enabled_tenants?: string[];

  /** Optional metadata about the plugin */
  metadata?: Record<string, unknown>;

  /** Timestamp when plugin was installed */
  installed_at?: string;

  /** Timestamp when plugin status last changed */
  last_updated?: string;
}

/**
 * Response from GET /v1/plugins/:name
 */
export interface PluginStatusResponse {
  /** Plugin details */
  plugin: PluginInfo;

  /** Plugin is currently active */
  is_active: boolean;

  /** Number of active instances */
  active_instances?: number;

  /** Optional error message if plugin has issues */
  error?: string;
}

/**
 * Request body for enabling a plugin
 * POST /v1/plugins/:name/enable
 */
export interface EnablePluginRequest {
  /** Optional list of tenant IDs to enable plugin for (if omitted, enables globally) */
  tenant_ids?: string[];

  /** Optional reason for enabling */
  reason?: string;

  /** Optional additional configuration */
  config?: Record<string, unknown>;
}

/**
 * Response from POST /v1/plugins/:name/enable
 */
export interface EnablePluginResponse {
  /** Plugin that was enabled */
  plugin: PluginInfo;

  /** Success message */
  message: string;

  /** Timestamp when operation completed */
  timestamp: string;

  /** List of affected tenants */
  affected_tenants?: string[];
}

/**
 * Request body for disabling a plugin
 * POST /v1/plugins/:name/disable
 */
export interface DisablePluginRequest {
  /** Optional list of tenant IDs to disable plugin for (if omitted, disables globally) */
  tenant_ids?: string[];

  /** Optional reason for disabling */
  reason?: string;

  /** Force disable even if plugin has active dependencies */
  force?: boolean;
}

/**
 * Response from POST /v1/plugins/:name/disable
 */
export interface DisablePluginResponse {
  /** Plugin that was disabled */
  plugin: PluginInfo;

  /** Success message */
  message: string;

  /** Timestamp when operation completed */
  timestamp: string;

  /** List of affected tenants */
  affected_tenants?: string[];

  /** Any warnings about dependencies or active instances */
  warnings?: string[];
}

/**
 * Response from GET /v1/plugins
 */
export interface ListPluginsResponse {
  /** Array of installed plugins */
  plugins: PluginInfo[];

  /** Total number of plugins */
  total: number;

  /** Number of enabled plugins */
  enabled_count: number;

  /** Number of disabled plugins */
  disabled_count: number;
}

/**
 * Plugin configuration from database
 */
export interface PluginConfigRecord {
  /** Unique configuration ID */
  id: string;

  /** Plugin name */
  plugin_name: string;

  /** Whether plugin is globally enabled */
  enabled: boolean;

  /** JSON configuration string */
  config_json: string | null;

  /** Creation timestamp */
  created_at: string;

  /** Last update timestamp */
  updated_at: string;
}

/**
 * Request body for updating plugin configuration
 * PUT/PATCH /v1/plugins/:name/config
 */
export interface UpdatePluginConfigRequest {
  /** JSON configuration (will be stored as string) */
  config_json: string | null;

  /** Optionally update enabled status */
  enabled?: boolean;
}

/**
 * Response from GET /v1/plugins/:name/config
 */
export interface GetPluginConfigResponse {
  /** Plugin configuration */
  config: PluginConfigRecord | null;
}

/**
 * Response from PUT/PATCH /v1/plugins/:name/config
 */
export interface UpdatePluginConfigResponse {
  /** Updated plugin configuration */
  config: PluginConfigRecord;

  /** Success message */
  message: string;

  /** Timestamp when operation completed */
  timestamp: string;
}
