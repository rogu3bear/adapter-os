import { Service, ServiceManagerConfig } from '../types/service-lifecycle';
import { ServiceLifecycleManager } from './ServiceLifecycleManager';

// Service Registry - Factory for creating predefined AdapterOS services
export class ServiceRegistry {
  private manager: ServiceLifecycleManager;

  constructor(manager: ServiceLifecycleManager) {
    this.manager = manager;
  }

  async initializeDefaultServices(): Promise<void> {
    // Core Infrastructure Services
    await this.registerDatabaseService();
    await this.registerBackendServer();
    await this.registerSecurityDaemon();
    await this.registerSupervisorDaemon();

    // User Interface Services
    await this.registerUIService();

    // Inference Services
    await this.registerInferenceWorker();

    // Monitoring Services
    await this.registerTelemetryExporter();
    await this.registerMetricsCollector();
  }

  private async registerDatabaseService(): Promise<void> {
    const service: Omit<Service, 'createdAt' | 'phase' | 'health' | 'logs' | 'metrics'> = {
      id: 'database',
      name: 'Database',
      description: 'SQLite/PostgreSQL data persistence layer with connection pooling and migrations',
      version: '1.0.0',
      category: 'storage',
      status: 'uninitialized',
      dependencies: [],
      dependents: ['backend-server', 'supervisor'],
      startCommand: 'echo "Database service initialized via individual components"',
      statusCommand: 'test -f var/aos-cp.sqlite3 && echo "Database ready" || echo "Database not found"',
      lifecycle: {
        hooks: [
          {
            name: 'database-validation',
            enabled: true,
            timeout: 5000,
            retryCount: 3,
            preStart: async (service) => {
              // Validate database schema and migrations
              console.log(`Validating database for ${service.id}`);
            },
            postStart: async (service) => {
              // Run health checks and connection tests
              console.log(`Database ${service.id} started successfully`);
            }
          }
        ],
        healthChecks: [
          {
            type: 'command',
            command: 'test -f var/aos-cp.sqlite3 && echo "OK" || exit 1',
            interval: 30000, // 30 seconds
            timeout: 5000,
            retries: 3,
            backoffMultiplier: 2
          }
        ],
        configuration: {
          environment: 'development',
          autoStart: true,
          autoRestart: false, // Database doesn't auto-restart
          restartDelay: 5000,
          maxRestarts: 0,
          quarantineAfter: 5,
          quarantineDuration: 300000, // 5 minutes
          maintenanceMode: false,
          configOverrides: {}
        }
      },
      metadata: {
        storageType: 'sqlite',
        migrationVersion: '0046',
        connectionPool: {
          min: 2,
          max: 10,
          idle: 30000
        }
      },
      icon: 'Database',
      color: '#6b7280',
      priority: 100
    };

    await this.manager.registerService(service);
  }

