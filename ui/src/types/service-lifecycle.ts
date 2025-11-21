// Service Lifecycle Management Types
// AdapterOS Service Management Panel - Full Lifecycle Support

export type ServiceStatus =
  | 'uninitialized'    // Service not yet initialized
  | 'initializing'     // Service is being initialized
  | 'initialized'      // Service initialized but not started
  | 'starting'         // Service is starting up
  | 'running'          // Service is running normally
  | 'degraded'         // Service is running but degraded
  | 'stopping'         // Service is shutting down
  | 'stopped'          // Service is stopped
  | 'failed'           // Service failed to start or crashed
  | 'maintenance'      // Service is in maintenance mode
  | 'quarantined';     // Service quarantined due to repeated failures

export type LifecyclePhase =
  | 'discovery'        // Service discovery and registration
  | 'validation'       // Configuration and dependency validation
  | 'initialization'   // Service initialization
  | 'startup'          // Service startup sequence
  | 'running'          // Normal operation
  | 'shutdown'         // Graceful shutdown
  | 'cleanup'          // Resource cleanup
  | 'error'            // Error state
  | 'unknown';         // Unknown phase

export type HealthStatus =
  | 'unknown'          // Health status not yet determined
  | 'healthy'          // Service is healthy
  | 'unhealthy'        // Service is unhealthy
  | 'critical';        // Service is critically unhealthy

export interface ServiceDependency {
  serviceId: string;
  required: boolean;              // Is this dependency mandatory?
  startupOrder: 'before' | 'after'; // When should this dependency start?
  healthCheck?: boolean;          // Should we wait for dependency health?
}

export interface HealthCheck {
  type: 'http' | 'tcp' | 'command' | 'custom';
  endpoint?: string;              // For HTTP/TCP checks
  command?: string;               // For command-based checks
  interval: number;               // Check interval in milliseconds
  timeout: number;                // Check timeout in milliseconds
  retries: number;                // Number of retries before marking unhealthy
  backoffMultiplier: number;      // Exponential backoff multiplier
  customCheck?: (service: Service) => Promise<HealthStatus>;
}

export interface LifecycleHook {
  name: string;
  enabled: boolean;
  timeout: number;                // Hook timeout in milliseconds
  retryCount: number;             // Number of retries on failure
  preStart?: (service: Service) => Promise<void>;
  postStart?: (service: Service) => Promise<void>;
  preStop?: (service: Service) => Promise<void>;
  postStop?: (service: Service) => Promise<void>;
  onHealthCheck?: (service: Service, status: HealthStatus) => Promise<void>;
  onFailure?: (service: Service, error: Error) => Promise<void>;
  onRecovery?: (service: Service) => Promise<void>;
}

export interface ServiceMetrics {
  startCount: number;             // Total number of starts
  stopCount: number;              // Total number of stops
  failureCount: number;           // Total number of failures
  uptime: number;                 // Total uptime in milliseconds
  downtime: number;               // Total downtime in milliseconds
  lastStartTime?: Date;           // Last successful start time
  lastStopTime?: Date;            // Last stop time
  lastFailureTime?: Date;         // Last failure time
  healthCheckCount: number;       // Total health checks performed
  healthCheckFailures: number;    // Number of failed health checks
  averageStartupTime: number;     // Average startup time in milliseconds
}

export interface ServiceConfiguration {
  environment: 'development' | 'staging' | 'production';
  autoStart: boolean;             // Should service start automatically?
  autoRestart: boolean;           // Should service restart on failure?
  restartDelay: number;           // Delay before restart in milliseconds
  maxRestarts: number;            // Maximum restart attempts
  quarantineAfter: number;        // Quarantine after N failures
  quarantineDuration: number;     // Quarantine duration in milliseconds
  maintenanceMode: boolean;       // Is service in maintenance mode?
  configOverrides: Record<string, any>; // Environment-specific config
}

export interface Service {
  // Core Identity
  id: string;
  name: string;
  description: string;
  version: string;
  category: 'core' | 'inference' | 'monitoring' | 'storage' | 'networking';

