import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Progress } from './ui/progress';
import { Alert, AlertDescription } from './ui/alert';
import {
  MemoryStick,
  TrendingUp,
  TrendingDown,
  AlertTriangle,
  CheckCircle,
  XCircle,
  Activity,
  Target,
  Zap,
  Clock,
  BarChart3,
  PieChart,
  Settings,
  Trash2,
  Pin,
  PinOff
} from 'lucide-react';
import {
  Adapter,
  AdapterCategory,
  AdapterState,
  EvictionPriority,
  MemoryUsageByCategory
} from '../api/types';
import apiClient from '../api/client';
import { logger } from '../utils/logger';
import { toast } from 'sonner';

interface AdapterMemoryMonitorProps {
  adapters: Adapter[];
  totalMemory: number;
  onEvictAdapter: (adapterId: string) => void;
  onPinAdapter: (adapterId: string, pinned: boolean) => void;
  onUpdateMemoryLimit: (category: AdapterCategory, limit: number) => void;
}

export function AdapterMemoryMonitor({ 
  adapters, 
  totalMemory, 
  onEvictAdapter, 
  onPinAdapter,
  onUpdateMemoryLimit 
}: AdapterMemoryMonitorProps) {
  const [memoryPressureThreshold, setMemoryPressureThreshold] = useState(80);
  const [selectedCategory, setSelectedCategory] = useState<AdapterCategory | 'all'>('all');

  // Calculate memory statistics
  const totalMemoryUsed = adapters.reduce((sum, adapter) => sum + adapter.memory_bytes, 0);
  const memoryUsagePercent = totalMemory > 0 ? (totalMemoryUsed / totalMemory) * 100 : 0;
  
  const memoryByCategory = adapters.reduce((acc, adapter) => {
    acc[adapter.category] = (acc[adapter.category] || 0) + adapter.memory_bytes;
    return acc;
  }, {} as MemoryUsageByCategory);

  const memoryByState = adapters.reduce((acc, adapter) => {
    acc[adapter.current_state] = (acc[adapter.current_state] || 0) + adapter.memory_bytes;
    return acc;
  }, {} as Record<AdapterState, number>);

  const evictionCandidates = adapters
    .filter(adapter => !adapter.pinned && adapter.current_state !== 'resident')
    .sort((a, b) => {
      // Sort by eviction priority and memory usage
      const priorityOrder = { 'critical': 0, 'high': 1, 'normal': 2, 'low': 3, 'never': 4 };
      const aPriority = priorityOrder[a.category === 'ephemeral' ? 'critical' : 'normal'];
      const bPriority = priorityOrder[b.category === 'ephemeral' ? 'critical' : 'normal'];
      
      if (aPriority !== bPriority) return aPriority - bPriority;
      return b.memory_bytes - a.memory_bytes; // Higher memory first
    });

  const getMemoryPressureLevel = () => {
    if (memoryUsagePercent >= memoryPressureThreshold) return 'critical';
    if (memoryUsagePercent >= memoryPressureThreshold * 0.8) return 'high';
    if (memoryUsagePercent >= memoryPressureThreshold * 0.6) return 'medium';
    return 'low';
  };

  const getMemoryPressureColor = (level: string) => {
    switch (level) {
      case 'critical': return 'bg-red-100 text-red-800 border-red-200';
      case 'high': return 'bg-orange-100 text-orange-800 border-orange-200';
      case 'medium': return 'bg-yellow-100 text-yellow-800 border-yellow-200';
      case 'low': return 'bg-green-100 text-green-800 border-green-200';
      default: return 'bg-gray-100 text-gray-800 border-gray-200';
    }
  };

  const getCategoryIcon = (category: AdapterCategory) => {
    switch (category) {
      case 'code': return <Target className="h-4 w-4" />;
      case 'framework': return <Zap className="h-4 w-4" />;
      case 'codebase': return <Activity className="h-4 w-4" />;
      case 'ephemeral': return <Clock className="h-4 w-4" />;
      default: return <Activity className="h-4 w-4" />;
    }
  };

  const getCategoryColor = (category: AdapterCategory) => {
    switch (category) {
      case 'code': return 'bg-green-100 text-green-800';
      case 'framework': return 'bg-blue-100 text-blue-800';
      case 'codebase': return 'bg-purple-100 text-purple-800';
      case 'ephemeral': return 'bg-yellow-100 text-yellow-800';
      default: return 'bg-gray-100 text-gray-800';
    }
  };

  const getStateIcon = (state: AdapterState) => {
    switch (state) {
      case 'unloaded': return <XCircle className="h-4 w-4 text-gray-500" />;
      case 'cold': return <CheckCircle className="h-4 w-4 text-blue-500" />;
      case 'warm': return <Activity className="h-4 w-4 text-orange-500" />;
      case 'hot': return <TrendingUp className="h-4 w-4 text-red-500" />;
      case 'resident': return <Pin className="h-4 w-4 text-purple-500" />;
      default: return <Activity className="h-4 w-4 text-gray-500" />;
    }
  };


  const handleEvictAdapter = async (adapterId: string) => {
    try {
      logger.info('Evicting adapter', {
        component: 'AdapterMemoryMonitor',
        operation: 'evictAdapter',
        adapterId
      });

      const result = await apiClient.evictAdapter(adapterId);
      onEvictAdapter(adapterId);

      toast.success(`Adapter evicted: ${result.message || 'Memory freed successfully'}`);
      logger.info('Adapter evicted successfully', {
        component: 'AdapterMemoryMonitor',
        operation: 'evictAdapter',
        adapterId,
        result
      });
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to evict adapter';
      logger.error('Failed to evict adapter', {
        component: 'AdapterMemoryMonitor',
        operation: 'evictAdapter',
        adapterId,
        error: errorMessage
      }, error instanceof Error ? error : new Error(String(error)));
      toast.error(`Failed to evict adapter: ${errorMessage}`);
    }
  };

  const handlePinToggle = async (adapterId: string, pinned: boolean) => {
    try {
      logger.info('Toggling adapter pin status', {
        component: 'AdapterMemoryMonitor',
        operation: 'pinToggle',
        adapterId,
        pinned
      });

      await apiClient.pinAdapter(adapterId, pinned);
      onPinAdapter(adapterId, pinned);

      toast.success(pinned ? 'Adapter pinned successfully' : 'Adapter unpinned successfully');
      logger.info('Adapter pin status updated successfully', {
        component: 'AdapterMemoryMonitor',
        operation: 'pinToggle',
        adapterId,
        pinned
      });
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to pin/unpin adapter';
      logger.error('Failed to pin/unpin adapter', {
        component: 'AdapterMemoryMonitor',
        operation: 'pinToggle',
        adapterId,
        pinned,
        error: errorMessage
      }, error instanceof Error ? error : new Error(String(error)));
      toast.error(`Failed to ${pinned ? 'pin' : 'unpin'} adapter: ${errorMessage}`);
    }
  };

  const memoryPressureLevel = getMemoryPressureLevel();
  const categories: AdapterCategory[] = ['code', 'framework', 'codebase', 'ephemeral'];

  return (
    <div className="space-y-6">
      {/* Memory Overview */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center justify-between">
            <div className="flex items-center">
              <MemoryStick className="mr-2 h-5 w-5" />
              Memory Usage Overview
            </div>
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="space-y-4">
            <div className="flex items-center justify-between">
              <span className="text-sm font-medium">Total Memory Usage</span>
              <span className="text-sm text-muted-foreground">
                {Math.round(totalMemoryUsed / 1024 / 1024)} MB / {Math.round(totalMemory / 1024 / 1024)} MB
              </span>
            </div>
            <Progress value={memoryUsagePercent} className="h-3" />
            <div className="flex items-center justify-between">
              <span className="text-xs text-muted-foreground">
                {memoryUsagePercent.toFixed(1)}% of total memory allocated
              </span>
              <Badge className={getMemoryPressureColor(memoryPressureLevel)}>
                {memoryPressureLevel.toUpperCase()} PRESSURE
              </Badge>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Memory Pressure Alert */}
      {memoryPressureLevel === 'critical' && (
        <Alert variant="destructive">
          <AlertTriangle className="icon-standard" />
          <AlertDescription>
            <strong>Critical Memory Pressure:</strong> Memory usage is above {memoryPressureThreshold}%. 
            Consider evicting unused adapters immediately.
          </AlertDescription>
        </Alert>
      )}

      {memoryPressureLevel === 'high' && (
        <Alert className="status-indicator status-warning">
          <AlertTriangle className="icon-standard" />
          <AlertDescription>
            <strong>High Memory Pressure:</strong> Memory usage is above {memoryPressureThreshold * 0.8}%. 
            Monitor closely and consider proactive eviction.
          </AlertDescription>
        </Alert>
      )}

      {/* Memory by Category */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center">
            <PieChart className="mr-2 h-5 w-5" />
            Memory Usage by Category
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="space-y-4">
            {categories.map((category) => {
              const memory = memoryByCategory[category] || 0;
              const percentage = totalMemoryUsed > 0 ? (memory / totalMemoryUsed) * 100 : 0;
              const adapterCount = adapters.filter(a => a.category === category).length;
              
              return (
                <div key={category} className="flex items-center justify-between p-3 rounded-lg border">
                  <div className="flex items-center space-x-3">
                    {getCategoryIcon(category)}
                    <div>
                      <div className="font-medium capitalize">{category}</div>
                      <div className="text-sm text-muted-foreground">
                        {adapterCount} adapters
                      </div>
                    </div>
                  </div>
                  <div className="flex items-center space-x-3">
                    <div className="text-right">
                      <div className="font-medium">{Math.round(memory / 1024 / 1024)} MB</div>
                      <div className="text-sm text-muted-foreground">
                        {percentage.toFixed(1)}% of used
                      </div>
                    </div>
                    <div className="w-20">
                      <Progress value={percentage} className="h-2" />
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        </CardContent>
      </Card>

      {/* Memory by State */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center">
            <BarChart3 className="mr-2 h-5 w-5" />
            Memory Usage by State
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="space-y-4">
            {Object.entries(memoryByState).map(([state, memory]) => {
              const percentage = totalMemoryUsed > 0 ? (memory / totalMemoryUsed) * 100 : 0;
              const adapterCount = adapters.filter(a => a.current_state === state).length;
              
              return (
                <div key={state} className="flex items-center justify-between p-3 rounded-lg border">
                  <div className="flex items-center space-x-3">
                    {getStateIcon(state as AdapterState)}
                    <div>
                      <div className="font-medium capitalize">{state}</div>
                      <div className="text-sm text-muted-foreground">
                        {adapterCount} adapters
                      </div>
                    </div>
                  </div>
                  <div className="flex items-center space-x-3">
                    <div className="text-right">
                      <div className="font-medium">{Math.round(memory / 1024 / 1024)} MB</div>
                      <div className="text-sm text-muted-foreground">
                        {percentage.toFixed(1)}% of used
                      </div>
                    </div>
                    <div className="w-20">
                      <Progress value={percentage} className="h-2" />
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        </CardContent>
      </Card>

      {/* Eviction Candidates */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center">
            <Trash2 className="mr-2 h-5 w-5" />
            Eviction Candidates
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="space-y-3">
            {evictionCandidates.slice(0, 10).map((adapter) => (
              <div key={adapter.adapter_id} className="flex items-center justify-between p-3 rounded-lg border">
                <div className="flex items-center space-x-3">
                  {getCategoryIcon(adapter.category)}
                  <div>
                    <div className="font-medium">{adapter.name}</div>
                    <div className="text-sm text-muted-foreground">
                      {adapter.category} • {adapter.current_state} • {Math.round(adapter.memory_bytes / 1024 / 1024)} MB
                    </div>
                  </div>
                </div>
                <div className="flex items-center space-x-2">
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => handlePinToggle(adapter.adapter_id, !adapter.pinned)}
                  >
                    {adapter.pinned ? <PinOff className="h-4 w-4" /> : <Pin className="h-4 w-4" />}
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => handleEvictAdapter(adapter.adapter_id)}
                  >
                    <Trash2 className="h-4 w-4" />
                  </Button>
                </div>
              </div>
            ))}
            {evictionCandidates.length === 0 && (
              <div className="text-center py-8 text-muted-foreground">
                No eviction candidates found
              </div>
            )}
          </div>
        </CardContent>
      </Card>

      {/* Memory Statistics */}
      <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Total Adapters</CardTitle>
            <Activity className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{adapters.length}</div>
            <p className="text-xs text-muted-foreground">
              All categories
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Pinned Adapters</CardTitle>
            <Pin className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {adapters.filter(a => a.pinned).length}
            </div>
            <p className="text-xs text-muted-foreground">
              Protected from eviction
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Hot Adapters</CardTitle>
            <TrendingUp className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {adapters.filter(a => a.current_state === 'hot').length}
            </div>
            <p className="text-xs text-muted-foreground">
              Frequently used
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Cold Adapters</CardTitle>
            <TrendingDown className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {adapters.filter(a => a.current_state === 'cold').length}
            </div>
            <p className="text-xs text-muted-foreground">
              Loaded but inactive
            </p>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