  private async registerBackendServer(): Promise<void> {
    const service: Omit<Service, 'createdAt' | 'phase' | 'health' | 'logs' | 'metrics'> = {
      id: 'backend-server',
      name: 'Backend Server',
      description: 'AdapterOS API server with REST endpoints, GraphQL, and WebSocket support',
      version: '1.0.0',
      category: 'core',
      status: 'uninitialized',
      port: 8080,
      host: 'localhost',
      dependencies: [
        { serviceId: 'database', required: true, startupOrder: 'before', healthCheck: true }
      ],
      dependents: ['ui-frontend'],
      startCommand: 'cargo run --bin adapteros-server -- --port 8080 --host 0.0.0.0',
      stopCommand: 'pkill -f "adapteros-server.*8080"',
      statusCommand: 'pgrep -f "adapteros-server.*8080" && curl -f http://localhost:8080/api/health || exit 1',
      healthCommand: 'curl -f http://localhost:8080/api/health',
      lifecycle: {
        hooks: [
          {
            name: 'server-prep',
            enabled: true,
            timeout: 10000,
            retryCount: 2,
            preStart: async (service) => {
              // Pre-flight checks: ports, permissions, config validation
              console.log(`Preparing backend server ${service.id}`);
            },
            postStart: async (service) => {
              // Wait for server to be ready, run initial health checks
              await new Promise(resolve => setTimeout(resolve, 2000));
              console.log(`Backend server ${service.id} ready`);
            },
            onHealthCheck: async (service, status) => {
              if (status === 'critical') {
                console.warn(`Backend server ${service.id} health critical`);
              }
            }
          }
        ],
        healthChecks: [
          {
            type: 'http',
            endpoint: 'http://localhost:8080/api/health',
            interval: 15000, // 15 seconds
            timeout: 5000,
            retries: 3,
            backoffMultiplier: 1.5
          },
          {
            type: 'tcp',
            endpoint: 'localhost:8080',
            interval: 30000,
            timeout: 3000,
            retries: 2,
            backoffMultiplier: 2
          }
        ],
        configuration: {
          environment: 'development',
          autoStart: true,
          autoRestart: true,
          restartDelay: 5000,
          maxRestarts: 5,
          quarantineAfter: 10,
          quarantineDuration: 600000, // 10 minutes
          maintenanceMode: false,
          configOverrides: {
            cors: true,
            rateLimit: '1000req/min',
            authRequired: false
          }
        }
      },
      metadata: {
        endpoints: ['/api/v1', '/graphql', '/ws'],
        features: ['rest', 'graphql', 'websockets', 'sse'],
        middleware: ['cors', 'auth', 'rate-limit', 'compression']
      },
      icon: 'Server',
      color: '#3b82f6',
      priority: 90
    };

    await this.manager.registerService(service);
  }

  private async registerUIService(): Promise<void> {
    const service: Omit<Service, 'createdAt' | 'phase' | 'health' | 'logs' | 'metrics'> = {
      id: 'ui-frontend',
      name: 'UI Frontend',
      description: 'React-based web interface with real-time monitoring and control panels',
      version: '1.0.0',
      category: 'core',
      status: 'uninitialized',
      port: 3200,
      host: 'localhost',
      dependencies: [
        { serviceId: 'backend-server', required: true, startupOrder: 'after', healthCheck: true }
      ],
      dependents: [],
      startCommand: 'cd ui && pnpm dev -- --host 0.0.0.0 --port 3200',
      stopCommand: 'pkill -f "vite.*3200"',
      statusCommand: 'pgrep -f "vite.*3200" && curl -f http://localhost:3200 || exit 1',
      healthCommand: 'curl -f http://localhost:3200',
      lifecycle: {
        hooks: [
          {
            name: 'ui-validation',
            enabled: true,
            timeout: 5000,
            retryCount: 1,
            preStart: async (service) => {
              // Check if backend is accessible
              try {
                const response = await fetch('http://localhost:8080/api/health');
                if (!response.ok) {
                  throw new Error('Backend not healthy');
                }
              } catch (error) {
                throw new Error(`UI startup blocked: Backend unavailable - ${error}`);
              }
            }
          }
        ],
        healthChecks: [
          {
            type: 'http',
            endpoint: 'http://localhost:3200',
            interval: 20000,
            timeout: 5000,
            retries: 3,
            backoffMultiplier: 1.5
          }
        ],
        configuration: {
          environment: 'development',
          autoStart: true,
          autoRestart: true,
          restartDelay: 3000,
          maxRestarts: 10,
          quarantineAfter: 15,
          quarantineDuration: 300000,
          maintenanceMode: false,
          configOverrides: {
            hotReload: true,
            sourceMaps: true,
            proxyApi: true
          }
        }
      },
      metadata: {
        framework: 'React + TypeScript',
        buildTool: 'Vite',
        features: ['hot-reload', 'dev-tools', 'responsive'],
        routes: ['/dashboard', '/monitoring', '/services', '/audit']
      },
      icon: 'Monitor',
      color: '#10b981',
      priority: 70
    };

    await this.manager.registerService(service);
  }

