import {
  Service,
  ServiceStatus,
  LifecyclePhase,
  HealthStatus,
  ServiceDependency,
  LifecycleEvent,
  ServiceLogEntry,
  ServiceGroup,
  ServiceCluster,
  StartOptions,
  StopOptions,
  RestartOptions,
  ServiceManagerConfig
} from '../types/service-lifecycle';

export class ServiceLifecycleManager {
  private services: Map<string, Service> = new Map();
  private groups: Map<string, ServiceGroup> = new Map();
  private clusters: Map<string, ServiceCluster> = new Map();
  private events: LifecycleEvent[] = [];
  private config: ServiceManagerConfig;
  private healthCheckIntervals: Map<string, NodeJS.Timeout> = new Map();
  private recoveryTimeouts: Map<string, NodeJS.Timeout> = new Map();
  private persistenceTimeout?: NodeJS.Timeout;

  constructor(config: ServiceManagerConfig) {
    this.config = config;
    this.initializePersistence();
    this.initializeMonitoring();
  }

  // Service Registration and Discovery
  async registerService(service: Omit<Service, 'createdAt' | 'phase' | 'health' | 'logs' | 'metrics'>): Promise<void> {
    const fullService: Service = {
      ...service,
      createdAt: new Date(),
      phase: 'discovery',
      health: 'unknown',
      logs: [],
      metrics: {
        startCount: 0,
        stopCount: 0,
        failureCount: 0,
        uptime: 0,
        downtime: 0,
        healthCheckCount: 0,
        healthCheckFailures: 0,
        averageStartupTime: 0
      }
    };

    this.services.set(service.id, fullService);
    await this.logEvent({
      id: crypto.randomUUID(),
      timestamp: new Date(),
      serviceId: service.id,
      eventType: 'start_requested',
      phase: 'discovery',
      newStatus: 'uninitialized'
    });

    // Validate service configuration
    await this.validateService(service.id);

    // Update phase to initialized
    await this.updateServicePhase(service.id, 'validation');
  }

  async unregisterService(serviceId: string): Promise<void> {
    const service = this.services.get(serviceId);
    if (!service) return;

    // Stop service if running
    if (service.status === 'running' || service.status === 'starting') {
      await this.stopService(serviceId, { force: true });
    }

    // Clean up intervals and timeouts
    this.clearServiceIntervals(serviceId);

    this.services.delete(serviceId);
    this.persistState();
  }

  // Service Validation
  private async validateService(serviceId: string): Promise<void> {
    const service = this.services.get(serviceId);
    if (!service) throw new Error(`Service ${serviceId} not found`);

    // Validate dependencies
    for (const dep of service.dependencies) {
      if (dep.required && !this.services.has(dep.serviceId)) {
        throw new Error(`Required dependency ${dep.serviceId} not found for service ${serviceId}`);
      }
    }

    // Validate commands
    if (!service.startCommand) {
      throw new Error(`Service ${serviceId} missing start command`);
    }

    // Validate health checks
    for (const healthCheck of service.lifecycle.healthChecks) {
      if (healthCheck.interval <= 0) {
        throw new Error(`Invalid health check interval for service ${serviceId}`);
      }
    }

    await this.updateServicePhase(serviceId, 'initialization');
  }

