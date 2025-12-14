import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import {
  Play,
  Square,
  Clock,
  AlertTriangle,
  CheckCircle,
  Loader2,
  RotateCcw,
  Server as ServerIcon,
  Zap
} from 'lucide-react';
import { cn } from './ui/utils';
interface Service {
  id: string;
  name: string;
  status: 'running' | 'stopped' | 'starting' | 'stopping' | 'error';
  port?: number;
  pid?: number;
  startTime?: string;
  endTime?: Date;
  category: string;
  essential?: boolean;
  dependencies?: string[];
  startupOrder?: number;
  logs: string[];
  icon?: React.ComponentType<{ className?: string }>;
  description?: string;
  error?: string;
}

interface ServiceCardProps {
  service: Service;
  onStart: () => void;
  onStop: () => void;
  onRestart: () => void;
  onSelect: () => void;
  isSelected: boolean;
}

export function ServiceCard({ service, onStart, onStop, onRestart, onSelect, isSelected }: ServiceCardProps) {
  const IconComponent = service.icon ?? ServerIcon;

  const getStatusColor = (status: Service['status']) => {
    switch (status) {
      case 'running': return 'text-green-600 bg-green-50 border-green-200';
      case 'stopped': return 'text-gray-600 bg-gray-50 border-gray-200';
      case 'starting': return 'text-blue-600 bg-blue-50 border-blue-200';
      case 'stopping': return 'text-orange-600 bg-orange-50 border-orange-200';
      case 'error': return 'text-red-600 bg-red-50 border-red-200';
      default: return 'text-gray-600 bg-gray-50 border-gray-200';
    }
  };

  const getStatusIcon = (status: Service['status']) => {
    switch (status) {
      case 'running': return <CheckCircle className="w-4 h-4" />;
      case 'stopped': return <Square className="w-4 h-4" />;
      case 'starting': return <Loader2 className="w-4 h-4 animate-spin" />;
      case 'stopping': return <Loader2 className="w-4 h-4 animate-spin" />;
      case 'error': return <AlertTriangle className="w-4 h-4" />;
      default: return <Square className="w-4 h-4" />;
    }
  };

  const getStatusText = (status: Service['status']) => {
    switch (status) {
      case 'running': return 'Running';
      case 'stopped': return 'Stopped';
      case 'starting': return 'Starting...';
      case 'stopping': return 'Stopping...';
      case 'error': return 'Error';
      default: return 'Unknown';
    }
  };

  const getSessionDuration = () => {
    if (!service.startTime) return null;

    const startTime = new Date(service.startTime);
    const endTime = service.endTime || new Date();
    const duration = endTime.getTime() - startTime.getTime();

    const hours = Math.floor(duration / (1000 * 60 * 60));
    const minutes = Math.floor((duration % (1000 * 60 * 60)) / (1000 * 60));
    const seconds = Math.floor((duration % (1000 * 60)) / 1000);

    if (hours > 0) {
      return `${hours}h ${minutes}m ${seconds}s`;
    } else if (minutes > 0) {
      return `${minutes}m ${seconds}s`;
    } else {
      return `${seconds}s`;
    }
  };

  const canStart = service.status === 'stopped' || service.status === 'error';
  const canStop = service.status === 'running';

  return (
    <Card
      className={cn(
        "cursor-pointer transition-all duration-200 hover:shadow-md",
        isSelected && "ring-2 ring-accent-500",
        service.status === 'error' && "border-red-200 bg-red-50/30"
      )}
      onClick={onSelect}
    >
      <CardHeader className="pb-3">
        <div className="flex items-start justify-between">
          <div className="flex items-center gap-3">
            <div className="p-2 rounded-lg bg-gray-50">
              <IconComponent className="w-5 h-5 text-gray-600" />
            </div>
            <div>
              <CardTitle className="text-base font-semibold">{service.name}</CardTitle>
              <p className="text-sm text-gray-600 mt-1">{service.description}</p>
            </div>
          </div>
          <div className="flex flex-col items-end gap-1">
            {service.essential && (
              <Badge variant="outline" className="text-xs px-2 py-0.5 bg-yellow-50 border-yellow-200 text-yellow-700 flex items-center gap-1">
                <Zap className="w-3 h-3" />
                Essential
              </Badge>
            )}
            <Badge className={cn("flex items-center gap-1", getStatusColor(service.status))}>
              {getStatusIcon(service.status)}
              {getStatusText(service.status)}
            </Badge>
          </div>
        </div>
      </CardHeader>

      <CardContent className="pt-0">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-4 text-sm text-gray-600">
            {service.port && (
              <div className="flex items-center gap-1">
                <ServerIcon className="w-4 h-4" />
                :{service.port}
              </div>
            )}
            {service.pid && (
              <div className="flex items-center gap-1">
                PID: {service.pid}
              </div>
            )}
            {service.startTime && (
              <div className="flex items-center gap-1">
                <Clock className="w-4 h-4" />
                {getSessionDuration()}
              </div>
            )}
          </div>

          <div className="flex items-center gap-2">
            {canStart && (
              <Button
                size="sm"
                variant="success"
                onClick={(e) => {
                  e.stopPropagation();
                  onStart();
                }}
                className="h-8"
              >
                <Play className="w-3 h-3" />
              </Button>
            )}
            {service.status === 'running' && (
              <Button
                size="sm"
                variant="outline"
                onClick={(e) => {
                  e.stopPropagation();
                  onRestart();
                }}
                className="h-8"
              >
                <RotateCcw className="w-3 h-3" />
              </Button>
            )}
            {canStop && (
              <Button
                size="sm"
                variant="destructive"
                onClick={(e) => {
                  e.stopPropagation();
                  onStop();
                }}
                className="h-8"
              >
                <Square className="w-3 h-3" />
              </Button>
            )}
            {service.status === 'starting' && (
              <Button size="sm" disabled className="h-8">
                <Loader2 className="w-3 h-3 animate-spin" />
              </Button>
            )}
            {service.status === 'stopping' && (
              <Button size="sm" disabled variant="destructive" className="h-8">
                <Loader2 className="w-3 h-3 animate-spin" />
              </Button>
            )}
          </div>
        </div>

        {service.error && (
          <div className="mt-3 p-2 bg-red-50 border border-red-200 rounded-md">
            <div className="flex items-center gap-2 text-red-700 text-sm">
              <AlertTriangle className="w-4 h-4" />
              <span className="font-medium">Error:</span>
              <span>{service.error}</span>
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
