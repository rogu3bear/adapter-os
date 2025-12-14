import React, { useState, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Separator } from './ui/separator';
import { ScrollArea } from './ui/scroll-area';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';
import { Alert, AlertDescription } from './ui/alert';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from './ui/alert-dialog';
import { Input } from './ui/input';
import {
  Play,
  Square,
  RefreshCw,
  AlertTriangle,
  CheckCircle,
  Clock,
  Server,
  Database,
  Monitor,
  Shield,
  Zap,
  Activity,
  Terminal,
  Brain,
  Settings,
  Key,
  X
} from 'lucide-react';
import { ServiceCard } from './ServiceCard';
import { TerminalOutput } from './TerminalOutput';
import PromptOrchestrationPanel from './PromptOrchestrationPanel';
import { AuthenticationSettings } from './AuthenticationSettings';
import { logger, toError } from '@/utils/logger';
import apiClient from '@/api/client';
import { useServiceStatus } from '@/hooks/system/useServiceStatus';

// Simple service interface
interface SimpleService {
  id: string;
  name: string;
  status: 'running' | 'stopped' | 'starting' | 'stopping' | 'error';
  port?: number;
  pid?: number;
  startTime?: string;
  category: string;
  essential?: boolean;
  dependencies?: string[];
  startupOrder?: number;
  logs: string[];
}