  // Lifecycle Operations
  async startService(serviceId: string, options: StartOptions = {}): Promise<void> {
    const service = this.services.get(serviceId);
    if (!service) throw new Error(`Service ${serviceId} not found`);

    if (service.status === 'running' && !options.force) {
      throw new Error(`Service ${serviceId} is already running`);
    }

    // Check maintenance mode
    if (service.lifecycle.configuration.maintenanceMode) {
      throw new Error(`Service ${serviceId} is in maintenance mode`);
    }

    // Check quarantine
    if (service.status === 'quarantined') {
      throw new Error(`Service ${serviceId} is quarantined due to repeated failures`);
    }

    await this.logEvent({
      id: crypto.randomUUID(),
      timestamp: new Date(),
      serviceId,
      eventType: 'start_requested',
      phase: service.phase,
      previousStatus: service.status,
      newStatus: 'starting'
    });

    await this.updateServiceStatus(serviceId, 'starting');
    await this.updateServicePhase(serviceId, 'startup');

    try {
      // Execute pre-start hooks
      await this.executeLifecycleHooks(serviceId, 'preStart');

      // Start dependencies first
      if (!options.skipDependencies) {
        await this.startDependencies(serviceId);
      }

      // Execute start command
      const startTime = Date.now();
      const result = await this.executeCommand(service.startCommand);

      if (result.success) {
        await this.updateServiceStatus(serviceId, 'running');
        await this.updateServicePhase(serviceId, 'running');

        const startupTime = Date.now() - startTime;
        await this.updateServiceMetrics(serviceId, {
          startCount: service.metrics.startCount + 1,
          lastStartTime: new Date(),
          averageStartupTime: (service.metrics.averageStartupTime * service.metrics.startCount + startupTime) / (service.metrics.startCount + 1)
        });

        // Start health monitoring
        this.startHealthMonitoring(serviceId);

        // Execute post-start hooks
        await this.executeLifecycleHooks(serviceId, 'postStart');

        // Wait for health if requested
        if (options.waitForHealth) {
          await this.waitForHealthy(serviceId, options.timeout || 30000);
        }

        await this.logEvent({
          id: crypto.randomUUID(),
          timestamp: new Date(),
          serviceId,
          eventType: 'start_completed',
          phase: 'running',
          previousStatus: 'starting',
          newStatus: 'running'
        });

      } else {
        throw new Error(`Start command failed: ${result.error}`);
      }

    } catch (error) {
      await this.handleServiceFailure(serviceId, error as Error);
    }
  }

  async stopService(serviceId: string, options: StopOptions = {}): Promise<void> {
    const service = this.services.get(serviceId);
    if (!service) throw new Error(`Service ${serviceId} not found`);

    if (service.status === 'stopped' && !options.force) {
      return; // Already stopped
    }

    await this.logEvent({
      id: crypto.randomUUID(),
      timestamp: new Date(),
      serviceId,
      eventType: 'stop_requested',
      phase: service.phase,
      previousStatus: service.status,
      newStatus: 'stopping'
    });

    await this.updateServiceStatus(serviceId, 'stopping');
    await this.updateServicePhase(serviceId, 'shutdown');

    try {
      // Execute pre-stop hooks
      await this.executeLifecycleHooks(serviceId, 'preStop');

      // Stop dependents first if not skipping
      if (!options.skipDependents) {
        await this.stopDependents(serviceId);
      }

      // Stop health monitoring
      this.stopHealthMonitoring(serviceId);

      // Execute stop command
      if (service.stopCommand) {
        const result = await this.executeCommand(service.stopCommand);
        if (!result.success && !options.force) {
          throw new Error(`Stop command failed: ${result.error}`);
        }
      } else if (service.pid) {
        // Fallback: kill process
        await this.killProcess(service.pid);
      }

      await this.updateServiceStatus(serviceId, 'stopped');
      await this.updateServicePhase(serviceId, 'cleanup');

      await this.updateServiceMetrics(serviceId, {
        stopCount: service.metrics.stopCount + 1,
        lastStopTime: new Date(),
        uptime: service.startedAt ? service.metrics.uptime + (Date.now() - service.startedAt.getTime()) : service.metrics.uptime
      });

      // Execute post-stop hooks
      await this.executeLifecycleHooks(serviceId, 'postStop');

      await this.logEvent({
        id: crypto.randomUUID(),
        timestamp: new Date(),
        serviceId,
        eventType: 'stop_completed',
        phase: 'cleanup',
        previousStatus: 'stopping',
        newStatus: 'stopped'
      });

    } catch (error) {
      await this.handleServiceFailure(serviceId, error as Error);
    }
  }

