import React, { useEffect, useState, useRef } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '../ui/card';
import { Button } from '../ui/button';
import { Badge } from '../ui/badge';
import { Input } from '../ui/input';
import apiClient from '../../api/client';
import { useSSE } from '../../hooks/useSSE';

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
    limit: 100,
  });
  const [autoScroll, setAutoScroll] = useState(true);
  const logsEndRef = useRef<HTMLDivElement>(null);

  // SSE stream for live logs
  const { data: streamData } = useSSE<LogEvent>('/api/logs/stream');

  useEffect(() => {
    if (streamData) {
      setLogs((prev) => {
        const updated = [streamData, ...prev].slice(0, filters.limit);
        return updated;
      });
    }
  }, [streamData, filters.limit]);

  useEffect(() => {
    const fetchLogs = async () => {
      try {
        const params = new URLSearchParams();
        if (filters.level) params.set('level', filters.level);
        if (filters.component) params.set('component', filters.component);
        params.set('limit', filters.limit.toString());
        
        const data = await apiClient.request<LogEvent[]>(`/api/logs/query?${params}`);
        setLogs(data);
      } catch (err) {
        console.error('Failed to fetch logs', err);
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
          <div className="flex gap-2">
            <Input
              placeholder="Filter by level..."
              value={filters.level}
              onChange={(e) => setFilters({ ...filters, level: e.target.value })}
              className="w-32"
            />
            <Input
              placeholder="Filter by component..."
              value={filters.component}
              onChange={(e) => setFilters({ ...filters, component: e.target.value })}
              className="flex-1"
            />
            <Input
              type="number"
              placeholder="Limit"
              value={filters.limit}
              onChange={(e) => setFilters({ ...filters, limit: parseInt(e.target.value) || 100 })}
              className="w-24"
            />
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
