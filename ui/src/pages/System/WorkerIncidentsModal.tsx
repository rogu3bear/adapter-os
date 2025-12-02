import { useState } from 'react';
import { Modal } from '@/components/shared/Modal';
import { Badge } from '@/components/ui/badge';
import { Card, CardContent } from '@/components/ui/card';
import { Skeleton } from '@/components/ui/skeleton';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Button } from '@/components/ui/button';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { useWorkerIncidents } from '@/hooks/useSystemMetrics';

interface WorkerIncidentsModalProps {
  workerId: string;
  open: boolean;
  onClose: () => void;
}

export default function WorkerIncidentsModal({ workerId, open, onClose }: WorkerIncidentsModalProps) {
  const { data: incidents, isLoading } = useWorkerIncidents(workerId, open);

  const getIncidentTypeBadge = (type: string) => {
    const variant =
      type === 'fatal' || type === 'crash'
        ? 'destructive'
        : type === 'hung' || type === 'degraded'
        ? 'warning'
        : type === 'recovered'
        ? 'default'
        : 'secondary';
    return <Badge variant={variant}>{type.toUpperCase()}</Badge>;
  };

  return (
    <Modal
      open={open}
      onOpenChange={onClose}
      title="Worker Incidents"
      description={`Incident history for worker ${workerId}`}
      size="xl"
      footer={
        <Button variant="outline" onClick={onClose}>
          Close
        </Button>
      }
    >
      <ScrollArea className="h-[600px]">
        {isLoading ? (
          <div className="space-y-4 p-4">
            {[...Array(5)].map((_, i) => (
              <Skeleton key={i} className="h-32 w-full" />
            ))}
          </div>
        ) : incidents && incidents.length > 0 ? (
          <div className="space-y-4 p-4">
            {incidents.map((incident) => (
              <IncidentCard key={incident.id} incident={incident} />
            ))}
          </div>
        ) : (
          <div className="flex items-center justify-center h-full text-muted-foreground py-8">
            No incidents recorded for this worker
          </div>
        )}
      </ScrollArea>
    </Modal>
  );
}

interface IncidentCardProps {
  incident: {
    id: string;
    incident_type: string;
    reason: string;
    backtrace_snippet?: string;
    latency_at_incident_ms?: number;
    created_at: string;
  };
}

function IncidentCard({ incident }: IncidentCardProps) {
  const [isBacktraceOpen, setIsBacktraceOpen] = useState(false);

  const getIncidentTypeBadge = (type: string) => {
    const variant =
      type === 'fatal' || type === 'crash'
        ? 'destructive'
        : type === 'hung' || type === 'degraded'
        ? 'warning'
        : type === 'recovered'
        ? 'default'
        : 'secondary';
    return <Badge variant={variant}>{type.toUpperCase()}</Badge>;
  };

  return (
    <Card className={incident.incident_type === 'fatal' || incident.incident_type === 'crash' ? 'border-destructive' : ''}>
      <CardContent className="pt-6">
        <div className="space-y-3">
          <div className="flex items-center justify-between">
            {getIncidentTypeBadge(incident.incident_type)}
            <span className="text-sm text-muted-foreground">
              {new Date(incident.created_at).toLocaleString()}
            </span>
          </div>

          <div className="text-sm">
            <span className="font-medium">Reason:</span> {incident.reason}
          </div>

          {incident.latency_at_incident_ms !== undefined && (
            <div className="text-sm">
              <span className="font-medium">Latency at incident:</span>{' '}
              <span className={incident.latency_at_incident_ms > 1000 ? 'text-destructive font-semibold' : ''}>
                {incident.latency_at_incident_ms}ms
              </span>
            </div>
          )}

          {incident.backtrace_snippet && (
            <Collapsible open={isBacktraceOpen} onOpenChange={setIsBacktraceOpen}>
              <CollapsibleTrigger asChild>
                <Button variant="ghost" size="sm" className="w-full justify-start">
                  {isBacktraceOpen ? (
                    <ChevronDown className="h-4 w-4 mr-2" />
                  ) : (
                    <ChevronRight className="h-4 w-4 mr-2" />
                  )}
                  <span className="font-medium">Stack Trace</span>
                </Button>
              </CollapsibleTrigger>
              <CollapsibleContent className="mt-2">
                <pre className="text-xs bg-muted p-3 rounded overflow-x-auto font-mono">
                  {incident.backtrace_snippet}
                </pre>
              </CollapsibleContent>
            </Collapsible>
          )}
        </div>
      </CardContent>
    </Card>
  );
}