  async restartService(serviceId: string, options: RestartOptions = { strategy: 'immediate' }): Promise<void> {
    await this.logEvent({
      id: crypto.randomUUID(),
      timestamp: new Date(),
      serviceId,
      eventType: 'restart_requested',
      phase: this.services.get(serviceId)?.phase || 'unknown',
      newStatus: 'stopping'
    });

    // Different restart strategies
    switch (options.strategy) {
      case 'rolling':
        await this.stopService(serviceId, options);
        await new Promise(resolve => setTimeout(resolve, options.downtime || 1000));
        await this.startService(serviceId, options);
        break;

      case 'immediate':
        await this.stopService(serviceId, { ...options, force: true });
        await this.startService(serviceId, options);
        break;

      case 'blue-green':
        // For now, fallback to immediate restart
        // Blue-green would require more complex orchestration
        await this.restartService(serviceId, { ...options, strategy: 'immediate' });
        break;
    }

    await this.logEvent({
      id: crypto.randomUUID(),
      timestamp: new Date(),
      serviceId,
      eventType: 'restart_completed',
      phase: 'running',
      newStatus: 'running'
    });
  }

  // Dependency Management
  private async startDependencies(serviceId: string): Promise<void> {
    const service = this.services.get(serviceId);
    if (!service) return;

    for (const dep of service.dependencies.filter(d => d.startupOrder === 'before')) {
      const depService = this.services.get(dep.serviceId);
      if (depService && depService.status !== 'running') {
        await this.startService(dep.serviceId);

        if (dep.healthCheck) {
          await this.waitForHealthy(dep.serviceId);
        }
      }
    }
  }

  private async stopDependents(serviceId: string): Promise<void> {
    const dependents = Array.from(this.services.values())
      .filter(s => s.dependencies.some(d => d.serviceId === serviceId))
      .map(s => s.id);

    for (const dependentId of dependents) {
      await this.stopService(dependentId);
    }
  }

  // Health Monitoring
  private startHealthMonitoring(serviceId: string): void {
    const service = this.services.get(serviceId);
    if (!service || service.lifecycle.healthChecks.length === 0) return;

    const interval = setInterval(async () => {
      await this.performHealthCheck(serviceId);
    }, Math.min(...service.lifecycle.healthChecks.map(h => h.interval)));

    this.healthCheckIntervals.set(serviceId, interval);
  }

  private stopHealthMonitoring(serviceId: string): void {
    const interval = this.healthCheckIntervals.get(serviceId);
    if (interval) {
      clearInterval(interval);
      this.healthCheckIntervals.delete(serviceId);
    }
  }

  private async performHealthCheck(serviceId: string): Promise<void> {
    const service = this.services.get(serviceId);
    if (!service) return;

    for (const healthCheck of service.lifecycle.healthChecks) {
      try {
        const status = await this.executeHealthCheck(service, healthCheck);
        const previousHealth = service.health;

        if (status !== previousHealth) {
          await this.updateServiceHealth(serviceId, status);

          await this.logEvent({
            id: crypto.randomUUID(),
            timestamp: new Date(),
            serviceId,
            eventType: 'health_changed',
            phase: service.phase,
            previousStatus: service.status,
            newStatus: service.status,
            details: { previousHealth, newHealth: status }
          });

          // Execute health change hooks
          await this.executeLifecycleHooks(serviceId, 'onHealthCheck', status);
        }

        // Update metrics
        await this.updateServiceMetrics(serviceId, {
          healthCheckCount: service.metrics.healthCheckCount + 1,
          healthCheckFailures: status === 'healthy' ? service.metrics.healthCheckFailures : service.metrics.healthCheckFailures + 1
        });

      } catch (error) {
        await this.updateServiceHealth(serviceId, 'critical');
        await this.updateServiceMetrics(serviceId, {
          healthCheckCount: service.metrics.healthCheckCount + 1,
          healthCheckFailures: service.metrics.healthCheckFailures + 1
        });
      }
    }
  }

