import React, { useMemo, useState } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import { useMutation, useQuery } from '@tanstack/react-query';
import FeatureLayout from '@/layout/FeatureLayout';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import TelemetryPage from '@/pages/TelemetryPage';
import TelemetryViewerPage from '@/pages/TelemetryViewerPage';
import { telemetryTabOrder, telemetryTabToPath, TelemetryTab, resolveTelemetryTab } from '@/pages/Telemetry/tabs';
import apiClient from '@/api/client';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import type { Alert, TelemetryBundle, TelemetryEvent } from '@/api/types';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { useToast } from '@/hooks/use-toast';
import { Link } from 'react-router-dom';

export default function TelemetryShell() {
  const location = useLocation();
  const navigate = useNavigate();

  const activeTab: TelemetryTab = useMemo(
    () => resolveTelemetryTab(location.pathname, location.hash),
    [location.hash, location.pathname],
  );

  const tabPath = (tab: TelemetryTab) => telemetryTabToPath(tab);

  return (
    <FeatureLayout
      title="Telemetry"
      description="Event stream, viewer, and exports"
      maxWidth="xl"
    >
      <Tabs
        value={activeTab}
        onValueChange={(value: string) => {
          const tab = value as TelemetryTab;
          const next = tabPath(tab);
          const nextLocation = next.split('#')[0];
          if (nextLocation !== location.pathname || location.hash !== '') {
            navigate(next);
          }
        }}
      >
        <TabsList className="w-full grid grid-cols-2 md:grid-cols-5">
          {telemetryTabOrder.map(tab => (
            <TabsTrigger key={tab} value={tab}>
              {tab === 'event-stream' && 'Event Stream'}
              {tab === 'viewer' && 'Viewer'}
              {tab === 'alerts' && 'Alerts'}
              {tab === 'exports' && 'Exports'}
              {tab === 'filters' && 'Filters'}
            </TabsTrigger>
          ))}
        </TabsList>

        <TabsContent value="event-stream" className="mt-6">
          <TelemetryPage />
        </TabsContent>
        <TabsContent value="viewer" className="mt-6">
          <TelemetryViewerPage />
        </TabsContent>
        <TabsContent value="alerts" className="mt-6">
          <TelemetryAlertsTab />
        </TabsContent>
        <TabsContent value="exports" className="mt-6">
          <TelemetryExportsTab />
        </TabsContent>
        <TabsContent value="filters" className="mt-6">
          <TelemetryFiltersTab />
        </TabsContent>
      </Tabs>
    </FeatureLayout>
  );
}

function TelemetryAlertsTab() {
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
                <TableCell className="font-medium">{alert.type}</TableCell>
                <TableCell>
                  <Badge variant="outline">{alert.severity ?? 'info'}</Badge>
                </TableCell>
                <TableCell>{alert.status}</TableCell>
                <TableCell className="text-sm text-muted-foreground">{alert.message}</TableCell>
                <TableCell>
                  <Link to="/replay" className="text-xs underline underline-offset-4">
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

function TelemetryExportsTab() {
  const { toast } = useToast();
  const { data: bundles, refetch, isLoading } = useQuery({
    queryKey: ['telemetry-bundles'],
    queryFn: () => apiClient.listTelemetryBundles(),
  });

  const generate = useMutation({
    mutationFn: () => apiClient.generateTelemetryBundle(),
    onSuccess: () => {
      toast({ title: 'Export bundle requested' });
      refetch();
    },
  });

  const exportBundle = useMutation({
    mutationFn: (bundleId: string) => apiClient.exportTelemetryBundle(bundleId),
    onSuccess: (res) => {
      toast({ title: 'Export ready', description: `Bundle size ${res.size_bytes} bytes` });
    },
  });

  if (isLoading) {
    return <div className="text-sm text-muted-foreground">Loading exports…</div>;
  }

  return (
    <Card>
      <CardHeader className="flex items-center justify-between">
        <CardTitle>Telemetry exports</CardTitle>
        <Button onClick={() => generate.mutate()} disabled={generate.isPending}>
          {generate.isPending ? 'Generating…' : 'Generate bundle'}
        </Button>
      </CardHeader>
      <CardContent>
        {!bundles?.length ? (
          <div className="text-sm text-muted-foreground">No telemetry bundles found.</div>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>ID</TableHead>
                <TableHead>CPID</TableHead>
                <TableHead>Events</TableHead>
                <TableHead>Size</TableHead>
                <TableHead className="text-right">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {bundles.map((bundle: TelemetryBundle) => (
                <TableRow key={bundle.id}>
                  <TableCell className="font-mono text-xs">{bundle.id}</TableCell>
                  <TableCell>{bundle.cpid}</TableCell>
                  <TableCell>{bundle.event_count}</TableCell>
                  <TableCell>{bundle.size_bytes} bytes</TableCell>
                  <TableCell className="text-right">
                    <Button size="sm" variant="outline" onClick={() => exportBundle.mutate(bundle.id)}>
                      Download
                    </Button>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </CardContent>
    </Card>
  );
}

function TelemetryFiltersTab() {
  const [tenantId, setTenantId] = useState('');
  const [eventType, setEventType] = useState('');
  const [level, setLevel] = useState('');
  const [limit, setLimit] = useState(50);

  const { data, refetch, isFetching } = useQuery({
    queryKey: ['telemetry-filtered', tenantId, eventType, level, limit],
    queryFn: () =>
      apiClient.getTelemetryEvents({
        tenantId: tenantId || undefined,
        eventType: eventType || undefined,
        level: level || undefined,
        limit,
      }),
  });

  return (
    <Card>
      <CardHeader>
        <CardTitle>Saved filters</CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="grid grid-cols-1 md:grid-cols-4 gap-3">
          <div className="space-y-1">
            <Label htmlFor="tenant">Tenant</Label>
            <Input id="tenant" value={tenantId} onChange={(e) => setTenantId(e.target.value)} placeholder="tenant id" />
          </div>
          <div className="space-y-1">
            <Label htmlFor="eventType">Event type</Label>
            <Input id="eventType" value={eventType} onChange={(e) => setEventType(e.target.value)} placeholder="route, inference…" />
          </div>
          <div className="space-y-1">
            <Label htmlFor="level">Level</Label>
            <Input id="level" value={level} onChange={(e) => setLevel(e.target.value)} placeholder="info|warn|error" />
          </div>
          <div className="space-y-1">
            <Label htmlFor="limit">Limit</Label>
            <Input
              id="limit"
              type="number"
              min={1}
              max={500}
              value={limit}
              onChange={(e) => setLimit(Number(e.target.value))}
            />
          </div>
        </div>
        <div className="flex justify-end">
          <Button onClick={() => refetch()} disabled={isFetching}>
            {isFetching ? 'Loading…' : 'Run'}
          </Button>
        </div>
        <div className="space-y-2">
          {data && data.length > 0 ? (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Event</TableHead>
                  <TableHead>Type</TableHead>
                  <TableHead>Level</TableHead>
                  <TableHead>Tenant</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {data.map((event: TelemetryEvent) => (
                  <TableRow key={event.id}>
                    <TableCell className="text-sm">{event.message ?? event.event_type}</TableCell>
                    <TableCell>{event.event_type}</TableCell>
                    <TableCell>
                      <Badge variant="outline">{event.level ?? 'info'}</Badge>
                    </TableCell>
                    <TableCell className="font-mono text-xs text-muted-foreground">{event.tenant_id ?? '—'}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          ) : (
            <div className="text-sm text-muted-foreground">No events for this filter yet.</div>
          )}
        </div>
      </CardContent>
    </Card>
  );
}

