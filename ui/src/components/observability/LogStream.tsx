import React, { useEffect, useState, useRef } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { apiClient } from '@/api/services';
import { useLiveData } from '@/hooks/realtime/useLiveData';
import { logger, toError } from '@/utils/logger';

const LOG_LEVELS = ['', 'error', 'warn', 'info', 'debug', 'trace'] as const;
const LOG_LEVEL_LABELS: Record<string, string> = {
  '': 'All Levels',
  'error': 'Error',
  'warn': 'Warning',
  'info': 'Info',
  'debug': 'Debug',
  'trace': 'Trace',
};

interface LogEvent {
  id: string;
  timestamp: string;
  event_type: string;
  level: string;
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
  useLiveData({
    sseEndpoint: '/v1/logs/stream',
    sseEventType: 'log',
    fetchFn: async () => {
      // Polling fallback - fetch logs
      const queryParams: Record<string, string | number> = {
        limit: filters.limit,
      };

      if (filters.level) queryParams.level = filters.level;
      if (filters.component) queryParams.component = filters.component;
      if (filters.tenant_id) queryParams.tenant_id = filters.tenant_id;
      if (filters.event_type) queryParams.event_type = filters.event_type;

      const data = await apiClient.queryLogs(queryParams);

      // Transform UnifiedTelemetryEvent to LogEvent format
      const transformedLogs: LogEvent[] = data.map(event => ({
        id: event.id ?? '',
        timestamp: event.timestamp,
        event_type: event.event_type,
        level: event.level ?? '',
        message: event.message ?? '',
        component: event.component,
        tenant_id: event.tenant_id,
        trace_id: event.trace_id,
      }));

      return transformedLogs;
    },
    enabled: true,
    pollingSpeed: 'normal',
    onSSEMessage: (event) => {
      const logEvent = event as LogEvent;
      setLogs((prev) => {
        const updated = [logEvent, ...prev].slice(0, filters.limit);
        return updated;
      });
    },
  });

  useEffect(() => {
    const fetchLogs = async () => {
      try {
        const queryParams: Record<string, string | number> = {
          limit: filters.limit,
        };

        if (filters.level) queryParams.level = filters.level;
        if (filters.component) queryParams.component = filters.component;
        if (filters.tenant_id) queryParams.tenant_id = filters.tenant_id;
        if (filters.event_type) queryParams.event_type = filters.event_type;

        const data = await apiClient.queryLogs(queryParams);

        // Transform UnifiedTelemetryEvent to LogEvent format
        const transformedLogs: LogEvent[] = data.map(event => ({
          id: event.id ?? '',
          timestamp: event.timestamp,
          event_type: event.event_type,
          level: event.level ?? '',
          message: event.message ?? '',
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

  const getLevelColor = (level: string): "destructive" | "default" | "secondary" | "outline" => {
    const normalizedLevel = level.toLowerCase();
    switch (normalizedLevel) {
      case 'error':
      case 'critical':
        return 'destructive';
      case 'warn':
      case 'warning':
        return 'default';
      case 'info':
        return 'secondary';
      case 'debug':
      case 'trace':
        return 'outline';
      default:
        return 'secondary';
    }
  };

  const getLevelLabel = (level: string): string => {
    const normalizedLevel = level.toLowerCase();
    return LOG_LEVEL_LABELS[normalizedLevel] || level.toUpperCase();
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
            <Select
              value={filters.level}
              onValueChange={(value) => setFilters({ ...filters, level: value === 'all' ? '' : value })}
            >
              <SelectTrigger>
                <SelectValue placeholder="All Levels" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">All Levels</SelectItem>
                <SelectItem value="error">
                  <span className="flex items-center gap-2">
                    <span className="h-2 w-2 rounded-full bg-red-500" />
                    Error
                  </span>
                </SelectItem>
                <SelectItem value="warn">
                  <span className="flex items-center gap-2">
                    <span className="h-2 w-2 rounded-full bg-yellow-500" />
                    Warning
                  </span>
                </SelectItem>
                <SelectItem value="info">
                  <span className="flex items-center gap-2">
                    <span className="h-2 w-2 rounded-full bg-blue-500" />
                    Info
                  </span>
                </SelectItem>
                <SelectItem value="debug">
                  <span className="flex items-center gap-2">
                    <span className="h-2 w-2 rounded-full bg-gray-500" />
                    Debug
                  </span>
                </SelectItem>
                <SelectItem value="trace">
                  <span className="flex items-center gap-2">
                    <span className="h-2 w-2 rounded-full bg-gray-400" />
                    Trace
                  </span>
                </SelectItem>
              </SelectContent>
            </Select>
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
            <Select
              value={filters.limit.toString()}
              onValueChange={(value) => setFilters({ ...filters, limit: parseInt(value) })}
            >
              <SelectTrigger>
                <SelectValue placeholder="Limit" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="25">25 entries</SelectItem>
                <SelectItem value="50">50 entries</SelectItem>
                <SelectItem value="100">100 entries</SelectItem>
                <SelectItem value="250">250 entries</SelectItem>
                <SelectItem value="500">500 entries</SelectItem>
              </SelectContent>
            </Select>
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
                <Badge variant={getLevelColor(log.level)} className="shrink-0 min-w-[60px] justify-center">
                  {getLevelLabel(log.level)}
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
