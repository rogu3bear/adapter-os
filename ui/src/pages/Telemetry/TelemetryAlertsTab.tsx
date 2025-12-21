import { Link } from 'react-router-dom';
import { useMutation, useQuery } from '@tanstack/react-query';
import { apiClient } from '@/api/services';
import { buildReplayLink } from '@/utils/navLinks';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import type { Alert } from '@/api/types';
import { useToast } from '@/hooks/use-toast';

export default function TelemetryAlertsTab() {
  const { toast } = useToast();
  const { data, refetch, isLoading } = useQuery({
    queryKey: ['alerts'],
    queryFn: () => apiClient.listAlerts({ limit: 50 }),
  });

  const acknowledge = useMutation({
    mutationFn: (alertId: string) => apiClient.acknowledgeAlert(alertId, { alert_id: alertId, acknowledged_by: 'ui' }),
    onSuccess: () => {
      toast({ title: 'Alert acknowledged' });
      refetch();
    },
  });

  const resolve = useMutation({
    mutationFn: (alertId: string) => apiClient.resolveAlert(alertId, { alert_id: alertId, resolved_by: 'ui' }),
    onSuccess: () => {
      toast({ title: 'Alert resolved' });
      refetch();
    },
  });

  if (isLoading) {
    return <div className="text-sm text-muted-foreground">Loading alerts…</div>;
  }

  const alerts = data ?? [];
  if (!alerts.length) {
    return <div className="text-sm text-muted-foreground">No alerts found.</div>;
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Active alerts</CardTitle>
      </CardHeader>
      <CardContent>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Type</TableHead>
              <TableHead>Severity</TableHead>
              <TableHead>Status</TableHead>
              <TableHead>Message</TableHead>
              <TableHead>Investigate</TableHead>
              <TableHead className="text-right">Actions</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {alerts.map((alert: Alert) => (
              <TableRow key={alert.id}>
                <TableCell className="font-medium">{alert.source ?? 'system'}</TableCell>
                <TableCell>
                  <Badge variant="outline">{alert.severity ?? 'info'}</Badge>
                </TableCell>
                <TableCell>{alert.status ?? (alert.acknowledged ? 'acknowledged' : 'active')}</TableCell>
                <TableCell className="text-sm text-muted-foreground">{alert.message}</TableCell>
                <TableCell>
                  <Link to={buildReplayLink()} className="text-xs underline underline-offset-4">
                    View affected sessions
                  </Link>
                </TableCell>
                <TableCell className="text-right space-x-2">
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => acknowledge.mutate(alert.id)}
                    disabled={acknowledge.isPending}
                  >
                    Ack
                  </Button>
                  <Button
                    size="sm"
                    onClick={() => resolve.mutate(alert.id)}
                    disabled={resolve.isPending}
                    variant="secondary"
                  >
                    Resolve
                  </Button>
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  );
}