  private async executeHealthCheck(service: Service, healthCheck: any): Promise<HealthStatus> {
    switch (healthCheck.type) {
      case 'http':
        return await this.checkHttpHealth(service, healthCheck);
      case 'tcp':
        return await this.checkTcpHealth(service, healthCheck);
      case 'command':
        return await this.checkCommandHealth(service, healthCheck);
      case 'custom':
        return await healthCheck.customCheck(service);
      default:
        return 'unknown';
    }
  }

  // Recovery and Auto-restart
  private async handleServiceFailure(serviceId: string, error: Error): Promise<void> {
    const service = this.services.get(serviceId);
    if (!service) return;

    await this.updateServiceStatus(serviceId, 'failed');
    await this.updateServiceMetrics(serviceId, {
      failureCount: service.metrics.failureCount + 1,
      lastFailureTime: new Date()
    });

    await this.logEvent({
      id: crypto.randomUUID(),
      timestamp: new Date(),
      serviceId,
      eventType: 'failure',
      phase: service.phase,
      previousStatus: service.status,
      newStatus: 'failed',
      error: error.message
    });

    // Execute failure hooks
    await this.executeLifecycleHooks(serviceId, 'onFailure', error);

    // Check if service should be quarantined
    if (this.shouldQuarantine(service)) {
      await this.quarantineService(serviceId);
      return;
    }

    // Auto-restart if enabled
    if (service.lifecycle.configuration.autoRestart && this.config.recovery.autoRestart) {
      const delay = this.calculateRestartDelay(service);
      this.scheduleRestart(serviceId, delay);
    }
  }

  private shouldQuarantine(service: Service): boolean {
    return this.config.recovery.quarantineEnabled &&
           service.metrics.failureCount >= service.lifecycle.configuration.quarantineAfter;
  }

  private calculateRestartDelay(service: Service): number {
    const baseDelay = service.lifecycle.configuration.restartDelay;
    const failureCount = service.metrics.failureCount;

    switch (this.config.recovery.backoffStrategy) {
      case 'exponential':
        return Math.min(baseDelay * Math.pow(2, failureCount - 1), this.config.recovery.maxBackoffTime);
      case 'fibonacci':
        return Math.min(this.fibonacci(failureCount) * baseDelay, this.config.recovery.maxBackoffTime);
      case 'linear':
      default:
        return Math.min(baseDelay * failureCount, this.config.recovery.maxBackoffTime);
    }
  }

  private fibonacci(n: number): number {
    if (n <= 1) return 1;
    return this.fibonacci(n - 1) + this.fibonacci(n - 2);
  }

  private scheduleRestart(serviceId: string, delay: number): void {
    const timeout = setTimeout(async () => {
      try {
        await this.startService(serviceId);
      } catch (error) {
        // Restart failed, will be handled by failure logic
      }
    }, delay);

    this.recoveryTimeouts.set(serviceId, timeout);
  }

  private async quarantineService(serviceId: string): Promise<void> {
    await this.updateServiceStatus(serviceId, 'quarantined');

    await this.logEvent({
      id: crypto.randomUUID(),
      timestamp: new Date(),
      serviceId,
      eventType: 'quarantine',
      phase: 'error',
      newStatus: 'quarantined'
    });

    // Schedule unquarantine after duration
    setTimeout(async () => {
      const service = this.services.get(serviceId);
      if (service?.status === 'quarantined') {
        await this.updateServiceStatus(serviceId, 'stopped');
        await this.logEvent({
          id: crypto.randomUUID(),
          timestamp: new Date(),
          serviceId,
          eventType: 'recovery',
          phase: 'cleanup',
          newStatus: 'stopped'
        });
      }
    }, this.config.recovery.quarantineDuration);
  }

