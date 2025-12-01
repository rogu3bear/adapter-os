// Behavior Events Browser Tab
//
// Displays lifecycle behavior events for adapter training data generation.
// Pattern follows AuditLogsTab.tsx

import React, { useState } from 'react';
import { format } from 'date-fns';
import { Download, Filter, RefreshCw } from 'lucide-react';
import { useBehaviorEvents } from '@/hooks/useBehaviorTraining';
import { BehaviorExportWizard } from '@/components/training/BehaviorExportWizard';
import { BehaviorStatsCard } from '@/components/training/BehaviorStatsCard';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import type { BehaviorEvent, BehaviorEventFilters } from '@/api/adapter-types';

const EVENT_TYPE_COLORS: Record<string, string> = {
  promoted: 'bg-success/10 text-success',
  demoted: 'bg-warning/10 text-warning',
  evicted: 'bg-destructive/10 text-destructive',
  pinned: 'bg-info/10 text-info',
  recovered: 'bg-success/10 text-success',
  ttl_expired: 'bg-muted text-muted-foreground',
};

interface BehaviorEventsTabProps {
  selectedTenant: string;
}

export function BehaviorEventsTab({ selectedTenant }: BehaviorEventsTabProps) {
  const [filters, setFilters] = useState<BehaviorEventFilters>({
    tenant_id: selectedTenant,
    limit: 100,
    offset: 0,
  });
  const [showExportWizard, setShowExportWizard] = useState(false);
  const [expandedRows, setExpandedRows] = useState<Set<string>>(new Set());

  const { data: events, isLoading, refetch } = useBehaviorEvents(filters);

  const handleFilterChange = (key: keyof BehaviorEventFilters, value: string | number | undefined) => {
    setFilters((prev) => ({ ...prev, [key]: value || undefined }));
  };

  const toggleRowExpansion = (eventId: string) => {
    setExpandedRows((prev) => {
      const next = new Set(prev);
      if (next.has(eventId)) {
        next.delete(eventId);
      } else {
        next.add(eventId);
      }
      return next;
    });
  };

  return (
    <div className="space-y-4">
      {/* Statistics Card */}
      <BehaviorStatsCard tenantId={selectedTenant} />

      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <div>
              <CardTitle>Behavior Events</CardTitle>
              <CardDescription>
                Adapter lifecycle events captured for behavior training data generation
              </CardDescription>
            </div>
            <div className="flex gap-2">
              <Button onClick={() => refetch()} variant="outline" size="sm">
                <RefreshCw className="h-4 w-4 mr-2" />
                Refresh
              </Button>
              <Button onClick={() => setShowExportWizard(true)} size="sm">
                <Download className="h-4 w-4 mr-2" />
                Export
              </Button>
            </div>
          </div>
        </CardHeader>
        <CardContent>
          {/* Filters */}
          <div className="grid grid-cols-1 md:grid-cols-4 gap-4 mb-6">
            <div className="space-y-2">
              <label className="text-sm font-medium">Event Type</label>
              <Select
                value={filters.event_type || 'all'}
                onValueChange={(value) =>
                  handleFilterChange('event_type', value === 'all' ? undefined : value)
                }
              >
                <SelectTrigger>
                  <SelectValue placeholder="All types" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="all">All types</SelectItem>
                  <SelectItem value="promoted">Promoted</SelectItem>
                  <SelectItem value="demoted">Demoted</SelectItem>
                  <SelectItem value="evicted">Evicted</SelectItem>
                  <SelectItem value="pinned">Pinned</SelectItem>
                  <SelectItem value="recovered">Recovered</SelectItem>
                  <SelectItem value="ttl_expired">TTL Expired</SelectItem>
                </SelectContent>
              </Select>
            </div>

            <div className="space-y-2">
              <label className="text-sm font-medium">Adapter ID</label>
              <Input
                placeholder="Filter by adapter..."
                value={filters.adapter_id || ''}
                onChange={(e) => handleFilterChange('adapter_id', e.target.value)}
              />
            </div>

            <div className="space-y-2">
              <label className="text-sm font-medium">Since</label>
              <Input
                type="date"
                value={filters.since || ''}
                onChange={(e) => handleFilterChange('since', e.target.value)}
              />
            </div>

            <div className="space-y-2">
              <label className="text-sm font-medium">Until</label>
              <Input
                type="date"
                value={filters.until || ''}
                onChange={(e) => handleFilterChange('until', e.target.value)}
              />
            </div>
          </div>

          {/* Table */}
          {isLoading ? (
            <div className="text-center py-8 text-muted-foreground">Loading events...</div>
          ) : !events || events.length === 0 ? (
            <div className="text-center py-8 text-muted-foreground">
              No behavior events found. Events are captured during adapter lifecycle transitions.
            </div>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Timestamp</TableHead>
                  <TableHead>Event Type</TableHead>
                  <TableHead>Adapter ID</TableHead>
                  <TableHead>Transition</TableHead>
                  <TableHead>Activation %</TableHead>
                  <TableHead>Memory (MB)</TableHead>
                  <TableHead>Reason</TableHead>
                  <TableHead></TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {events.map((event) => (
                  <React.Fragment key={event.id}>
                    <TableRow
                      className="cursor-pointer hover:bg-muted/50"
                      onClick={() => toggleRowExpansion(event.id)}
                    >
                      <TableCell>
                        {format(new Date(event.created_at), 'MMM dd, HH:mm:ss')}
                      </TableCell>
                      <TableCell>
                        <Badge className={EVENT_TYPE_COLORS[event.event_type] || ''}>
                          {event.event_type}
                        </Badge>
                      </TableCell>
                      <TableCell className="font-mono text-sm">{event.adapter_id}</TableCell>
                      <TableCell>
                        <span className="text-sm">
                          {event.from_state} → {event.to_state}
                        </span>
                      </TableCell>
                      <TableCell>{event.activation_pct.toFixed(2)}%</TableCell>
                      <TableCell>{event.memory_mb}</TableCell>
                      <TableCell className="text-sm text-muted-foreground">
                        {event.reason}
                      </TableCell>
                      <TableCell>
                        <Button variant="ghost" size="sm">
                          {expandedRows.has(event.id) ? '▼' : '▶'}
                        </Button>
                      </TableCell>
                    </TableRow>
                    {expandedRows.has(event.id) && (
                      <TableRow>
                        <TableCell colSpan={8} className="bg-muted/30">
                          <div className="p-4 space-y-2">
                            <div>
                              <span className="font-semibold">Event ID:</span> {event.id}
                            </div>
                            <div>
                              <span className="font-semibold">Tenant ID:</span> {event.tenant_id}
                            </div>
                            {event.metadata && (
                              <div>
                                <span className="font-semibold">Metadata:</span>
                                <pre className="mt-2 p-2 bg-background rounded text-xs overflow-auto">
                                  {JSON.stringify(JSON.parse(event.metadata), null, 2)}
                                </pre>
                              </div>
                            )}
                          </div>
                        </TableCell>
                      </TableRow>
                    )}
                  </React.Fragment>
                ))}
              </TableBody>
            </Table>
          )}

          {/* Pagination info */}
          {events && events.length > 0 && (
            <div className="mt-4 text-sm text-muted-foreground text-center">
              Showing {events.length} events (limit: {filters.limit})
            </div>
          )}
        </CardContent>
      </Card>

      {showExportWizard && (
        <BehaviorExportWizard
          isOpen={showExportWizard}
          onClose={() => setShowExportWizard(false)}
          defaultTenantId={selectedTenant}
        />
      )}
    </div>
  );
}