  private async registerSupervisorDaemon(): Promise<void> {
    const service: Omit<Service, 'createdAt' | 'phase' | 'health' | 'logs' | 'metrics'> = {
      id: 'supervisor',
      name: 'Supervisor Daemon',
      description: 'Service orchestration and health monitoring with automatic recovery',
      version: '1.0.0',
      category: 'core',
      status: 'uninitialized',
      dependencies: [
        { serviceId: 'database', required: true, startupOrder: 'before', healthCheck: true }
      ],
      dependents: ['inference-worker'],
      startCommand: 'aos-supervisor --config scripts/supervisor.toml',
      stopCommand: 'pkill -f "aos-supervisor"',
      statusCommand: 'pgrep -f "aos-supervisor" && test -S /var/run/aos/supervisor.sock',
      lifecycle: {
        hooks: [
          {
            name: 'supervisor-init',
            enabled: true,
            timeout: 15000,
            retryCount: 2,
            preStart: async (service) => {
              // Validate supervisor configuration
              console.log(`Initializing supervisor ${service.id}`);
            },
            onFailure: async (service, error) => {
              console.error(`Supervisor failure: ${error.message}`);
              // Could trigger alerts or failover procedures
            }
          }
        ],
        healthChecks: [
          {
            type: 'command',
            command: 'test -S /var/run/aos/supervisor.sock && echo "OK"',
            interval: 10000,
            timeout: 3000,
            retries: 5,
            backoffMultiplier: 2
          }
        ],
        configuration: {
          environment: 'development',
          autoStart: true,
          autoRestart: true,
          restartDelay: 10000,
          maxRestarts: 3,
          quarantineAfter: 5,
          quarantineDuration: 600000,
          maintenanceMode: false,
          configOverrides: {
            healthCheckInterval: 5000,
            workerTimeout: 30000
          }
        }
      },
      metadata: {
        managedServices: ['inference-worker', 'telemetry-exporter'],
        features: ['health-monitoring', 'auto-recovery', 'load-balancing'],
        socket: '/var/run/aos/supervisor.sock'
      },
      icon: 'Shield',
      color: '#8b5cf6',
      priority: 95
    };

    await this.manager.registerService(service);
  }

  private async registerSecurityDaemon(): Promise<void> {
    const service: Omit<Service, 'createdAt' | 'phase' | 'health' | 'logs' | 'metrics'> = {
      id: 'secd',
      name: 'Security Daemon',
      description: 'Policy enforcement and secure execution environment',
      version: '1.0.0',
      category: 'core',
      status: 'uninitialized',
      dependencies: [],
      dependents: ['backend-server', 'supervisor'],
      startCommand: 'aos-secd --socket /var/run/aos-secd.sock',
      stopCommand: 'pkill -f "aos-secd"',
      statusCommand: 'pgrep -f "aos-secd" && test -S /var/run/aos-secd.sock',
      lifecycle: {
        hooks: [
          {
            name: 'security-init',
            enabled: true,
            timeout: 10000,
            retryCount: 1,
            preStart: async (service) => {
              // Security pre-flight checks
              console.log(`Initializing security daemon ${service.id}`);
            },
            onFailure: async (service, error) => {
              console.error(`Security daemon failure: ${error.message}`);
              // Critical security failure - could shutdown system
            }
          }
        ],
        healthChecks: [
          {
            type: 'command',
            command: 'test -S /var/run/aos-secd.sock && echo "OK"',
            interval: 12000,
            timeout: 3000,
            retries: 3,
            backoffMultiplier: 2
          }
        ],
        configuration: {
          environment: 'development',
          autoStart: true,
          autoRestart: true,
          restartDelay: 15000,
          maxRestarts: 2,
          quarantineAfter: 3,
          quarantineDuration: 1800000, // 30 minutes
          maintenanceMode: false,
          configOverrides: {
            strictMode: false,
            auditLogging: true
          }
        }
      },
      metadata: {
        policies: ['egress-control', 'tenant-isolation', 'resource-limits'],
        features: ['policy-enforcement', 'audit-logging', 'secure-execution'],
        socket: '/var/run/aos-secd.sock'
      },
      icon: 'Shield',
      color: '#ef4444',
      priority: 100
    };

    await this.manager.registerService(service);
  }

