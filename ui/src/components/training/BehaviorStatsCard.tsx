// Behavior Stats Card
//
// Dashboard widget showing behavior event statistics with charts.

import React from 'react';
import { Activity, TrendingUp } from 'lucide-react';
import { useBehaviorStats } from '@/hooks/useBehaviorTraining';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';

interface BehaviorStatsCardProps {
  tenantId?: string;
}

const CATEGORY_COLORS: Record<string, string> = {
  promoted: 'var(--success)',
  demoted: 'var(--warning)',
  evicted: 'var(--destructive)',
  pinned: 'var(--info)',
  recovered: 'var(--primary)',
  ttl_expired: 'var(--muted-foreground)',
};

export function BehaviorStatsCard({ tenantId }: BehaviorStatsCardProps) {
  const { data: stats, isLoading } = useBehaviorStats(tenantId);

  if (isLoading) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Behavior Statistics</CardTitle>
          <CardDescription>Loading...</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="text-center py-8 text-muted-foreground">Loading statistics...</div>
        </CardContent>
      </Card>
    );
  }

  if (!stats) {
    return null;
  }

  const totalEvents = stats.total_events;
  const categoryEntries = Object.entries(stats.by_category);
  const maxCategoryCount = Math.max(...categoryEntries.map(([_, count]) => count as number), 1);

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between">
          <div>
            <CardTitle>Behavior Statistics</CardTitle>
            <CardDescription>Adapter lifecycle event summary</CardDescription>
          </div>
          <Activity className="h-5 w-5 text-muted-foreground" />
        </div>
      </CardHeader>
      <CardContent className="space-y-6">
        {/* Total events */}
        <div className="text-center py-4 border-b">
          <div className="text-4xl font-bold">{totalEvents.toLocaleString()}</div>
          <div className="text-sm text-muted-foreground mt-1">Total Events</div>
        </div>

        {/* Events by category */}
        <div className="space-y-3">
          <div className="text-sm font-semibold">Events by Category</div>
          {categoryEntries.length === 0 ? (
            <div className="text-sm text-muted-foreground">No events recorded yet</div>
          ) : (
            categoryEntries.map(([category, count]) => {
              const percentage = ((count as number) / totalEvents) * 100;
              const barWidth = ((count as number) / maxCategoryCount) * 100;

              return (
                <div key={category} className="space-y-1">
                  <div className="flex items-center justify-between text-sm">
                    <span className="capitalize">{category}</span>
                    <span className="font-mono">{count as number}</span>
                  </div>
                  <div className="h-2 bg-muted rounded-full overflow-hidden">
                    <div
                      className="h-full rounded-full transition-all duration-300"
                      style={{
                        width: `${barWidth}%`,
                        backgroundColor: CATEGORY_COLORS[category] || 'hsl(var(--primary))',
                      }}
                    />
                  </div>
                  <div className="text-xs text-muted-foreground">{percentage.toFixed(1)}%</div>
                </div>
              );
            })
          )}
        </div>

        {/* Top state transitions */}
        {stats.by_state_transition && stats.by_state_transition.length > 0 && (
          <div className="space-y-3">
            <div className="text-sm font-semibold flex items-center gap-2">
              <TrendingUp className="h-4 w-4" />
              Top State Transitions
            </div>
            <div className="space-y-2">
              {stats.by_state_transition.slice(0, 5).map((transition, idx) => (
                <div
                  key={`${transition.from}-${transition.to}-${idx}`}
                  className="flex items-center justify-between p-2 bg-muted/50 rounded"
                >
                  <div className="flex items-center gap-2">
                    <Badge variant="outline" className="text-xs">
                      {transition.from}
                    </Badge>
                    <span className="text-xs text-muted-foreground">→</span>
                    <Badge variant="outline" className="text-xs">
                      {transition.to}
                    </Badge>
                  </div>
                  <span className="text-sm font-mono">{transition.count}</span>
                </div>
              ))}
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