  // Status and Lifecycle
  status: ServiceStatus;
  phase: LifecyclePhase;
  health: HealthStatus;
  pid?: number;
  port?: number;
  host?: string;

  // Timing
  createdAt: Date;
  startedAt?: Date;
  stoppedAt?: Date;
  lastHealthCheck?: Date;

  // Dependencies and Relationships
  dependencies: ServiceDependency[];
  dependents: string[];            // Services that depend on this one

  // Commands and Configuration
  startCommand: string;
  stopCommand?: string;
  restartCommand?: string;
  statusCommand?: string;
  healthCommand?: string;

  // Lifecycle Management
  lifecycle: {
    hooks: LifecycleHook[];
    healthChecks: HealthCheck[];
    configuration: ServiceConfiguration;
  };

  // State and Data
  logs: ServiceLogEntry[];
  metrics: ServiceMetrics;
  metadata: Record<string, any>;   // Additional service-specific data

  // UI Configuration
  icon: string;                    // Icon identifier
  color: string;                   // Theme color
  priority: number;                // Display priority (higher = more important)
}

export interface ServiceLogEntry {
  id: string;
  timestamp: Date;
  level: 'trace' | 'debug' | 'info' | 'warn' | 'error' | 'fatal';
  message: string;
  component?: string;
  metadata?: Record<string, any>;
  source: 'system' | 'service' | 'lifecycle' | 'health' | 'user';
}

export interface LifecycleEvent {
  id: string;
  timestamp: Date;
  serviceId: string;
  eventType: 'start_requested' | 'start_completed' | 'stop_requested' | 'stop_completed' |
             'restart_requested' | 'restart_completed' | 'health_check' | 'health_changed' |
             'failure' | 'recovery' | 'quarantine' | 'maintenance';
  phase: LifecyclePhase;
  previousStatus?: ServiceStatus;
  newStatus: ServiceStatus;
  details?: Record<string, any>;
  error?: string;
}

export interface ServiceGroup {
  id: string;
  name: string;
  description: string;
  services: string[];              // Service IDs in this group
  startupOrder: string[];          // Order to start services in this group
  shutdownOrder: string[];         // Order to stop services in this group
  autoStart: boolean;              // Should this group start automatically?
  parallelStartup: boolean;        // Can services start in parallel?
}

export interface ServiceCluster {
  id: string;
  name: string;
  description: string;
  groups: ServiceGroup[];
  globalConfig: {
    environment: string;
    autoStart: boolean;
    healthCheckInterval: number;
    failureThreshold: number;
    quarantineEnabled: boolean;
  };
}

// Service Control Operations
export interface StartOptions {
  waitForHealth?: boolean;         // Wait for service to be healthy
  timeout?: number;                // Startup timeout
  skipDependencies?: boolean;      // Skip dependency checks
  force?: boolean;                 // Force start even if already running
}

export interface StopOptions {
  graceful?: boolean;              // Attempt graceful shutdown
  timeout?: number;                // Shutdown timeout
  force?: boolean;                 // Force kill if graceful fails
  skipDependents?: boolean;        // Don't stop dependent services
}

export interface RestartOptions extends StartOptions, StopOptions {
  strategy: 'rolling' | 'immediate' | 'blue-green';
  downtime?: number;               // Acceptable downtime during restart
}

// Service Manager Configuration
export interface ServiceManagerConfig {
  persistence: {
    enabled: boolean;
    storagePath: string;
    backupInterval: number;        // Backup state every N milliseconds
    maxBackups: number;
  };
  monitoring: {
    enabled: boolean;
    metricsInterval: number;
    alertThresholds: {
      maxFailures: number;
      maxDowntime: number;
      healthCheckFailureRate: number;
    };
  };
  recovery: {
    autoRestart: boolean;
    backoffStrategy: 'linear' | 'exponential' | 'fibonacci';
    maxBackoffTime: number;
    quarantineEnabled: boolean;
    quarantineDuration: number;
  };
  security: {
    requireAuth: boolean;
    auditLogging: boolean;
    commandValidation: boolean;
    resourceLimits: {
      maxMemory: number;
      maxCpu: number;
      maxProcesses: number;
    };
  };
}
