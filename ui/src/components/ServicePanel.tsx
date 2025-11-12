import React, { useState, useEffect, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Separator } from './ui/separator';
import { ScrollArea } from './ui/scroll-area';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';
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
  Key
} from 'lucide-react';
import { ServiceCard } from './ServiceCard';
import { TerminalOutput } from './TerminalOutput';
import PromptOrchestrationPanel from './PromptOrchestrationPanel';
import { AuthenticationSettings } from './AuthenticationSettings';
import { logger, toError } from '../utils/logger';
import apiClient from '../api/client';

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
  const [services, setServices] = useState<SimpleService[]>([]);
  const [selectedService, setSelectedService] = useState<SimpleService | null>(null);
  const [globalStatus, setGlobalStatus] = useState<'checking' | 'healthy' | 'warning' | 'error'>('checking');
  const [isLoading, setIsLoading] = useState(false);
  const [essentialServices, setEssentialServices] = useState<any[]>([]);
  const [essentialOperation, setEssentialOperation] = useState<'idle' | 'starting' | 'stopping'>('idle');

  // Load services from backend
  const loadServices = useCallback(async () => {
    try {
      const data = await apiClient.getStatus();
      // Map status.services to SimpleService format
      const services: SimpleService[] = (data.services || []).map((s: any) => ({
        id: s.id,
        name: s.name,
        status: s.state as 'running' | 'stopped' | 'starting' | 'stopping' | 'error',
        port: s.port,
        pid: s.pid,
        startTime: s.start_time,
        category: s.category || 'core',
        essential: s.essential,
        dependencies: s.dependencies,
        startupOrder: s.startup_order,
        logs: s.logs || [],
      }));
      setServices(services);

      // Calculate global status
      const running = services.filter((s) => s.status === 'running').length;
      const total = services.length;

      if (running === total) {
        setGlobalStatus('healthy');
      } else if (running >= total * 0.5) {
        setGlobalStatus('warning');
      } else {
        setGlobalStatus('error');
      }
    } catch (error) {
      logger.error('Failed to load services', { 
        component: 'ServicePanel',
        error: toError(error),
      });
      setGlobalStatus('error');
    }
  }, []);

  // Load essential services
  const loadEssentialServices = useCallback(async () => {
    // For now, filter essential from services
    // This is a placeholder; implement proper essential services endpoint when available
    try {
      const data = await apiClient.getStatus();
      const essential = (data.services || [])
        .filter((s: any) => s.essential)
        .map((s: any) => ({
          id: s.id,
          name: s.name,
          status: s.state,
          // ... other fields
        }));
      setEssentialServices(essential);
    } catch (error) {
      logger.error('Failed to load essential services', { 
        component: 'ServicePanel',
        error: toError(error),
      });
    }
  }, []);

  // Initial load and polling
  useEffect(() => {
    loadServices();
    loadEssentialServices();
    const interval = setInterval(() => {
      loadServices();
      loadEssentialServices();
    }, 3000); // Poll every 3 seconds
    return () => clearInterval(interval);
  }, [loadServices, loadEssentialServices]);

  // Disable service control functions with placeholders
  const handleStartService = async (service: SimpleService) => {
    // Placeholder: Service control under development
    logger.warn('Service start requested but not implemented', {
      component: 'ServicePanel',
      serviceId: service.id,
    });
    // TODO: Implement when backend endpoint available
  };

  const handleStopService = async (service: SimpleService) => {
    // Placeholder: Service control under development
    logger.warn('Service stop requested but not implemented', {
      component: 'ServicePanel',
      serviceId: service.id,
    });
    // TODO: Implement when backend endpoint available
  };

  // Start all essential services
  const handleStartEssentialServices = async () => {
    // Placeholder
    logger.warn('Essential services start requested but not implemented', {
      component: 'ServicePanel',
    });
  };

  // Stop all essential services
  const handleStopEssentialServices = async () => {
    // Placeholder
    logger.warn('Essential services stop requested but not implemented', {
      component: 'ServicePanel',
    });
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
      </div>
    </div>
  );
}
