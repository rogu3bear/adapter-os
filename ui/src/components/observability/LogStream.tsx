import React, { useEffect, useState, useRef } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '../ui/card';
import { Button } from '../ui/button';
import { Badge } from '../ui/badge';
import { Input } from '../ui/input';
import apiClient from '../../api/client';
import { useSSE } from '../../hooks/useSSE';
import { logger, toError } from '../../utils/logger';

interface LogEvent {
  id: string;
  timestamp: string;
  event_type: string;
  level: 'Debug' | 'Info' | 'Warn' | 'Error' | 'Critical';
  message: string;
  component?: string;
  tenant_id?: string;
  trace_id?: string;
}

export function LogStream() {
  const [logs, setLogs] = useState<LogEvent[]>([]);
  const [filters, setFilters] = useState({
    level: '',
    component: '',
    tenant_id: '',
    event_type: '',
    limit: 100,
    start_time: '',
    end_time: '',
  });
  const [autoScroll, setAutoScroll] = useState(true);
  const logsEndRef = useRef<HTMLDivElement>(null);

  // SSE stream for live logs
  const { data: streamData } = useSSE<LogEvent>('/api/logs/stream');

  useEffect(() => {
    if (!streamData) return;
    setLogs((prev) => {
      const updated = [streamData, ...prev].slice(0, filters.limit);
      return updated;
    });
  }, [streamData, filters.limit]);

  useEffect(() => {
    const fetchLogs = async () => {
      try {
        const queryParams: any = {
          limit: filters.limit,
        };

        if (filters.level) queryParams.level = filters.level;
        if (filters.component) queryParams.component = filters.component;
        if (filters.tenant_id) queryParams.tenant_id = filters.tenant_id;
        if (filters.event_type) queryParams.event_type = filters.event_type;

        const data = await apiClient.queryLogs(queryParams);

        // Transform UnifiedTelemetryEvent to LogEvent format
        const transformedLogs: LogEvent[] = data.map(event => ({
          id: event.id,
          timestamp: event.timestamp,
          event_type: event.event_type,
          level: event.level,
          message: event.message,
          component: event.component,
          tenant_id: event.tenant_id,
          trace_id: event.trace_id,
        }));

        setLogs(transformedLogs);
      } catch (err) {
        logger.error('Failed to fetch logs', { component: 'LogStream', operation: 'fetchLogs' }, toError(err));
      }
    };

    fetchLogs();
  }, [filters]);

  useEffect(() => {
    if (autoScroll && logsEndRef.current) {
      logsEndRef.current.scrollIntoView({ behavior: 'smooth' });
    }
  }, [logs, autoScroll]);

  const getLevelColor = (level: string) => {
    switch (level) {
      case 'Error':
      case 'Critical':
        return 'destructive';
      case 'Warn':
        return 'default';
      default:
        return 'secondary';
    }
  };

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between">
          <CardTitle>Log Stream</CardTitle>
          <div className="flex items-center gap-2">
            <Button
              variant="outline"
              size="sm"
              onClick={() => setAutoScroll(!autoScroll)}
            >
              {autoScroll ? 'Auto-scroll: ON' : 'Auto-scroll: OFF'}
            </Button>
          </div>
        </div>
      </CardHeader>
      <CardContent>
        <div className="space-y-4">
          {/* Filters */}
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-2">
            <Input
              placeholder="Filter by level..."
              value={filters.level}
              onChange={(e) => setFilters({ ...filters, level: e.target.value })}
            />
            <Input
              placeholder="Filter by component..."
              value={filters.component}
              onChange={(e) => setFilters({ ...filters, component: e.target.value })}
            />
            <Input
              placeholder="Filter by tenant..."
              value={filters.tenant_id}
              onChange={(e) => setFilters({ ...filters, tenant_id: e.target.value })}
            />
            <Input
              placeholder="Filter by event type..."
              value={filters.event_type}
              onChange={(e) => setFilters({ ...filters, event_type: e.target.value })}
            />
            <Input
              type="number"
              placeholder="Limit"
              value={filters.limit}
              onChange={(e) => setFilters({ ...filters, limit: parseInt(e.target.value) || 100 })}
            />
            <Input
              type="datetime-local"
              placeholder="Start time"
              value={filters.start_time}
              onChange={(e) => setFilters({ ...filters, start_time: e.target.value })}
            />
            <Input
              type="datetime-local"
              placeholder="End time"
              value={filters.end_time}
              onChange={(e) => setFilters({ ...filters, end_time: e.target.value })}
            />
            <Button
              variant="outline"
              onClick={() => setFilters({
                level: '',
                component: '',
                tenant_id: '',
                event_type: '',
                limit: 100,
                start_time: '',
                end_time: '',
              })}
              className="w-full"
            >
              Clear Filters
            </Button>
          </div>

          {/* Logs */}
          <div className="space-y-1 max-h-96 overflow-y-auto font-mono text-sm">
            {logs.map((log) => (
              <div
                key={log.id}
                className="flex items-start gap-2 p-2 rounded hover:bg-muted/50"
              >
                <Badge variant={getLevelColor(log.level)} className="shrink-0">
                  {log.level}
                </Badge>
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="text-muted-foreground">
                      {new Date(log.timestamp).toLocaleTimeString()}
                    </span>
                    {log.component && (
                      <span className="text-xs text-muted-foreground">
                        [{log.component}]
                      </span>
                    )}
                    {log.tenant_id && (
                      <span className="text-xs text-muted-foreground">
                        tenant:{log.tenant_id}
                      </span>
                    )}
                  </div>
                  <div className="mt-1">{log.message}</div>
                  {log.trace_id && (
                    <div className="text-xs text-muted-foreground mt-1">
                      trace: {log.trace_id}
                    </div>
                  )}
                </div>
              </div>
            ))}
            {logs.length === 0 && (
              <div className="text-center py-8 text-muted-foreground">
                No logs available
              </div>
            )}
            <div ref={logsEndRef} />
          </div>
        </div>
      </CardContent>
    </Card>
  );
}
