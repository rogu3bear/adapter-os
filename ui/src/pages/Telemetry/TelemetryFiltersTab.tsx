import React, { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { apiClient } from '@/api/services';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import type { TelemetryEvent } from '@/api/types';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';

export default function TelemetryFiltersTab() {
  const [tenantId, setTenantId] = useState('');
  const [eventType, setEventType] = useState('');
  const [level, setLevel] = useState('');
  const [limit, setLimit] = useState(50);

  const { data, refetch, isFetching } = useQuery({
    queryKey: ['telemetry-filtered', tenantId, eventType, level, limit],
    queryFn: () =>
      apiClient.getTelemetryEvents({
        limit,
        eventTypes: eventType ? [eventType] : undefined,
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