  private async registerInferenceWorker(): Promise<void> {
    const service: Omit<Service, 'createdAt' | 'phase' | 'health' | 'logs' | 'metrics'> = {
      id: 'inference-worker',
      name: 'Inference Worker',
      description: 'ML inference engine with LoRA routing and Metal acceleration',
      version: '1.0.0',
      category: 'inference',
      status: 'uninitialized',
      dependencies: [
        { serviceId: 'supervisor', required: true, startupOrder: 'after', healthCheck: true },
        { serviceId: 'secd', required: true, startupOrder: 'before', healthCheck: true }
      ],
      dependents: [],
      startCommand: 'cargo run --bin adapteros-cli -- serve default base-lora --socket /tmp/aos-worker.sock',
      stopCommand: 'pkill -f "adapteros-cli.*serve"',
      statusCommand: 'pgrep -f "adapteros-cli.*serve" && test -S /tmp/aos-worker.sock',
      healthCommand: 'echo "health"', // Would implement actual health check
      lifecycle: {
        hooks: [
          {
            name: 'inference-setup',
            enabled: true,
            timeout: 30000,
            retryCount: 2,
            preStart: async (service) => {
              // Model loading, GPU initialization
              console.log(`Setting up inference worker ${service.id}`);
            },
            postStart: async (service) => {
              // Warm-up inferences, validate model
              console.log(`Inference worker ${service.id} ready`);
            },
            onFailure: async (service, error) => {
              console.error(`Inference worker failure: ${error.message}`);
              // Could trigger model reloading or failover
            }
          }
        ],
        healthChecks: [
          {
            type: 'command',
            command: 'test -S /tmp/aos-worker.sock && echo "OK"',
            interval: 25000,
            timeout: 5000,
            retries: 4,
            backoffMultiplier: 1.5
          },
          {
            type: 'custom',
            interval: 60000,
            timeout: 10000,
            retries: 2,
            backoffMultiplier: 2,
            customCheck: async (service) => {
              // Custom health check: test inference capability
              try {
                // Would make a test inference request
                return 'healthy';
              } catch {
                return 'critical';
              }
            }
          }
        ],
        configuration: {
          environment: 'development',
          autoStart: false, // Start on-demand due to resource usage
          autoRestart: true,
          restartDelay: 20000,
          maxRestarts: 5,
          quarantineAfter: 8,
          quarantineDuration: 900000, // 15 minutes
          maintenanceMode: false,
          configOverrides: {
            model: 'base-lora',
            gpuAcceleration: true,
            maxBatchSize: 32,
            timeout: 30000
          }
        }
      },
      metadata: {
        model: 'Llama-2-7B',
        adapters: ['code-lang-v1', 'README_adapter'],
        acceleration: 'Metal',
        features: ['lora-routing', 'quantization', 'batch-processing']
      },
      icon: 'Zap',
      color: '#f59e0b',
      priority: 60
    };

    await this.manager.registerService(service);
  }

  private async registerTelemetryExporter(): Promise<void> {
    const service: Omit<Service, 'createdAt' | 'phase' | 'health' | 'logs' | 'metrics'> = {
      id: 'telemetry-exporter',
      name: 'Telemetry Exporter',
      description: 'Metrics collection and export to monitoring systems',
      version: '1.0.0',
      category: 'monitoring',
      status: 'uninitialized',
      dependencies: [
        { serviceId: 'database', required: false, startupOrder: 'before', healthCheck: false }
      ],
      dependents: [],
      startCommand: 'cargo run --bin adapteros-telemetry-exporter -- --port 9090',
      stopCommand: 'pkill -f "adapteros-telemetry-exporter"',
      statusCommand: 'pgrep -f "adapteros-telemetry-exporter" && curl -f http://localhost:9090/metrics || exit 1',
      lifecycle: {
        hooks: [
          {
            name: 'telemetry-init',
            enabled: true,
            timeout: 8000,
            retryCount: 2,
            preStart: async (service) => {
              console.log(`Starting telemetry exporter ${service.id}`);
            }
          }
        ],
        healthChecks: [
          {
            type: 'http',
            endpoint: 'http://localhost:9090/metrics',
            interval: 30000,
            timeout: 5000,
            retries: 3,
            backoffMultiplier: 1.5
          }
        ],
        configuration: {
          environment: 'development',
          autoStart: true,
          autoRestart: true,
          restartDelay: 10000,
          maxRestarts: 8,
          quarantineAfter: 12,
          quarantineDuration: 300000,
          maintenanceMode: false,
          configOverrides: {
            format: 'prometheus',
            interval: 15000
          }
        }
      },
      metadata: {
        exporters: ['prometheus', 'json', 'statsd'],
        metrics: ['system', 'services', 'inference', 'errors'],
        retention: '7d'
      },
      icon: 'Activity',
      color: '#06b6d4',
      priority: 50
    };

    await this.manager.registerService(service);
  }