export default function ServicePanel() {
  const [selectedService, setSelectedService] = useState<SimpleService | null>(null);
  const [essentialOperation, setEssentialOperation] = useState<'idle' | 'starting' | 'stopping'>('idle');

  // Service-level loading state
  const [serviceOperations, setServiceOperations] = useState<Record<string, 'starting' | 'stopping' | 'restarting' | null>>({});

  // Notification state
  const [notification, setNotification] = useState<{ type: 'success' | 'error'; message: string } | null>(null);

  // Confirmation dialogs
  const [stopConfirmation, setStopConfirmation] = useState<SimpleService | null>(null);
  const [stopAllConfirmation, setStopAllConfirmation] = useState(false);
  const [stopAllConfirmText, setStopAllConfirmText] = useState('');

  // Use shared service status hook (handles 404s silently, deduplicates polling)
  const { status: serviceStatusData, isLoading, refetch: loadServices } = useServiceStatus();

  // Map status data to SimpleService format, applying optimistic overrides from serviceOperations
  const services: SimpleService[] = React.useMemo(() => {
    if (!serviceStatusData?.services) return [];
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    return serviceStatusData.services.map((s: any) => {
      const operation = serviceOperations[s.id];
      // Apply optimistic status override if there's an active operation
      let status = s.state as 'running' | 'stopped' | 'starting' | 'stopping' | 'error';
      if (operation === 'starting') status = 'starting';
      else if (operation === 'stopping') status = 'stopping';

      return {
        id: s.id,
        name: s.name,
        status,
        port: s.port,
        pid: s.pid,
        startTime: s.start_time,
        category: s.category || 'core',
        essential: s.essential,
        dependencies: s.dependencies,
        startupOrder: s.startup_order,
        logs: s.logs || [],
      };
    });
  }, [serviceStatusData, serviceOperations]);

  // Calculate global status from services
  const globalStatus = React.useMemo((): 'checking' | 'healthy' | 'warning' | 'error' => {
    if (isLoading && services.length === 0) return 'checking';
    if (services.length === 0) return 'error';
    
    const running = services.filter((s) => s.status === 'running').length;
    const total = services.length;

    if (running === total) return 'healthy';
    if (running >= total * 0.5) return 'warning';
    return 'error';
  }, [services, isLoading]);

  // Filter essential services
  const essentialServices = React.useMemo(() => {
    return services.filter(s => s.essential).map(s => ({
      id: s.id,
      name: s.name,
      status: s.status,
    }));
  }, [services]);

  // Helper to show notification with auto-dismiss
  const showNotification = useCallback((type: 'success' | 'error', message: string) => {
    setNotification({ type, message });
    setTimeout(() => setNotification(null), 5000); // Auto-dismiss after 5s
  }, []);

  // Start a service
  const handleStartService = async (service: SimpleService) => {
    logger.info('Starting service', {
      component: 'ServicePanel',
      serviceId: service.id,
    });

    // Set optimistic loading state (status override applied in services useMemo)
    setServiceOperations(prev => ({ ...prev, [service.id]: 'starting' }));

    try {
      const result = await apiClient.startService(service.id);

      logger.info('Service started successfully', {
        component: 'ServicePanel',
        serviceId: service.id,
        message: result.message,
      });

      showNotification('success', `Service "${service.name}" started successfully`);
      // Service status will update via the shared polling hook
    } catch (error) {
      logger.error('Failed to start service', {
        component: 'ServicePanel',
        serviceId: service.id,
        error: toError(error),
      });

      showNotification('error', `Failed to start service "${service.name}": ${toError(error).message}`);
      // Service status will revert via the shared polling hook
    } finally {
      setServiceOperations(prev => ({ ...prev, [service.id]: null }));
    }
  };

  // Stop a service (with confirmation)
  const handleStopService = async (service: SimpleService) => {
    // Show confirmation dialog
    setStopConfirmation(service);
  };

  const confirmStopService = async () => {
    if (!stopConfirmation) return;

    const service = stopConfirmation;
    setStopConfirmation(null);

    logger.info('Stopping service', {
      component: 'ServicePanel',
      serviceId: service.id,
    });

    // Set loading state (status override applied in services useMemo)
    setServiceOperations(prev => ({ ...prev, [service.id]: 'stopping' }));

    try {
      const result = await apiClient.stopService(service.id);

      logger.info('Service stopped successfully', {
        component: 'ServicePanel',
        serviceId: service.id,
        message: result.message,
      });

      showNotification('success', `Service "${service.name}" stopped successfully`);
      // Service status will update via the shared polling hook
    } catch (error) {
      logger.error('Failed to stop service', {
        component: 'ServicePanel',
        serviceId: service.id,
        error: toError(error),
      });

      showNotification('error', `Failed to stop service "${service.name}": ${toError(error).message}`);
      // Service status will revert via the shared polling hook
    } finally {
      setServiceOperations(prev => ({ ...prev, [service.id]: null }));
    }
  };

  // Start all essential services (with progress tracking)
  const handleStartEssentialServices = async () => {
    logger.info('Starting all essential services', {
      component: 'ServicePanel',
    });

    setEssentialOperation('starting');

    try {
      const result = await apiClient.startEssentialServices();

      logger.info('Essential services started successfully', {
        component: 'ServicePanel',
        message: result.message,
      });

      showNotification('success', 'All essential services started successfully');
      // Service status will update via the shared polling hook
    } catch (error) {
      logger.error('Failed to start essential services', {
        component: 'ServicePanel',
        error: toError(error),
      });

      showNotification('error', `Failed to start essential services: ${toError(error).message}`);
    } finally {
      setEssentialOperation('idle');
    }
  };

  // Stop all essential services (with strict confirmation)
  const handleStopEssentialServices = async () => {
    // Show confirmation dialog that requires typing "STOP"
    setStopAllConfirmation(true);
    setStopAllConfirmText('');
  };

  const confirmStopAllServices = async () => {
    if (stopAllConfirmText !== 'STOP') return;

    setStopAllConfirmation(false);
    setStopAllConfirmText('');

    logger.info('Stopping all essential services', {
      component: 'ServicePanel',
    });

    setEssentialOperation('stopping');

    try {
      const result = await apiClient.stopEssentialServices();

      logger.info('Essential services stopped successfully', {
        component: 'ServicePanel',
        message: result.message,
      });

      showNotification('success', 'All essential services stopped successfully');
      // Service status will update via the shared polling hook
    } catch (error) {
      logger.error('Failed to stop essential services', {
        component: 'ServicePanel',
        error: toError(error),
      });

      showNotification('error', `Failed to stop essential services: ${toError(error).message}`);
    } finally {
      setEssentialOperation('idle');
    }
  };

  const coreServices = services.filter(s => s.category === 'core');
  const monitoringServices = services.filter(s => s.category === 'monitoring');

  const runningServices = services.filter(s => s.status === 'running').length;
  const totalServices = services.length;

  const getGlobalStatusIcon = () => {
    switch (globalStatus) {
      case 'healthy': return <CheckCircle className="w-5 h-5 text-gray-600" />;
      case 'warning': return <AlertTriangle className="w-5 h-5 text-gray-500" />;
      case 'error': return <AlertTriangle className="w-5 h-5 text-gray-700" />;
      default: return <RefreshCw className="w-5 h-5 text-gray-500 animate-spin" />;
    }
  };

  const getGlobalStatusText = () => {
    switch (globalStatus) {
      case 'healthy': return 'All Systems Operational';
      case 'warning': return 'Partial Service Degradation';
      case 'error': return 'Major Service Issues';
      default: return 'Checking System Status...';
    }
  };

  const getServiceIcon = (category: string) => {
    switch (category) {
      case 'core': return Server;
      case 'monitoring': return Activity;
      default: return Settings;
    }
  };

  return (
    <div className="min-h-screen bg-surface-1 p-6">
      <div className="max-w-7xl mx-auto space-y-6">
        {/* Header */}
        <div className="flex items-center justify-between">
          <div>
            <h1 className="text-3xl font-bold text-gray-900">AdapterOS Control Panel</h1>
            <p className="text-gray-600 mt-1">Manage services, orchestration, and system configuration</p>
          </div>
          <div className="flex items-center gap-4">
            <div className="flex items-center gap-2">
              {getGlobalStatusIcon()}
              <span className="text-sm font-medium">{getGlobalStatusText()}</span>
            </div>
            <Badge variant="secondary">
              {runningServices}/{totalServices} Services Running
            </Badge>
            <Button onClick={loadServices} variant="outline" size="sm" disabled={isLoading}>
              <RefreshCw className={`w-4 h-4 mr-2 ${isLoading ? 'animate-spin' : ''}`} />
              Refresh
            </Button>
          </div>
        </div>

        <Tabs defaultValue="services" className="space-y-6">
          <TabsList className="grid w-full grid-cols-4">
            <TabsTrigger value="services" className="flex items-center gap-2">
              <Settings className="w-4 h-4" />
              Services
            </TabsTrigger>
            <TabsTrigger value="orchestration" className="flex items-center gap-2">
              <Brain className="w-4 h-4" />
              Prompt Orchestration
            </TabsTrigger>
            <TabsTrigger value="monitoring" className="flex items-center gap-2">
              <Activity className="w-4 h-4" />
              Monitoring
            </TabsTrigger>
            <TabsTrigger value="authentication" className="flex items-center gap-2">
              <Key className="w-4 h-4" />
              Authentication
            </TabsTrigger>
          </TabsList>

          <TabsContent value="services" className="space-y-6">
            {/* Essential Services Quick Actions */}
            <Card>
              <CardHeader>
                <CardTitle className="flex items-center gap-2">
                  <Zap className="w-5 h-5 text-gray-500" />
                  Essential Services
                </CardTitle>
              </CardHeader>
              <CardContent className="space-y-4">
                <div className="flex items-center justify-between">
                  <div>
                    <p className="text-sm text-gray-600">
                      {essentialServices.length} essential services configured
                    </p>
                    <p className="text-xs text-gray-500">
                      {essentialServices.filter(s => s.status === 'running').length} currently running
                    </p>
                  </div>
                  <div className="flex gap-2">
                    <Button
                      onClick={handleStartEssentialServices}
                      disabled={true}
                      variant="default"
                      size="sm"
                      title="Essential services management under development"
                    >
                      Start All Essential
                    </Button>
                    <Button
                      onClick={handleStopEssentialServices}
                      disabled={true}
                      variant="outline"
                      size="sm"
                      title="Essential services management under development"
                    >
                      Stop All Essential
                    </Button>
                  </div>
                </div>

                {essentialServices.length > 0 && (
                  <div className="space-y-2">
                    <p className="text-sm font-medium">Essential Services:</p>
                    <div className="flex flex-wrap gap-2">
                      {essentialServices.map(service => (
                        <Badge
                          key={service.id}
                          variant={service.status === 'running' ? 'default' : 'secondary'}
                          className="flex items-center gap-1"
                        >
                          {service.status === 'running' ? (
                            <CheckCircle className="w-3 h-3" />
                          ) : (
                            <Square className="w-3 h-3" />
                          )}
                          {service.name}
                        </Badge>
                      ))}
                    </div>
                  </div>
                )}
              </CardContent>
            </Card>

            {/* Service Groups */}
            <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
              {/* Core Services */}
              <div className="space-y-4">
                <div className="flex items-center gap-2">
                  <Server className="w-5 h-5 text-gray-500" />
                  <h2 className="text-xl font-semibold text-gray-900">Core Services</h2>
                </div>
                <div className="space-y-3">
                  {coreServices.map(service => (
                    <ServiceCard
                      key={service.id}
                  service={{
                    ...service,
                    icon: getServiceIcon(service.category),
                    status: service.status,
                    description: `${service.name} service`
                  }}
                  onStart={() => handleStartService(service)}
                  onStop={() => handleStopService(service)}
                  onRestart={() => {}} // Not implemented yet
                  onSelect={() => setSelectedService(service)}
                  isSelected={selectedService?.id === service.id}
                />
              ))}
            </div>
          </div>

          {/* Monitoring Services */}
          <div className="space-y-4">
            <div className="flex items-center gap-2">
              <Activity className="w-5 h-5 text-green-500" />
              <h2 className="text-xl font-semibold text-gray-900">Monitoring</h2>
            </div>
            <div className="space-y-3">
              {monitoringServices.map(service => (
                <ServiceCard
                  key={service.id}
                  service={{
                    ...service,
                    icon: getServiceIcon(service.category),
                    status: service.status,
                    description: `${service.name} service`
                  }}
                  onStart={() => handleStartService(service)}
                  onStop={() => handleStopService(service)}
                  onRestart={() => {}} // Not implemented yet
                  onSelect={() => setSelectedService(service)}
                  isSelected={selectedService?.id === service.id}
                />
                  ))}
                </div>
              </div>
            </div>

            {/* Terminal Output */}
            {selectedService && (
              <Card className="mt-6">
                <CardHeader>
                  <div className="flex items-center gap-2">
                    <Terminal className="w-5 h-5" />
                    <CardTitle>Terminal Output - {selectedService.name}</CardTitle>
                  </div>
                </CardHeader>
                <CardContent>
                  <TerminalOutput logs={selectedService.logs} />
                </CardContent>
              </Card>
            )}
          </TabsContent>

          <TabsContent value="orchestration">
            <PromptOrchestrationPanel />
          </TabsContent>

          <TabsContent value="monitoring">
            <Card>
              <CardHeader>
                <CardTitle className="flex items-center gap-2">
                  <Activity className="w-5 h-5" />
                  System Monitoring
                </CardTitle>
              </CardHeader>
              <CardContent>
                <div className="text-center py-8 text-gray-500">
                  <Monitor className="w-12 h-12 mx-auto mb-4 opacity-50" />
                  <p>Advanced monitoring dashboard coming soon</p>
                  <p className="text-sm mt-2">Service status and logs available in the Services tab</p>
                </div>
              </CardContent>
            </Card>
          </TabsContent>

          <TabsContent value="authentication">
            <AuthenticationSettings />
          </TabsContent>
        </Tabs>

        {/* Notification Alert */}
        {notification && (
          <div className="fixed bottom-4 right-4 z-50 max-w-md">
            <Alert variant={notification.type === 'error' ? 'destructive' : 'default'} className="shadow-lg">
              <AlertDescription className="flex items-center justify-between">
                <span>{notification.message}</span>
                <button
                  onClick={() => setNotification(null)}
                  className="ml-4 hover:opacity-70"
                >
                  <X className="w-4 h-4" />
                </button>
              </AlertDescription>
            </Alert>
          </div>
        )}

        {/* Stop Service Confirmation Dialog */}
        <AlertDialog open={!!stopConfirmation} onOpenChange={(open) => !open && setStopConfirmation(null)}>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>Stop Service?</AlertDialogTitle>
              <AlertDialogDescription>
                Are you sure you want to stop <strong>{stopConfirmation?.name}</strong>?
                <br /><br />
                This will terminate the service and may affect dependent services.
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>Cancel</AlertDialogCancel>
              <AlertDialogAction onClick={confirmStopService} className="bg-red-600 hover:bg-red-700">
                Stop Service
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>

        {/* Stop All Essential Services Confirmation Dialog */}
        <AlertDialog open={stopAllConfirmation} onOpenChange={(open) => !open && setStopAllConfirmation(false)}>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle className="text-red-600">⚠️ Stop All Essential Services?</AlertDialogTitle>
              <AlertDialogDescription>
                <div className="space-y-3">
                  <p>
                    This action will stop <strong>all essential services</strong>, which may cause system-wide disruption.
                  </p>
                  <p>
                    Type <code className="bg-gray-100 px-2 py-1 rounded">STOP</code> to confirm:
                  </p>
                  <Input
                    value={stopAllConfirmText}
                    onChange={(e) => setStopAllConfirmText(e.target.value)}
                    placeholder="Type STOP to confirm"
                    className="font-mono"
                  />
                </div>
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel onClick={() => setStopAllConfirmText('')}>Cancel</AlertDialogCancel>
              <AlertDialogAction
                onClick={confirmStopAllServices}
                disabled={stopAllConfirmText !== 'STOP'}
                className="bg-red-600 hover:bg-red-700 disabled:opacity-50 disabled:cursor-not-allowed"
              >
                Stop All Services
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </div>
    </div>
  );
}
