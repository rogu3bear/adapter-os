import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Alert, AlertDescription } from './ui/alert';
import { Badge } from './ui/badge';
import { Progress } from './ui/progress';
import { 
  Thermometer, 
  Snowflake, 
  Flame, 
  Zap, 
  Anchor,
  Clock,
  MemoryStick,
  Activity,
  Target,
  TrendingUp,
  TrendingDown,
  Minus
} from 'lucide-react';
import { AdapterState, AdapterCategory, AdapterStateRecord } from '@/api/types';

interface AdapterStateVisualizationProps {
  adapters: AdapterStateRecord[];
  totalMemory: number;
}

export function AdapterStateVisualization({ adapters, totalMemory }: AdapterStateVisualizationProps) {
  const getStateIcon = (state: AdapterState) => {
    switch (state) {
      case 'unloaded': return <Minus className="h-4 w-4 text-gray-500" />;
      case 'cold': return <Snowflake className="h-4 w-4 text-blue-500" />;
      case 'warm': return <Thermometer className="h-4 w-4 text-orange-500" />;
      case 'hot': return <Flame className="h-4 w-4 text-red-500" />;
      case 'resident': return <Anchor className="h-4 w-4 text-purple-500" />;
      default: return <Activity className="h-4 w-4 text-gray-500" />;
    }
  };

  const getStateColor = (state: AdapterState) => {
    switch (state) {
      case 'unloaded': return 'bg-gray-100 text-gray-800';
      case 'cold': return 'bg-blue-100 text-blue-800';
      case 'warm': return 'bg-orange-100 text-orange-800';
      case 'hot': return 'bg-red-100 text-red-800';
      case 'resident': return 'bg-purple-100 text-purple-800';
      default: return 'bg-gray-100 text-gray-800';
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

  // Calculate statistics
  const stateStats = adapters.reduce((acc, adapter) => {
    const state = (adapter.state ?? 'unloaded') as AdapterState;
    acc[state] = (acc[state] || 0) + 1;
    return acc;
  }, {} as Record<AdapterState, number>);

  const categoryStats = adapters.reduce((acc, adapter) => {
    const cat = (adapter.category ?? 'code') as AdapterCategory;
    acc[cat] = (acc[cat] || 0) + 1;
    return acc;
  }, {} as Record<AdapterCategory, number>);

  const memoryByState = adapters.reduce((acc, adapter) => {
    const state = (adapter.state ?? 'unloaded') as AdapterState;
    acc[state] = (acc[state] || 0) + (adapter.memory_bytes ?? 0);
    return acc;
  }, {} as Record<AdapterState, number>);

  const memoryByCategory = adapters.reduce((acc, adapter) => {
    const cat = (adapter.category ?? 'code') as AdapterCategory;
    acc[cat] = (acc[cat] || 0) + (adapter.memory_bytes ?? 0);
    return acc;
  }, {} as Record<AdapterCategory, number>);

  const totalMemoryUsed = adapters.reduce((sum, adapter) => sum + (adapter.memory_bytes ?? 0), 0);
  const memoryUsagePercent = totalMemory > 0 ? (totalMemoryUsed / totalMemory) * 100 : 0;

  const states: AdapterState[] = ['unloaded', 'cold', 'warm', 'hot', 'resident'];
  const categories: AdapterCategory[] = ['code', 'framework', 'codebase', 'ephemeral'];

  return (
    <div className="space-y-6">
      {/* Memory Usage Overview */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center">
            <MemoryStick className="mr-2 h-5 w-5" />
            Memory Usage Overview
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
            <Progress value={memoryUsagePercent} className="h-2" />
            <div className="text-xs text-muted-foreground">
              {memoryUsagePercent.toFixed(1)}% of total memory allocated
            </div>
          </div>
        </CardContent>
      </Card>

      {/* State Distribution */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center">
            <Activity className="mr-2 h-5 w-5" />
            Adapter State Distribution
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="space-y-4">
            {states.map((state) => {
              const count = stateStats[state] || 0;
              const memory = memoryByState[state] || 0;
              const percentage = adapters.length > 0 ? (count / adapters.length) * 100 : 0;
              
              return (
                <div key={state} className="flex items-center justify-between p-3 rounded-lg border">
                  <div className="flex items-center space-x-3">
                    {getStateIcon(state)}
                    <div>
                      <div className="font-medium capitalize">{state}</div>
                      <div className="text-sm text-muted-foreground">
                        {Math.round(memory / 1024 / 1024)} MB allocated
                      </div>
                    </div>
                  </div>
                  <div className="flex items-center space-x-3">
                    <Badge className={getStateColor(state)}>
                      {count} adapters
                    </Badge>
                    <div className="w-20">
                      <Progress value={percentage} className="h-2" />
                    </div>
                    <span className="text-sm text-muted-foreground w-12 text-right">
                      {percentage.toFixed(1)}%
                    </span>
                  </div>
                </div>
              );
            })}
          </div>
        </CardContent>
      </Card>

      {/* Category Distribution */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center">
            <Target className="mr-2 h-5 w-5" />
            Category Distribution
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="space-y-4">
            {categories.map((category) => {
              const count = categoryStats[category] || 0;
              const memory = memoryByCategory[category] || 0;
              const percentage = adapters.length > 0 ? (count / adapters.length) * 100 : 0;
              
              return (
                <div key={category} className="flex items-center justify-between p-3 rounded-lg border">
                  <div className="flex items-center space-x-3">
                    {getCategoryIcon(category)}
                    <div>
                      <div className="font-medium capitalize">{category}</div>
                      <div className="text-sm text-muted-foreground">
                        {Math.round(memory / 1024 / 1024)} MB allocated
                      </div>
                    </div>
                  </div>
                  <div className="flex items-center space-x-3">
                    <Badge className={getCategoryColor(category)}>
                      {count} adapters
                    </Badge>
                    <div className="w-20">
                      <Progress value={percentage} className="h-2" />
                    </div>
                    <span className="text-sm text-muted-foreground w-12 text-right">
                      {percentage.toFixed(1)}%
                    </span>
                  </div>
                </div>
              );
            })}
          </div>
        </CardContent>
      </Card>

      {/* Performance Indicators */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Hot Adapters</CardTitle>
            <Flame className="h-4 w-4 text-red-500" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{stateStats.hot || 0}</div>
            <p className="text-xs text-muted-foreground">
              Frequently used adapters
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Cold Adapters</CardTitle>
            <Snowflake className="h-4 w-4 text-blue-500" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{stateStats.cold || 0}</div>
            <p className="text-xs text-muted-foreground">
              Loaded but inactive
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Protected Adapters</CardTitle>
            <Anchor className="h-4 w-4 text-purple-500" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {adapters.filter(a => a.pinned).length}
            </div>
            <p className="text-xs text-muted-foreground">
              Will not be removed
            </p>
          </CardContent>
        </Card>
      </div>

      {/* Memory Pressure Indicator */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center">
            <TrendingUp className="mr-2 h-5 w-5" />
            Memory Pressure Analysis
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="space-y-4">
            <div className="flex items-center justify-between">
              <span className="text-sm font-medium">Memory Pressure Level</span>
              <Badge 
                className={
                  memoryUsagePercent > 80 
                    ? 'bg-red-100 text-red-800' 
                    : memoryUsagePercent > 60 
                    ? 'bg-yellow-100 text-yellow-800'
                    : 'bg-green-100 text-green-800'
                }
              >
                {memoryUsagePercent > 80 ? 'High' : memoryUsagePercent > 60 ? 'Medium' : 'Low'}
              </Badge>
            </div>
            
            {memoryUsagePercent > 80 && (
              <Alert variant="destructive">
                <TrendingUp className="icon-standard" />
                <AlertDescription>
                  <span className="font-medium">High Memory Pressure</span>
                  <p className="text-xs mt-1">
                    Consider evicting cold or unused adapters to free memory.
                  </p>
                </AlertDescription>
              </Alert>
            )}

            {memoryUsagePercent > 60 && memoryUsagePercent <= 80 && (
              <div className="status-indicator status-warning">
                <TrendingUp className="icon-standard" />
                <div>
                  <span className="font-medium">Medium Memory Pressure</span>
                  <p className="text-xs mt-1">
                    Monitor memory usage and consider proactive eviction.
                  </p>
                </div>
              </div>
            )}

            {memoryUsagePercent <= 60 && (
              <div className="status-indicator status-success">
                <TrendingDown className="icon-standard" />
                <div>
                  <span className="font-medium">Low Memory Pressure</span>
                  <p className="text-xs mt-1">
                    Memory usage is within acceptable limits.
                  </p>
                </div>
              </div>
            )}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
