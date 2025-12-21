import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Copy } from 'lucide-react';
import type { TraceResponseV1 } from '@/api/types';

interface TraceSummaryPanelProps {
  trace: TraceResponseV1;
  onExport?: () => void;
}

function copy(text: string) {
  if (navigator?.clipboard?.writeText) {
    navigator.clipboard.writeText(text);
  }
}

export function TraceSummaryPanel({ trace, onExport }: TraceSummaryPanelProps) {
  const digestItems = [
    { label: 'Context digest', value: trace.context_digest },
    { label: 'Policy digest', value: trace.policy_digest },
  ];

  return (
    <Card>
      <CardHeader className="flex flex-col gap-2 md:flex-row md:items-center md:justify-between">
        <div>
          <CardTitle className="text-base">Trace Summary</CardTitle>
          <CardDescription>Inspect digests and runtime metadata for this trace.</CardDescription>
        </div>
        <div className="flex items-center gap-2">
          <Badge variant="outline">Backend: {trace.backend_id}</Badge>
          <Badge variant="outline">Kernel: {trace.kernel_version_id}</Badge>
          <Badge variant="outline">Tokens: {trace.tokens.length}</Badge>
          <Button size="sm" onClick={onExport} disabled={!onExport}>
            Export Evidence Bundle
          </Button>
        </div>
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="text-sm text-muted-foreground">
          Trace ID <span className="font-mono text-primary">{trace.trace_id}</span>
        </div>
        <div className="grid gap-3 md:grid-cols-2">
          {digestItems.map((item) => (
            <div key={item.label} className="flex items-start justify-between rounded-md border p-3">
              <div>
                <div className="text-xs text-muted-foreground">{item.label}</div>
                <div className="font-mono text-sm break-all">{item.value}</div>
              </div>
              <Button
                variant="ghost"
                size="icon"
                className="h-8 w-8"
                onClick={() => copy(item.value)}
                aria-label={`Copy ${item.label}`}
              >
                <Copy className="h-4 w-4" />
              </Button>
            </div>
          ))}
        </div>
      </CardContent>
    </Card>
  );
}
