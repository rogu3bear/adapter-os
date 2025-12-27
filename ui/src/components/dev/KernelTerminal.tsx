import { useCallback, useMemo, useState } from 'react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { TerminalOutput } from '@/components/TerminalOutput';
import { useSSE } from '@/hooks/realtime/useSSE';
import { cn } from '@/lib/utils';
import { ChevronDown, ChevronUp, RefreshCw, Terminal } from 'lucide-react';

interface KernelTerminalProps {
  visible: boolean;
  workerId?: string | null;
}

export function KernelTerminal({ visible, workerId }: KernelTerminalProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [logs, setLogs] = useState<string[]>([]);

  const endpoint = useMemo(() => {
    const base = '/v1/stream/worker-logs';
    if (workerId) {
      return `${base}?worker_id=${encodeURIComponent(workerId)}`;
    }
    return base;
  }, [workerId]);

  const handleMessage = useCallback((event: unknown) => {
    let line: string | null = null;
    if (typeof event === 'string') {
      line = event;
    } else if (event && typeof event === 'object') {
      const record = event as Record<string, unknown>;
      if (typeof record.stdout === 'string') {
        line = record.stdout;
      } else if (typeof record.line === 'string') {
        line = record.line;
      } else if (typeof record.message === 'string') {
        line = record.message;
      }
    }

    if (!line) {
      try {
        line = JSON.stringify(event);
      } catch {
        line = '[unstructured log event]';
      }
    }

    setLogs(prev => [...prev.slice(-199), line!]);
  }, []);

  const { connected, error, reconnect } = useSSE(endpoint, {
    enabled: visible && isOpen,
    onMessage: handleMessage,
  });

  return (
    <div
      className={cn(
        'fixed bottom-3 left-3 right-3 z-40',
        'drop-shadow-lg transition-opacity'
      )}
      aria-live="polite"
    >
      <div className="flex items-center gap-2">
        <Button
          size="sm"
          variant="secondary"
          onClick={() => setIsOpen(prev => !prev)}
          className="shadow-sm"
          data-cy="kernel-terminal-toggle"
        >
          <Terminal className="mr-2 h-4 w-4" />
          {isOpen ? 'Hide Terminal' : 'Terminal'}
          {isOpen ? <ChevronDown className="ml-2 h-4 w-4" /> : <ChevronUp className="ml-2 h-4 w-4" />}
        </Button>
        <Badge variant={connected ? 'default' : 'secondary'} className="text-[11px] uppercase tracking-wide">
          {connected ? 'Streaming stdout' : 'Paused'}
        </Badge>
        {error && (
          <>
            <Badge variant="destructive" className="text-[11px]">SSE error</Badge>
            <Button size="icon" variant="ghost" onClick={reconnect} aria-label="Reconnect worker logs">
              <RefreshCw className="h-4 w-4" />
            </Button>
          </>
        )}
      </div>
      {isOpen && (
        <div className="mt-2 rounded-md border border-border bg-background/95 shadow-2xl backdrop-blur-sm">
          <div className="px-3 py-2 text-xs text-muted-foreground flex items-center justify-between">
            <span className="font-mono">
              {workerId ? `worker:${workerId}` : 'worker:active'} • tail -f stdout
            </span>
            <span className="text-[10px] uppercase">Kernel Stream</span>
          </div>
          <TerminalOutput logs={logs} maxHeight="320px" />
        </div>
      )}
    </div>
  );
}
