import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '../ui/card';
import { Badge } from '../ui/badge';
import { Progress } from '../ui/progress';
import { Button } from '../ui/button';
import { AlertTriangle, Cpu, HardDrive, MemoryStick, Zap, TrendingUp, TrendingDown } from 'lucide-react';

interface MetricCardProps {
  title: string;
  value: string;
  subtitle: string;
  icon: React.ReactNode;
  trend?: 'up' | 'down' | 'stable';
  color?: 'green' | 'yellow' | 'red';
}

function MetricCard({ title, value, subtitle, icon, trend, color = 'green' }: MetricCardProps) {
  const trendIcon = trend === 'up' ? <TrendingUp className="h-3 w-3" /> :
                   trend === 'down' ? <TrendingDown className="h-3 w-3" /> : null;

  const colorClasses = {
    green: 'text-green-600',
    yellow: 'text-yellow-600',
    red: 'text-red-600'
  };

  return (
    <Card>
      <CardContent className="p-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center space-x-2">
            <div className={colorClasses[color]}>{icon}</div>
            <div>
              <p className="text-sm font-medium text-muted-foreground">{title}</p>
              <p className="text-2xl font-bold">{value}</p>
            </div>
          </div>
          {trendIcon && (
            <div className={`flex items-center ${trend === 'up' ? 'text-green-600' : 'text-red-600'}`}>
              {trendIcon}
            </div>
          )}
        </div>
        <p className="text-xs text-muted-foreground mt-2">{subtitle}</p>
      </CardContent>
    </Card>
  );
}

export default function DevOpsResourceDashboard() {
  return (
    <div className="space-y-6 h-full overflow-y-auto">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-lg font-semibold">System Resources</h2>
          <p className="text-sm text-muted-foreground">Monitor AdapterOS resource utilization</p>
        </div>
        <div className="flex items-center space-x-2">
          <Badge variant="outline" className="text-green-600 border-green-200">
            <div className="w-2 h-2 bg-green-500 rounded-full mr-2"></div>
            Healthy
          </Badge>
          <Button variant="outline" size="sm">
            Configure Alerts
          </Button>
        </div>
      </div>

      {/* System Health Overview */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        <MetricCard
          title="CPU Usage"
          value="67%"
          subtitle="8 cores active"
          icon={<Cpu className="h-5 w-5" />}
          trend="stable"
          color="green"
        />
        <MetricCard
          title="Memory"
          value="12.4GB"
          subtitle="of 16GB total"
          icon={<MemoryStick className="h-5 w-5" />}
          trend="up"
          color="yellow"
        />
        <MetricCard
          title="GPU Memory"
          value="8.2GB"
          subtitle="of 24GB total"
          icon={<Zap className="h-5 w-5" />}
          trend="stable"
          color="green"
        />
        <MetricCard
          title="Storage"
          value="234GB"
          subtitle="of 500GB free"
          icon={<HardDrive className="h-5 w-5" />}
          trend="stable"
          color="green"
        />
      </div>

      {/* Detailed Metrics */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        {/* CPU Breakdown */}
        <Card>
          <CardHeader>
            <CardTitle className="text-base flex items-center space-x-2">
              <Cpu className="h-4 w-4" />
              <span>CPU Core Utilization</span>
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-3">
            {[1, 2, 3, 4, 5, 6, 7, 8].map((core) => (
              <div key={core} className="flex items-center space-x-3">
                <span className="text-sm w-8">Core {core}</span>
                <Progress value={Math.random() * 100} className="flex-1" />
                <span className="text-sm w-12 text-right">
                  {Math.floor(Math.random() * 100)}%
                </span>
              </div>
            ))}
          </CardContent>
        </Card>

        {/* Memory Breakdown */}
        <Card>
          <CardHeader>
            <CardTitle className="text-base flex items-center space-x-2">
              <MemoryStick className="h-4 w-4" />
              <span>Memory Allocation</span>
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="space-y-2">
              <div className="flex justify-between text-sm">
                <span>System Memory</span>
                <span>12.4GB / 16GB</span>
              </div>
              <Progress value={77.5} />
            </div>

            <div className="space-y-2">
              <div className="flex justify-between text-sm">
                <span>GPU Memory</span>
                <span>8.2GB / 24GB</span>
              </div>
              <Progress value={34.2} />
            </div>

            <div className="pt-2 border-t">
              <div className="flex justify-between text-sm font-medium">
                <span>Adapter Cache</span>
                <span>3.2GB</span>
              </div>
              <div className="text-xs text-muted-foreground mt-1">
                5 active adapters loaded
              </div>
            </div>
          </CardContent>
        </Card>
      </div>

      {/* Active Alerts */}
      <Card>
        <CardHeader>
          <CardTitle className="text-base flex items-center space-x-2">
            <AlertTriangle className="h-4 w-4 text-yellow-500" />
            <span>Active Alerts</span>
            <Badge variant="secondary">2</Badge>
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="space-y-3">
            <div className="flex items-center justify-between p-3 bg-yellow-50 border border-yellow-200 rounded">
              <div className="flex items-center space-x-3">
                <AlertTriangle className="h-4 w-4 text-yellow-600" />
                <div>
                  <p className="text-sm font-medium">High Memory Usage</p>
                  <p className="text-xs text-muted-foreground">Memory utilization above 80%</p>
                </div>
              </div>
              <Badge variant="outline">Warning</Badge>
            </div>

            <div className="flex items-center justify-between p-3 bg-blue-50 border border-blue-200 rounded">
              <div className="flex items-center space-x-3">
                <HardDrive className="h-4 w-4 text-blue-600" />
                <div>
                  <p className="text-sm font-medium">Storage Maintenance</p>
                  <p className="text-xs text-muted-foreground">Scheduled cleanup in 2 hours</p>
                </div>
              </div>
              <Badge variant="outline">Info</Badge>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Performance Trends */}
      <Card>
        <CardHeader>
          <CardTitle className="text-base">Performance Trends (24h)</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-3 gap-4 text-center">
            <div>
              <div className="text-2xl font-bold text-green-600">98.5%</div>
              <div className="text-xs text-muted-foreground">Uptime</div>
            </div>
            <div>
              <div className="text-2xl font-bold text-blue-600">145ms</div>
              <div className="text-xs text-muted-foreground">Avg Response</div>
            </div>
            <div>
              <div className="text-2xl font-bold text-purple-600">2.3K</div>
              <div className="text-xs text-muted-foreground">Requests/Hour</div>
            </div>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
