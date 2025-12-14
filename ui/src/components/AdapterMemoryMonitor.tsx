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
  PinOff,
  CheckSquare,
  Square
} from 'lucide-react';
import {
  Adapter,
  AdapterCategory,
  AdapterState,
  EvictionPriority,
  MemoryUsageByCategory
} from '@/api/types';
import apiClient from '@/api/client';
import { logger } from '@/utils/logger';
import { formatMB, formatString } from '@/utils';

import { ErrorRecovery, errorRecoveryTemplates } from './ui/error-recovery';

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
  const [statusMessage, setStatusMessage] = useState<{ message: string; variant: 'success' | 'info' | 'warning' } | null>(null);
  const [errorRecovery, setErrorRecovery] = useState<React.ReactElement | null>(null);
  const [selectedAdapterIds, setSelectedAdapterIds] = useState<Set<string>>(new Set());
  const [isLoading, setIsLoading] = useState(false);
  const [memoryData, setMemoryData] = useState<{
    total_memory_mb: number;
    available_memory_mb: number;
    memory_pressure_level: 'low' | 'medium' | 'high' | 'critical';
    adapters: Array<{
      id: string;
      name: string;
      memory_usage_mb: number;
      state: string;
      pinned: boolean;
      category: string;
    }>;
  } | null>(null);

  const showStatus = (message: string, variant: 'success' | 'info' | 'warning') => {
    setStatusMessage({ message, variant });
  };

  // Fetch memory usage from API
  const refreshMemoryData = async () => {
    try {
      const data = await apiClient.getMemoryUsage();
      setMemoryData(data);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to fetch memory usage';
      logger.error('Failed to fetch memory usage', {
        component: 'AdapterMemoryMonitor',
        error: errorMessage,
      }, error instanceof Error ? error : new Error(errorMessage));
      // Fall back to props-based calculation
    }
  };

  // Poll memory usage every 5 seconds
  useEffect(() => {
    refreshMemoryData();
    const interval = setInterval(refreshMemoryData, 5000);
    return () => clearInterval(interval);
  }, []);

  // Calculate memory statistics - use API data if available, otherwise fall back to props
  const totalMemoryUsed = memoryData
    ? memoryData.total_memory_mb - memoryData.available_memory_mb
    : adapters.reduce((sum, adapter) => sum + (adapter.memory_bytes ?? 0), 0) / 1024 / 1024;
  const effectiveTotalMemory = memoryData ? memoryData.total_memory_mb : totalMemory / 1024 / 1024;
  const memoryUsagePercent = effectiveTotalMemory > 0 ? (totalMemoryUsed / effectiveTotalMemory) * 100 : 0;

  const memoryByCategory = adapters.reduce((acc, adapter) => {
    const cat = adapter.category ?? 'code';
    acc[cat] = (acc[cat] || 0) + (adapter.memory_bytes ?? 0);
    return acc;
  }, {} as MemoryUsageByCategory);

  const memoryByState = adapters.reduce((acc, adapter) => {
    const state = adapter.current_state ?? 'unloaded';
    acc[state] = (acc[state] || 0) + (adapter.memory_bytes ?? 0);
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
      return (b.memory_bytes ?? 0) - (a.memory_bytes ?? 0); // Higher memory first
    });

  const getMemoryPressureLevel = () => {
    // Use API pressure level if available
    if (memoryData) {
      return memoryData.memory_pressure_level;
    }
    // Otherwise calculate from usage percent
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

      await refreshMemoryData(); // Refresh after eviction

      showStatus(`Adapter evicted: ${result.message || 'Memory freed successfully.'}`, 'success');


      toast.success(`Adapter evicted: ${result.message || 'Memory freed successfully'}`);
      logger.info('Adapter evicted successfully', {
        component: 'AdapterMemoryMonitor',
        operation: 'evictAdapter',
        adapterId,
        result
      });

      setErrorRecovery(null);

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to evict adapter';
      logger.error('Failed to evict adapter', {
        component: 'AdapterMemoryMonitor',
        operation: 'evictAdapter',
        adapterId,
        error: errorMessage
      }, error instanceof Error ? error : new Error(String(error)));

      setStatusMessage({ message: `Failed to evict adapter: ${errorMessage}`, variant: 'warning' });
      setErrorRecovery(
        <ErrorRecovery
          error={errorMessage}
          onRetry={() => handleEvictAdapter(adapterId)}
        />
      );

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

      await refreshMemoryData(); // Refresh after pinning

      showStatus(pinned ? 'Adapter pinned successfully.' : 'Adapter unpinned successfully.', 'success');


      toast.success(pinned ? 'Adapter pinned successfully' : 'Adapter unpinned successfully');
      logger.info('Adapter pin status updated successfully', {
        component: 'AdapterMemoryMonitor',
        operation: 'pinToggle',
        adapterId,
        pinned
      });

      setErrorRecovery(null);

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to pin/unpin adapter';
      logger.error('Failed to pin/unpin adapter', {
        component: 'AdapterMemoryMonitor',
        operation: 'pinToggle',
        adapterId,
        pinned,
        error: errorMessage
      }, error instanceof Error ? error : new Error(String(error)));

      setStatusMessage({ message: `Failed to ${pinned ? 'pin' : 'unpin'} adapter: ${errorMessage}`, variant: 'warning' });
      setErrorRecovery(
        <ErrorRecovery
          error={errorMessage}
          onRetry={() => handlePinToggle(adapterId, pinned)}
        />
      );
    }
  };

  const handleBulkPin = async (pinned: boolean) => {
    if (selectedAdapterIds.size === 0) return;
    
    const adapterIds = Array.from(selectedAdapterIds);
    setIsLoading(true);
    
    try {
      const results = await Promise.allSettled(
        adapterIds.map(id => apiClient.pinAdapter(id, pinned))
      );
      
      const succeeded: string[] = [];
      const failed: Array<{ id: string; error: string }> = [];
      
      results.forEach((result, index) => {
        const adapterId = adapterIds[index];
        if (result.status === 'fulfilled') {
          succeeded.push(adapterId);
          onPinAdapter(adapterId, pinned);
        } else {
          failed.push({
            id: adapterId,
            error: result.reason instanceof Error ? result.reason.message : String(result.reason),
          });
        }
      });
      
      await refreshMemoryData();
      setSelectedAdapterIds(new Set());
      
      if (failed.length === 0) {
        showStatus(
          pinned 
            ? `${succeeded.length} adapters pinned successfully.` 
            : `${succeeded.length} adapters unpinned successfully.`,
          'success'
        );
      } else {
        showStatus(
          `${succeeded.length} succeeded, ${failed.length} failed. First error: ${failed[0].error}`,
          'warning'
        );
      }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to bulk pin/unpin';
      showStatus(`Failed to ${pinned ? 'pin' : 'unpin'} adapters: ${errorMessage}`, 'warning');
      setErrorRecovery(
        <ErrorRecovery
          error={errorMessage}
          onRetry={() => handleBulkPin(pinned)}
        />
      );
    } finally {
      setIsLoading(false);
    }
  };

  const handleBulkEvict = async () => {
    if (selectedAdapterIds.size === 0) return;
    
    const adapterIds = Array.from(selectedAdapterIds);
    setIsLoading(true);
    
    try {
      // Filter out pinned adapters
      const evictableIds = adapterIds.filter(id => {
        const adapter = adapters.find(a => a.adapter_id === id);
        return adapter && !adapter.pinned;
      });
      
      if (evictableIds.length === 0) {
        showStatus('No unpinned adapters selected for eviction.', 'warning');
        setIsLoading(false);
        return;
      }
      
      const results = await Promise.allSettled(
        evictableIds.map(id => apiClient.evictAdapter(id))
      );
      
      const succeeded: string[] = [];
      const failed: Array<{ id: string; error: string }> = [];
      
      results.forEach((result, index) => {
        const adapterId = evictableIds[index];
        if (result.status === 'fulfilled') {
          succeeded.push(adapterId);
          onEvictAdapter(adapterId);
        } else {
          failed.push({
            id: adapterId,
            error: result.reason instanceof Error ? result.reason.message : String(result.reason),
          });
        }
      });
      
      await refreshMemoryData();
      setSelectedAdapterIds(new Set());
      
      if (failed.length === 0) {
        showStatus(`${succeeded.length} adapters evicted successfully.`, 'success');
      } else {
        showStatus(
          `${succeeded.length} succeeded, ${failed.length} failed. First error: ${failed[0].error}`,
          'warning'
        );
      }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to evict adapters';
      showStatus(`Failed to evict adapters: ${errorMessage}`, 'warning');
      setErrorRecovery(
        <ErrorRecovery
          error={errorMessage}
          onRetry={() => handleBulkEvict()}
        />
      );
    } finally {
      setIsLoading(false);
    }
  };

  const toggleAdapterSelection = (adapterId: string) => {
    setSelectedAdapterIds(prev => {
      const next = new Set(prev);
      if (next.has(adapterId)) {
        next.delete(adapterId);
      } else {
        next.add(adapterId);
      }
      return next;
    });
  };

  const toggleSelectAll = () => {
    if (selectedAdapterIds.size === evictionCandidates.length) {
      setSelectedAdapterIds(new Set());
    } else {
      setSelectedAdapterIds(new Set(evictionCandidates.map(a => a.adapter_id)));
    }
  };

  const memoryPressureLevel = getMemoryPressureLevel();
  const categories: AdapterCategory[] = ['code', 'framework', 'codebase', 'ephemeral'];

  return (
    <div className="space-y-6">
      {errorRecovery && (
        <div>
          {errorRecovery}
        </div>
      )}

      {statusMessage && (
        <Alert
          className={
            statusMessage.variant === 'success'
              ? 'border-green-200 bg-green-50'
              : statusMessage.variant === 'warning'
                ? 'border-amber-200 bg-amber-50'
                : 'border-blue-200 bg-blue-50'
          }
        >
          <AlertDescription
            className={
              statusMessage.variant === 'success'
                ? 'text-green-700'
                : statusMessage.variant === 'warning'
                  ? 'text-amber-700'
                  : 'text-blue-700'
            }
          >
            {statusMessage.message}
          </AlertDescription>
        </Alert>
      )}

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
                {Math.round(totalMemoryUsed)} MB / {Math.round(effectiveTotalMemory)} MB
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
              const percentage = totalMemoryUsed > 0 ? ((memory / 1024 / 1024) / totalMemoryUsed) * 100 : 0;
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
          <CardTitle className="flex items-center justify-between">
            <div className="flex items-center">
              <Trash2 className="mr-2 h-5 w-5" />
              Eviction Candidates
            </div>
            {selectedAdapterIds.size > 0 && (
              <div className="flex items-center space-x-2">
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => handleBulkPin(true)}
                  disabled={isLoading}
                >
                  <Pin className="mr-2 h-4 w-4" />
                  Pin Selected ({selectedAdapterIds.size})
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => handleBulkEvict()}
                  disabled={isLoading}
                >
                  <Trash2 className="mr-2 h-4 w-4" />
                  Evict Selected ({selectedAdapterIds.size})
                </Button>
              </div>
            )}
          </CardTitle>
        </CardHeader>
        <CardContent>
          {evictionCandidates.length > 0 && (
            <div className="mb-3 flex items-center space-x-2">
              <Button
                variant="ghost"
                size="sm"
                onClick={toggleSelectAll}
              >
                {selectedAdapterIds.size === evictionCandidates.length ? (
                  <CheckSquare className="mr-2 h-4 w-4" />
                ) : (
                  <Square className="mr-2 h-4 w-4" />
                )}
                {selectedAdapterIds.size === evictionCandidates.length ? 'Deselect All' : 'Select All'}
              </Button>
            </div>
          )}
          <div className="space-y-3">
            {evictionCandidates.slice(0, 10).map((adapter) => (
              <div key={adapter.adapter_id} className="flex items-center justify-between p-3 rounded-lg border">
                <div className="flex items-center space-x-3">
                  <button
                    onClick={() => toggleAdapterSelection(adapter.adapter_id)}
                    className="cursor-pointer"
                  >
                    {selectedAdapterIds.has(adapter.adapter_id) ? (
                      <CheckSquare className="h-4 w-4 text-primary" />
                    ) : (
                      <Square className="h-4 w-4 text-muted-foreground" />
                    )}
                  </button>
                  {getCategoryIcon(adapter.category ?? 'code')}
                  <div>
                    <div className="font-medium">{adapter.name}</div>
                    <div className="text-sm text-muted-foreground">
                      {formatString(adapter.category) || 'code'} • {formatString(adapter.current_state) || 'unloaded'} • {formatMB(adapter.memory_bytes, 0)}
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
            <CardTitle className="text-sm font-medium">Protected Adapters</CardTitle>
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