  // Lifecycle Hooks
  private async executeLifecycleHooks(serviceId: string, hookType: string, ...args: any[]): Promise<void> {
    const service = this.services.get(serviceId);
    if (!service) return;

    for (const hook of service.lifecycle.hooks.filter(h => h.enabled)) {
      try {
        const hookFunction = hook[hookType];
        if (hookFunction) {
          await Promise.race([
            hookFunction(service, ...args),
            new Promise((_, reject) =>
              setTimeout(() => reject(new Error(`Hook timeout: ${hook.name}`)), hook.timeout)
            )
          ]);
        }
      } catch (error) {
        await this.logToService(serviceId, {
          id: crypto.randomUUID(),
          timestamp: new Date(),
          level: 'error',
          message: `Lifecycle hook ${hookType} failed: ${error}`,
          component: 'lifecycle',
          source: 'system'
        });
      }
    }
  }

  // Utility Methods
  private async updateServiceStatus(serviceId: string, status: ServiceStatus): Promise<void> {
    const service = this.services.get(serviceId);
    if (!service) return;

    const updatedService = { ...service, status };
    this.services.set(serviceId, updatedService);
    this.persistState();
  }

  private async updateServicePhase(serviceId: string, phase: LifecyclePhase): Promise<void> {
    const service = this.services.get(serviceId);
    if (!service) return;

    const updatedService = { ...service, phase };
    this.services.set(serviceId, updatedService);
    this.persistState();
  }

  private async updateServiceHealth(serviceId: string, health: HealthStatus): Promise<void> {
    const service = this.services.get(serviceId);
    if (!service) return;

    const updatedService = { ...service, health, lastHealthCheck: new Date() };
    this.services.set(serviceId, updatedService);
    this.persistState();
  }

  private async updateServiceMetrics(serviceId: string, metrics: Partial<Service['metrics']>): Promise<void> {
    const service = this.services.get(serviceId);
    if (!service) return;

    const updatedService = {
      ...service,
      metrics: { ...service.metrics, ...metrics }
    };
    this.services.set(serviceId, updatedService);
    this.persistState();
  }

  private async logEvent(event: LifecycleEvent): Promise<void> {
    this.events.push(event);

    // Keep only recent events (last 1000)
    if (this.events.length > 1000) {
      this.events = this.events.slice(-1000);
    }

    this.persistState();
  }

  private async logToService(serviceId: string, entry: ServiceLogEntry): Promise<void> {
    const service = this.services.get(serviceId);
    if (!service) return;

    const updatedService = {
      ...service,
      logs: [...service.logs.slice(-999), entry] // Keep last 1000 logs
    };
    this.services.set(serviceId, updatedService);
    this.persistState();
  }

