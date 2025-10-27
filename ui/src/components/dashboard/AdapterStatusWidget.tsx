import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '../ui/card';
import { Badge } from '../ui/badge';
import { Progress } from '../ui/progress';
import { Layers, TrendingUp, Activity } from 'lucide-react';

interface AdapterStateCount {
  state: string;
  count: number;
  color: string;
}

export function AdapterStatusWidget() {
  // Mock adapter state data - in production, fetch from API
  const stateDistribution: AdapterStateCount[] = [
    { state: 'hot', count: 5, color: 'bg-red-500' },
    { state: 'warm', count: 12, color: 'bg-orange-500' },
    { state: 'cold', count: 23, color: 'bg-blue-500' },
    { state: 'unloaded', count: 45, color: 'bg-gray-400' }
  ];

  const totalAdapters = stateDistribution.reduce((sum, s) => sum + s.count, 0);
  const activeAdapters = stateDistribution
    .filter(s => ['hot', 'warm'].includes(s.state))
    .reduce((sum, s) => sum + s.count, 0);

  const memoryUsage = 67; // Mock percentage
  const avgActivationRate = 0.42; // Mock rate

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center justify-between">
          <span>Adapter Status</span>
          <Badge variant="outline">
            {activeAdapters} Active
          </Badge>
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        {/* State Distribution */}
        <div>
          <div className="flex items-center justify-between text-sm mb-2">
            <span className="text-muted-foreground">Lifecycle States</span>
            <span className="font-medium">{totalAdapters} total</span>
          </div>
          <div className="flex h-2 rounded-full overflow-hidden">
            {stateDistribution.map((state) => (
              <div
                key={state.state}
                className={state.color}
                style={{ width: `${(state.count / totalAdapters) * 100}%` }}
                title={`${state.state}: ${state.count}`}
              />
            ))}
          </div>
          <div className="grid grid-cols-2 gap-2 mt-2">
            {stateDistribution.map((state) => (
              <div key={state.state} className="flex items-center gap-2 text-xs">
                <div className={`w-2 h-2 rounded-full ${state.color}`} />
                <span className="text-muted-foreground capitalize">{state.state}:</span>
                <span className="font-medium">{state.count}</span>
              </div>
            ))}
          </div>
        </div>

        {/* Memory Usage */}
        <div>
          <div className="flex items-center gap-2 mb-2">
            <Layers className="h-4 w-4 text-muted-foreground" />
            <span className="text-sm text-muted-foreground">Memory Usage</span>
          </div>
          <Progress value={memoryUsage} className="h-2" />
          <p className="text-xs text-muted-foreground mt-1">
            {memoryUsage}% of adapter memory in use
          </p>
        </div>

        {/* Activation Rate */}
        <div className="flex items-center justify-between p-3 bg-muted rounded-lg">
          <div className="flex items-center gap-2">
            <Activity className="h-4 w-4 text-muted-foreground" />
            <span className="text-sm font-medium">Avg Activation</span>
          </div>
          <div className="flex items-center gap-1">
            <span className="text-lg font-semibold">{(avgActivationRate * 100).toFixed(1)}%</span>
            <TrendingUp className="h-4 w-4 text-green-600" />
          </div>
        </div>
      </CardContent>
    </Card>
  );
}