  private async registerMetricsCollector(): Promise<void> {
    const service: Omit<Service, 'createdAt' | 'phase' | 'health' | 'logs' | 'metrics'> = {
      id: 'metrics-collector',
      name: 'Metrics Collector',
      description: 'System and application metrics collection and aggregation',
      version: '1.0.0',
      category: 'monitoring',
      status: 'uninitialized',
      dependencies: [],
      dependents: ['telemetry-exporter'],
      startCommand: 'cargo run --bin adapteros-metrics-collector',
      stopCommand: 'pkill -f "adapteros-metrics-collector"',
      statusCommand: 'pgrep -f "adapteros-metrics-collector"',
      lifecycle: {
        hooks: [
          {
            name: 'metrics-init',
            enabled: true,
            timeout: 5000,
            retryCount: 1,
            preStart: async (service) => {
              console.log(`Starting metrics collector ${service.id}`);
            }
          }
        ],
        healthChecks: [
          {
            type: 'command',
            command: 'pgrep -f "adapteros-metrics-collector" && echo "OK"',
            interval: 20000,
            timeout: 3000,
            retries: 3,
            backoffMultiplier: 2
          }
        ],
        configuration: {
          environment: 'development',
          autoStart: true,
          autoRestart: true,
          restartDelay: 8000,
          maxRestarts: 10,
          quarantineAfter: 15,
          quarantineDuration: 240000,
          maintenanceMode: false,
          configOverrides: {
            collectionInterval: 10000,
            bufferSize: 1000
          }
        }
      },
      metadata: {
        sources: ['system', 'processes', 'network', 'disk'],
        aggregation: ['avg', 'min', 'max', 'percentiles'],
        export: ['telemetry-exporter']
      },
      icon: 'BarChart3',
      color: '#84cc16',
      priority: 45
    };

    await this.manager.registerService(service);
  }

  // Utility methods
  getManager(): ServiceLifecycleManager {
    return this.manager;
  }

  async createCustomService(template: Partial<Omit<Service, 'createdAt' | 'phase' | 'health' | 'logs' | 'metrics'>>): Promise<void> {
    // Allow creating custom services with defaults
    const defaultService: Omit<Service, 'createdAt' | 'phase' | 'health' | 'logs' | 'metrics'> = {
      id: template.id || `custom-${Date.now()}`,
      name: template.name || 'Custom Service',
      description: template.description || 'Custom service',
      version: template.version || '1.0.0',
      category: template.category || 'core',
      status: 'uninitialized',
      dependencies: template.dependencies || [],
      dependents: template.dependents || [],
      startCommand: template.startCommand || 'echo "Custom service started"',
      stopCommand: template.stopCommand,
      restartCommand: template.restartCommand,
      statusCommand: template.statusCommand,
      healthCommand: template.healthCommand,
      lifecycle: template.lifecycle || {
        hooks: [],
        healthChecks: [],
        configuration: {
          environment: 'development',
          autoStart: false,
          autoRestart: false,
          restartDelay: 5000,
          maxRestarts: 3,
          quarantineAfter: 5,
          quarantineDuration: 300000,
          maintenanceMode: false,
          configOverrides: {}
        }
      },
      metadata: template.metadata || {},
      icon: template.icon || 'Settings',
      color: template.color || '#6b7280',
      priority: template.priority || 0,
      ...template
    };

    await this.manager.registerService(defaultService);
  }
}