  // Command Execution (would call backend API)
  private async executeCommand(command: string): Promise<{ success: boolean; error?: string }> {
    // This would make an API call to the backend server
    try {
      const response = await fetch('/api/services/execute', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ command })
      });
      return await response.json();
    } catch (error) {
      return { success: false, error: error.message };
    }
  }

  private async killProcess(pid: number): Promise<void> {
    await this.executeCommand(`kill -TERM ${pid}`);
  }

  // Health Check Implementations
  private async checkHttpHealth(service: Service, healthCheck: any): Promise<HealthStatus> {
    try {
      const url = healthCheck.endpoint || `http://localhost:${service.port}/health`;
      const response = await fetch(url, { timeout: healthCheck.timeout });
      return response.ok ? 'healthy' : 'unhealthy';
    } catch {
      return 'critical';
    }
  }

  private async checkTcpHealth(service: Service, healthCheck: any): Promise<HealthStatus> {
    // Would implement TCP connection check
    return 'healthy'; // Placeholder
  }

  private async checkCommandHealth(service: Service, healthCheck: any): Promise<HealthStatus> {
    const result = await this.executeCommand(healthCheck.command || service.healthCommand);
    return result.success ? 'healthy' : 'critical';
  }

  // Persistence
  private initializePersistence(): void {
    if (this.config.persistence.enabled) {
      // Load persisted state
      this.loadPersistedState();

      // Set up periodic backup
      this.persistenceTimeout = setInterval(() => {
        this.persistState();
      }, this.config.persistence.backupInterval);
    }
  }

  private loadPersistedState(): void {
    try {
      const data = localStorage.getItem('service-lifecycle-state');
      if (data) {
        const state = JSON.parse(data);
        // Restore services, events, etc.
        this.services = new Map(state.services || []);
        this.events = state.events || [];
      }
    } catch (error) {
      console.error('Failed to load persisted state:', error);
    }
  }

  private persistState(): void {
    if (!this.config.persistence.enabled) return;

    try {
      const state = {
        services: Array.from(this.services.entries()),
        events: this.events.slice(-100), // Only persist recent events
        timestamp: new Date().toISOString()
      };
      localStorage.setItem('service-lifecycle-state', JSON.stringify(state));
    } catch (error) {
      console.error('Failed to persist state:', error);
    }
  }

  // Monitoring and Metrics
  private initializeMonitoring(): void {
    if (this.config.monitoring.enabled) {
      // Set up monitoring intervals
      setInterval(() => {
        this.performSystemHealthCheck();
      }, this.config.monitoring.metricsInterval);
    }
  }

  private async performSystemHealthCheck(): Promise<void> {
    // Check overall system health
    const services = Array.from(this.services.values());
    const runningServices = services.filter(s => s.status === 'running').length;
    const failedServices = services.filter(s => s.status === 'failed').length;

    // Alert thresholds
    if (failedServices >= this.config.monitoring.alertThresholds.maxFailures) {
      console.warn(`High failure rate: ${failedServices} services failed`);
    }
  }

  // Cleanup
  private clearServiceIntervals(serviceId: string): void {
    this.stopHealthMonitoring(serviceId);

    const recoveryTimeout = this.recoveryTimeouts.get(serviceId);
    if (recoveryTimeout) {
      clearTimeout(recoveryTimeout);
      this.recoveryTimeouts.delete(serviceId);
    }
  }

  // Public API
  getService(serviceId: string): Service | undefined {
    return this.services.get(serviceId);
  }

  getAllServices(): Service[] {
    return Array.from(this.services.values());
  }

  getServiceEvents(serviceId?: string, limit = 100): LifecycleEvent[] {
    let events = this.events;
    if (serviceId) {
      events = events.filter(e => e.serviceId === serviceId);
    }
    return events.slice(-limit);
  }

  async startAllServices(): Promise<void> {
    const services = Array.from(this.services.values())
      .filter(s => s.lifecycle.configuration.autoStart)
      .sort((a, b) => a.priority - b.priority);

    for (const service of services) {
      try {
        await this.startService(service.id);
      } catch (error) {
        console.error(`Failed to start service ${service.id}:`, error);
      }
    }
  }

  async stopAllServices(): Promise<void> {
    const services = Array.from(this.services.values())
      .filter(s => s.status === 'running')
      .sort((a, b) => b.priority - a.priority); // Stop high priority first

    for (const service of services) {
      try {
        await this.stopService(service.id);
      } catch (error) {
        console.error(`Failed to stop service ${service.id}:`, error);
      }
    }
  }

  async destroy(): Promise<void> {
    await this.stopAllServices();

    if (this.persistenceTimeout) {
      clearInterval(this.persistenceTimeout);
    }

    // Clear all intervals and timeouts
    for (const interval of this.healthCheckIntervals.values()) {
      clearInterval(interval);
    }
    for (const timeout of this.recoveryTimeouts.values()) {
      clearTimeout(timeout);
    }
  }
}
