import React, { useState, useEffect, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Separator } from './ui/separator';
import { ScrollArea } from './ui/scroll-area';
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
  Terminal
} from 'lucide-react';
import { ServiceCard } from './ServiceCard';
import { TerminalOutput } from './TerminalOutput';

// Simple service interface
interface SimpleService {
  id: string;
  name: string;
  status: 'running' | 'stopped' | 'starting' | 'stopping' | 'error';
  port?: number;
  pid?: number;
  startTime?: string;
  category: string;
  logs: string[];
}

export default function ServicePanel() {
  const [services, setServices] = useState<SimpleService[]>([]);
  const [selectedService, setSelectedService] = useState<SimpleService | null>(null);
  const [globalStatus, setGlobalStatus] = useState<'checking' | 'healthy' | 'warning' | 'error'>('checking');
  const [isLoading, setIsLoading] = useState(false);

  // Load services from backend
  const loadServices = useCallback(async () => {
    try {
      const response = await fetch('/api/services');
      if (response.ok) {
        const data = await response.json();
        setServices(data.services);

        // Calculate global status
        const running = data.services.filter((s: SimpleService) => s.status === 'running').length;
        const total = data.services.length;

        if (running === total) {
          setGlobalStatus('healthy');
        } else if (running >= total * 0.5) {
          setGlobalStatus('warning');
        } else {
          setGlobalStatus('error');
        }
      }
    } catch (error) {
      console.error('Failed to load services:', error);
      setGlobalStatus('error');
    }
  }, []);

  // Initial load and polling
  useEffect(() => {
    loadServices();
    const interval = setInterval(loadServices, 3000); // Poll every 3 seconds
    return () => clearInterval(interval);
  }, [loadServices]);

  // Service control handlers
  const handleStartService = async (service: SimpleService) => {
    setIsLoading(true);
    try {
      const response = await fetch('/api/services/start', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ serviceId: service.id })
      });

      if (response.ok) {
        await loadServices(); // Refresh services
      } else {
        const error = await response.json();
        console.error('Failed to start service:', error);
      }
    } catch (error) {
      console.error('Error starting service:', error);
    }
    setIsLoading(false);
  };

  const handleStopService = async (service: SimpleService) => {
    setIsLoading(true);
    try {
      const response = await fetch('/api/services/stop', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ serviceId: service.id })
      });

      if (response.ok) {
        await loadServices(); // Refresh services
      } else {
        const error = await response.json();
        console.error('Failed to stop service:', error);
      }
    } catch (error) {
      console.error('Error stopping service:', error);
    }
    setIsLoading(false);
  };

  const coreServices = services.filter(s => s.category === 'core');
  const monitoringServices = services.filter(s => s.category === 'monitoring');

  const runningServices = services.filter(s => s.status === 'running').length;
  const totalServices = services.length;

  const getGlobalStatusIcon = () => {
    switch (globalStatus) {
      case 'healthy': return <CheckCircle className="w-5 h-5 text-green-500" />;
      case 'warning': return <AlertTriangle className="w-5 h-5 text-yellow-500" />;
      case 'error': return <AlertTriangle className="w-5 h-5 text-red-500" />;
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
            <h1 className="text-3xl font-bold text-gray-900">Service Management</h1>
            <p className="text-gray-600 mt-1">Monitor and control AdapterOS services</p>
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

        {/* Service Groups */}
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          {/* Core Services */}
          <div className="space-y-4">
            <div className="flex items-center gap-2">
              <Server className="w-5 h-5 text-blue-500" />
              <h2 className="text-xl font-semibold text-gray-900">Core Services</h2>
            </div>
            <div className="space-y-3">
              {coreServices.map(service => (
                <ServiceCard
                  key={service.id}
                  service={{
                    ...service,
                    icon: getServiceIcon(service.category),
                    status: service.status as any
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
                    status: service.status as any
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
      </div>
    </div>
  );
}
