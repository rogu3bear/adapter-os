/**
 * System State Types
 *
 * Ground truth system state types providing hierarchical visibility:
 * Node -> Tenant -> Stack -> Adapter
 *
 * All timestamps use RFC3339 format for consistency.
 */

/** Data origin for traceability */
export interface StateOrigin {
  /** Unique node identifier */
  node_id: string;
  /** Node hostname */
  hostname: string;
  /** Federation role (primary, replica, standalone) */
  federation_role: string;
}

/** Service health status */
export type ServiceHealthStatus = 'healthy' | 'degraded' | 'unhealthy' | 'unknown';

/** Service health within a node */
export interface ServiceState {
  /** Service name (e.g., "api_server", "lifecycle_manager") */
  name: string;
  /** Current health status */
  status: ServiceHealthStatus;
  /** RFC3339 timestamp of last health check */
  last_check: string;
}

/** Node-level state */
export interface NodeState {
  /** System uptime in seconds */
  uptime_seconds: number;
  /** Current CPU usage percentage */
  cpu_usage_percent: number;
  /** Current memory usage percentage */
  memory_usage_percent: number;
  /** Whether GPU is available on this node */
  gpu_available: boolean;
  /** Whether ANE (Apple Neural Engine) is available */
  ane_available: boolean;
  /** Health status of critical services */
  services: ServiceState[];
}

/** Adapter lifecycle state */
export type AdapterLifecycleState = 'unloaded' | 'cold' | 'warm' | 'hot' | 'resident';

/** Adapter summary within a stack */
export interface AdapterSummary {
  /** Adapter unique identifier */
  adapter_id: string;
  /** Adapter name */
  name: string;
  /** Current lifecycle state */
  state: AdapterLifecycleState;
  /** Memory usage in MB */
  memory_mb: number;
  /** RFC3339 timestamp of last access (if known) */
  last_access?: string;
  /** Total activation count */
  activation_count: number;
  /** Whether adapter is pinned (resident) */
  pinned: boolean;
}

/** Stack summary with nested adapters */
export interface StackSummary {
  /** Stack unique identifier */
  stack_id: string;
  /** Stack name (e.g., "prod/main/inference/v3") */
  name: string;
  /** Whether this stack is currently active */
  is_active: boolean;
  /** Number of adapters in this stack */
  adapter_count: number;
  /** Adapters in this stack */
  adapters?: AdapterSummary[];
}

/** Tenant-level state with nested stacks */
export interface TenantState {
  /** Tenant unique identifier */
  tenant_id: string;
  /** Tenant display name */
  name: string;
  /** Tenant status (active, paused, archived) */
  status: string;
  /** Total memory usage across all adapters in MB */
  memory_usage_mb: number;
  /** Total number of adapters for this tenant */
  adapter_count: number;
  /** Stacks belonging to this tenant */
  stacks: StackSummary[];
}

/** Memory pressure level */
export type MemoryPressureLevel = 'low' | 'medium' | 'high' | 'critical';

/** ANE-specific memory state (Apple Silicon only) */
export interface AneMemoryState {
  /** Allocated ANE memory in MB */
  allocated_mb: number;
  /** Used ANE memory in MB */
  used_mb: number;
  /** Available ANE memory in MB */
  available_mb: number;
  /** ANE memory usage percentage */
  usage_percent: number;
}

/** Adapter memory summary for top-N display */
export interface AdapterMemorySummary {
  /** Adapter unique identifier */
  adapter_id: string;
  /** Adapter name */
  name: string;
  /** Memory usage in MB */
  memory_mb: number;
  /** Current lifecycle state */
  state: AdapterLifecycleState;
  /** Tenant that owns this adapter */
  tenant_id: string;
}

/** Memory state summary */
export interface MemoryState {
  /** Total system memory in MB */
  total_mb: number;
  /** Used memory in MB */
  used_mb: number;
  /** Available memory in MB */
  available_mb: number;
  /** Headroom percentage (policy requires >= 15%) */
  headroom_percent: number;
  /** Current pressure level */
  pressure_level: MemoryPressureLevel;
  /** ANE-specific memory state (Apple Silicon only) */
  ane?: AneMemoryState;
  /** Top adapters by memory consumption */
  top_adapters: AdapterMemorySummary[];
}

/** Ground truth system state response */
export interface SystemStateResponse {
  schema_version: string;
  /** RFC3339 timestamp when this response was generated */
  timestamp: string;
  /** Origin node that produced this data */
  origin: StateOrigin;
  /** Node-level state (hardware, services) */
  node: NodeState;
  /** Tenant states with nested stacks and adapters */
  tenants: TenantState[];
  /** Memory state summary */
  memory: MemoryState;
}

/** Query parameters for system state endpoint */
export interface SystemStateQuery {
  /** Include adapter details in stack responses (default: true) */
  include_adapters?: boolean;
  /** Number of top adapters by memory to include (default: 10) */
  top_adapters?: number;
  /** Filter to specific tenant */
  tenant_id?: string;
}
