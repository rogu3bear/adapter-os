import React, { useState, useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './ui/table';
import { Badge } from './ui/badge';
import { Input } from './ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Slider } from './ui/slider';
import { Button } from './ui/button';
import { ErrorRecovery, errorRecoveryTemplates } from './ui/error-recovery';
import { GlossaryTooltip } from './ui/glossary-tooltip';
import { ExportMenu } from './ui/export-menu';
import { TransformedRoutingDecision, RouterCandidateInfo, RoutingDecisionFilters } from '@/api/types';
import { apiClient } from '@/api/services';
import { useTenant } from '@/providers/FeatureProviders';
import { useRBAC } from '@/hooks/security/useRBAC';
import { Calendar } from './ui/calendar';
import { Popover, PopoverContent, PopoverTrigger } from './ui/popover';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription } from './ui/dialog';
import { CalendarIcon } from 'lucide-react';
import { Link } from 'react-router-dom';
import { format as formatDate } from 'date-fns';
import { formatTimestamp as formatTimestampUtil } from '@/lib/formatters';
import { buildReplayRunsLink } from '@/utils/navLinks';

interface RoutingInspectorProps {
  className?: string;
}

export const RoutingInspector: React.FC<RoutingInspectorProps> = ({ className }) => {
  const [limit, setLimit] = useState(50);
  const [filter, setFilter] = useState('all');
  const [searchHash, setSearchHash] = useState('');
  const [selectedDecision, setSelectedDecision] = useState<TransformedRoutingDecision | null>(null);
  const [stackId, setStackId] = useState<string>('');
  const [adapterId, setAdapterId] = useState<string>('');
  const [minEntropy, setMinEntropy] = useState<number>(0);
  const [sinceDate, setSinceDate] = useState<Date | undefined>();
  const [untilDate, setUntilDate] = useState<Date | undefined>();
  const [selectedRequestId, setSelectedRequestId] = useState<string | null>(null);
  const { selectedTenant } = useTenant();
  const { can, userRole } = useRBAC();

  // Fetch stacks and adapters for dropdowns
  const { data: stacks } = useQuery({
    queryKey: ['adapter-stacks'],
    queryFn: () => apiClient.listAdapterStacks(),
  });

  const { data: adapters } = useQuery({
    queryKey: ['adapters'],
    queryFn: () => apiClient.listAdapters(),
  });

  // Build filters object
  const filters: RoutingDecisionFilters = useMemo(() => {
    const f: RoutingDecisionFilters = {
      limit,
      tenant_id: selectedTenant || 'default',
      anomalies_only: filter === 'anomalies',
    };
    if (stackId) f.stack_id = stackId;
    if (adapterId) f.adapter_id = adapterId;
    if (minEntropy > 0) f.min_entropy = minEntropy;
    if (sinceDate) f.since = formatDate(sinceDate, "yyyy-MM-dd'T'HH:mm:ss");
    if (untilDate) f.until = formatDate(untilDate, "yyyy-MM-dd'T'HH:mm:ss");
    return f;
  }, [limit, selectedTenant, filter, stackId, adapterId, minEntropy, sinceDate, untilDate]);

  const { data, isLoading, error, refetch } = useQuery({
    queryKey: ['/v1/routing/decisions', filters],
    queryFn: async () => {
      return apiClient.getRoutingDecisions(filters);
    },
    refetchInterval: 5000, // Refresh every 5 seconds
    retry: 1,
    retryDelay: 1000,
  });

  // Export functionality
  const handleExport = async (format: 'csv' | 'json') => {
    const decisions = data || [];
    if (format === 'json') {
      const blob = new Blob([JSON.stringify(decisions, null, 2)], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `routing-decisions-${formatDate(new Date(), 'yyyy-MM-dd')}.json`;
      a.click();
      URL.revokeObjectURL(url);
    } else {
      // CSV export
      const headers = ['Time', 'Step', 'K', 'Adapters', 'Gates', 'Entropy', 'Overhead %', 'Latency μs'];
      const rows = decisions.map(d => [
        d.timestamp,
        d.step?.toString() || '',
        d.k_value?.toString() || '0',
        d.candidates?.filter(c => c.selected).map(c => c.adapter_idx).join(',') || '',
        d.candidates?.filter(c => c.selected).map(c => c.gate_float.toFixed(3)).join(',') || '',
        d.entropy?.toFixed(3) || '0',
        d.overhead_pct?.toFixed(1) || '',
        d.router_latency_us?.toString() || '',
      ]);
      const csv = [headers.join(','), ...rows.map(r => r.join(','))].join('\n');
      const blob = new Blob([csv], { type: 'text/csv' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `routing-decisions-${formatDate(new Date(), 'yyyy-MM-dd')}.csv`;
      a.click();
      URL.revokeObjectURL(url);
    }
  };

  const decisions = data || [];

  // Filter decisions based on search hash
  const filteredDecisions = searchHash
    ? decisions.filter(d =>
        d.stack_hash?.toLowerCase().includes(searchHash.toLowerCase()) ||
        d.request_id?.toLowerCase().includes(searchHash.toLowerCase())
      )
    : decisions;

  const formatGates = (candidates: RouterCandidateInfo[] = []) => {
    return candidates
      .filter(c => c.selected)
      .map(c => c.gate_float.toFixed(3))
      .join(', ');
  };

  const getEntropyColor = (entropy: number) => {
    if (entropy > 0.8) return 'bg-green-100 text-green-800';
    if (entropy > 0.5) return 'bg-yellow-100 text-yellow-800';
    return 'bg-red-100 text-red-800';
  };

  const getKValueColor = (k: number) => {
    if (k >= 3) return 'bg-blue-100 text-blue-800';
    if (k >= 2) return 'bg-orange-100 text-orange-800';
    return 'bg-red-100 text-red-800';
  };

  const getOverheadColor = (overhead: number | null | undefined) => {
    if (!overhead) return 'bg-gray-100 text-gray-800';
    if (overhead > 8.0) return 'bg-red-100 text-red-800'; // Budget violation
    if (overhead > 5.0) return 'bg-yellow-100 text-yellow-800';
    return 'bg-green-100 text-green-800';
  };

  if (isLoading) {
    return (
      <Card className={className}>
        <CardHeader>
          <CardTitle>Routing Inspector</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex items-center justify-center h-32">
            <div className="text-muted-foreground">Loading routing decisions...</div>
          </div>
        </CardContent>
      </Card>
    );
  }

  if (error) {
    return (
      <Card className={className}>
        <CardHeader>
          <CardTitle>Routing Inspector</CardTitle>
        </CardHeader>
        <CardContent>
          {errorRecoveryTemplates.networkError(refetch)}
        </CardContent>
      </Card>
    );
  }

  return (
    <div className={className}>
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <CardTitle>Routing Inspector</CardTitle>
            <ExportMenu onExport={handleExport} filename="routing-decisions" />
          </div>
          <div className="flex flex-col gap-4 mt-4">
            <div className="flex flex-col sm:flex-row gap-4">
              <div className="flex-1">
                <Input
                  placeholder="Search by stack hash or request ID..."
                  value={searchHash}
                  onChange={(e) => setSearchHash(e.target.value)}
                />
              </div>
              <Select value={filter} onValueChange={setFilter}>
                <SelectTrigger className="w-40">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="all">All Decisions</SelectItem>
                  <SelectItem value="anomalies">Anomalies Only</SelectItem>
                </SelectContent>
              </Select>
              <Select value={limit.toString()} onValueChange={(value) => setLimit(parseInt(value))}>
                <SelectTrigger className="w-20">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="25">25</SelectItem>
                  <SelectItem value="50">50</SelectItem>
                  <SelectItem value="100">100</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div className="flex flex-col sm:flex-row gap-4">
              <Select value={stackId || "__all__"} onValueChange={(v) => setStackId(v === "__all__" ? "" : v)}>
                <SelectTrigger className="w-48">
                  <SelectValue placeholder="Filter by Stack" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="__all__">All Stacks</SelectItem>
                  {stacks?.map(stack => (
                    <SelectItem key={stack.id} value={stack.id}>{stack.name}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <Select value={adapterId || "__all__"} onValueChange={(v) => setAdapterId(v === "__all__" ? "" : v)}>
                <SelectTrigger className="w-48">
                  <SelectValue placeholder="Filter by Adapter" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="__all__">All Adapters</SelectItem>
                  {adapters?.map(adapter => (
                    <SelectItem key={adapter.adapter_id} value={adapter.adapter_id}>{adapter.name || adapter.adapter_id}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <div className="flex-1">
                <div className="flex items-center gap-2">
                  <span className="text-sm text-muted-foreground min-w-[calc(var(--base-unit)*20)]">Min Entropy:</span>
                  <Slider
                    value={[minEntropy]}
                    onValueChange={([value]) => setMinEntropy(value)}
                    min={0}
                    max={1}
                    step={0.01}
                    className="flex-1"
                  />
                  <span className="text-sm font-mono w-12">{minEntropy.toFixed(2)}</span>
                </div>
              </div>
            </div>
            <div className="flex flex-col sm:flex-row gap-4">
              <Popover>
                <PopoverTrigger asChild>
                  <Button variant="outline" className="w-full sm:w-[calc(var(--base-unit)*50)] justify-start text-left font-normal">
                    <CalendarIcon className="mr-2 h-4 w-4" />
                    {sinceDate ? formatDate(sinceDate, 'PPP') : 'Since date'}
                  </Button>
                </PopoverTrigger>
                <PopoverContent className="w-auto p-0">
                  <Calendar mode="single" selected={sinceDate} onSelect={setSinceDate} />
                </PopoverContent>
              </Popover>
              <Popover>
                <PopoverTrigger asChild>
                  <Button variant="outline" className="w-full sm:w-[calc(var(--base-unit)*50)] justify-start text-left font-normal">
                    <CalendarIcon className="mr-2 h-4 w-4" />
                    {untilDate ? formatDate(untilDate, 'PPP') : 'Until date'}
                  </Button>
                </PopoverTrigger>
                <PopoverContent className="w-auto p-0">
                  <Calendar mode="single" selected={untilDate} onSelect={setUntilDate} />
                </PopoverContent>
              </Popover>
            </div>
          </div>
        </CardHeader>
        <CardContent>
          <div className="rounded-md border">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Time</TableHead>
                  <TableHead>Step</TableHead>
                  <TableHead>Replay</TableHead>
                  <TableHead>
                    <span className="flex items-center gap-1">
                      K
                      <GlossaryTooltip brief="Number of adapters selected by K-sparse routing. Higher K increases expressiveness but adds compute overhead." />
                    </span>
                  </TableHead>
                  <TableHead>Adapters</TableHead>
                  <TableHead>Gates</TableHead>
                  <TableHead>
                    <span className="flex items-center gap-1">
                      Entropy
                      <GlossaryTooltip brief="Shannon entropy of gate distribution. Higher entropy indicates more uniform adapter selection. Low entropy may indicate collapsed routing." />
                    </span>
                  </TableHead>
                  <TableHead>
                    <span className="flex items-center gap-1">
                      Overhead
                      <GlossaryTooltip brief="Routing overhead as percentage of inference time. Budget limit is 8%. Values above indicate performance issues." />
                    </span>
                  </TableHead>
                  <TableHead>
                    <span className="flex items-center gap-1">
                      Latency
                      <GlossaryTooltip brief="Router decision latency in microseconds. Lower values indicate faster adapter selection." />
                    </span>
                  </TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {filteredDecisions.length === 0 ? (
                  <TableRow>
                    <TableCell colSpan={8} className="text-center text-muted-foreground">
                      No routing decisions found
                    </TableCell>
                  </TableRow>
                ) : (
                  filteredDecisions.map((decision) => {
                    const kValue = decision.k_value ?? 0;
                    const entropy = decision.entropy ?? 0;
                    const overhead = decision.overhead_pct;
                    const latency = decision.router_latency_us;

                    return (
                      <TableRow
                        key={decision.id}
                        className="cursor-pointer hover:bg-muted/50"
                        onClick={() => {
                          setSelectedDecision(decision);
                          if (decision.request_id) {
                            setSelectedRequestId(decision.request_id);
                          }
                        }}
                      >
                        <TableCell className="font-mono text-sm">
                          {formatTimestampUtil(decision.timestamp, 'short')}
                        </TableCell>
                        <TableCell className="font-mono text-sm">
                          {decision.step}
                        </TableCell>
                        <TableCell>
                          {decision.request_id ? (
                            <Link
                              to={buildReplayRunsLink(decision.request_id)}
                              className="text-xs underline underline-offset-4"
                              onClick={(e) => e.stopPropagation()}
                            >
                              Open replay
                            </Link>
                          ) : (
                            <span className="text-xs text-muted-foreground">—</span>
                          )}
                        </TableCell>
                        <TableCell>
                          <Badge className={getKValueColor(kValue)}>
                            K={kValue}
                          </Badge>
                        </TableCell>
                        <TableCell>
                          <div className="flex flex-wrap gap-1">
                            {decision.candidates
                              .filter(c => c.selected)
                              .map((candidate, index) => (
                                <Badge key={index} variant="outline" className="text-xs">
                                  #{candidate.adapter_idx}
                                </Badge>
                              ))}
                          </div>
                        </TableCell>
                        <TableCell className="font-mono text-sm">
                          {formatGates(decision.candidates)}
                        </TableCell>
                        <TableCell>
                          <Badge className={getEntropyColor(entropy)}>
                            {entropy.toFixed(3)}
                          </Badge>
                        </TableCell>
                        <TableCell>
                          {overhead !== null && overhead !== undefined ? (
                            <Badge className={getOverheadColor(overhead)}>
                              {overhead.toFixed(1)}%
                            </Badge>
                          ) : (
                            <span className="text-muted-foreground">—</span>
                          )}
                        </TableCell>
                        <TableCell className="font-mono text-sm">
                          {latency ? `${latency}μs` : '—'}
                        </TableCell>
                      </TableRow>
                    );
                  })
                )}
              </TableBody>
            </Table>
          </div>

          {filteredDecisions.length > 0 && (
            <div className="mt-4 text-sm text-muted-foreground">
              Showing {filteredDecisions.length} of {decisions.length} routing decisions
            </div>
          )}
        </CardContent>
      </Card>

      {/* Session Router View Modal */}
      {selectedRequestId && (
        <SessionDetailModal
          requestId={selectedRequestId}
          onClose={() => setSelectedRequestId(null)}
        />
      )}

      {/* Decision Detail Modal */}
      {selectedDecision && (
        <Card className="mt-4">
          <CardHeader>
            <CardTitle>Decision Details</CardTitle>
            <button
              onClick={() => setSelectedDecision(null)}
              className="text-sm text-muted-foreground hover:text-foreground"
            >
              Close
            </button>
          </CardHeader>
          <CardContent>
            <div className="grid grid-cols-2 gap-4">
              <div>
                <div className="text-sm font-medium">Request ID</div>
                <div className="text-sm text-muted-foreground font-mono">
                  {selectedDecision.request_id || 'N/A'}
                </div>
              </div>
              <div>
                <div className="text-sm font-medium">Step</div>
                <div className="text-sm text-muted-foreground">{selectedDecision.step}</div>
              </div>
              <div>
                <div className="text-sm font-medium">Stack Hash</div>
                <div className="text-sm text-muted-foreground font-mono">
                  {selectedDecision.stack_hash || 'N/A'}
                </div>
              </div>
              <div>
                <div className="text-sm font-medium">Input Token ID</div>
                <div className="text-sm text-muted-foreground">
                  {selectedDecision.input_token_id ?? 'N/A'}
                </div>
              </div>
              <div>
                <div className="text-sm font-medium">Entropy</div>
                <div className="text-sm text-muted-foreground">
                  {selectedDecision.entropy?.toFixed(4) ?? 'N/A'}
                </div>
              </div>
              <div>
                <div className="text-sm font-medium">Temperature (τ)</div>
                <div className="text-sm text-muted-foreground">
                  {selectedDecision.tau?.toFixed(4) ?? 'N/A'}
                </div>
              </div>
              <div>
                <div className="text-sm font-medium">Entropy Floor</div>
                <div className="text-sm text-muted-foreground">
                  {selectedDecision.entropy_floor?.toFixed(4) ?? 'N/A'}
                </div>
              </div>
              <div>
                <div className="text-sm font-medium">Overhead</div>
                <div className="text-sm text-muted-foreground">
                  {selectedDecision.overhead_pct?.toFixed(2)}%
                  {selectedDecision.overhead_pct && selectedDecision.overhead_pct > 8.0 && (
                    <span className="ml-2 text-red-600">⚠ Budget Exceeded</span>
                  )}
                </div>
              </div>
            </div>

            <div className="mt-6">
              <div className="text-sm font-medium mb-2">Candidates</div>
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Adapter #</TableHead>
                    <TableHead>Raw Score</TableHead>
                    <TableHead>Gate (Q15)</TableHead>
                    <TableHead>Gate (Float)</TableHead>
                    <TableHead>Selected</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {selectedDecision.candidates.map((candidate, index) => (
                    <TableRow key={index}>
                      <TableCell className="font-mono">{candidate.adapter_idx}</TableCell>
                      <TableCell className="font-mono">{candidate.raw_score.toFixed(4)}</TableCell>
                      <TableCell className="font-mono">{candidate.gate_q15}</TableCell>
                      <TableCell className="font-mono">{candidate.gate_float.toFixed(4)}</TableCell>
                      <TableCell>
                        {candidate.selected ? (
                          <Badge className="bg-green-100 text-green-800">✓</Badge>
                        ) : (
                          <span className="text-muted-foreground">—</span>
                        )}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
};

// Session Detail Modal Component
interface SessionDetailModalProps {
  requestId: string;
  onClose: () => void;
}

const SessionDetailModal: React.FC<SessionDetailModalProps> = ({ requestId, onClose }) => {
  const { data, isLoading, error } = useQuery({
    queryKey: ['session-router-view', requestId],
    queryFn: () => apiClient.getSessionRouterView(requestId),
    enabled: !!requestId,
  });

  const getEntropyColor = (entropy: number) => {
    if (entropy > 0.8) return 'bg-green-100 text-green-800';
    if (entropy > 0.5) return 'bg-yellow-100 text-yellow-800';
    return 'bg-red-100 text-red-800';
  };

  return (
    <Dialog open={!!requestId} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="max-w-4xl max-h-[80vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>Session Router View: {requestId}</DialogTitle>
          <DialogDescription>
            {data && (
              <>
                Stack: {data.stack_id || 'N/A'} | Total Steps: {data.total_steps}
              </>
            )}
          </DialogDescription>
        </DialogHeader>
        {isLoading && (
          <div className="flex items-center justify-center h-32">
            <div className="text-muted-foreground">Loading session details...</div>
          </div>
        )}
        {error && (
          <div className="text-red-600">Failed to load session details</div>
        )}
        {data && (
          <div className="space-y-4">
            <div className="grid grid-cols-2 gap-4">
              <div>
                <div className="text-sm font-medium text-muted-foreground">Stack ID</div>
                <div className="text-sm font-mono">{data.stack_id || 'N/A'}</div>
              </div>
              <div>
                <div className="text-sm font-medium text-muted-foreground">Stack Hash</div>
                <div className="text-sm font-mono">{data.stack_hash || 'N/A'}</div>
              </div>
            </div>
            <div className="border rounded-md">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Step</TableHead>
                    <TableHead>Time</TableHead>
                    <TableHead>Input Token</TableHead>
                    <TableHead>Adapters Fired</TableHead>
                    <TableHead>Gate Values</TableHead>
                    <TableHead>Entropy</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {data.steps.map((step) => (
                    <TableRow key={step.step}>
                      <TableCell className="font-mono">{step.step}</TableCell>
                      <TableCell className="text-sm">
                        {formatDate(new Date(step.timestamp), 'HH:mm:ss.SSS')}
                      </TableCell>
                      <TableCell className="font-mono">
                        {step.input_token_id ?? '—'}
                      </TableCell>
                      <TableCell>
                        <div className="flex flex-wrap gap-1">
                          {step.adapters_fired
                            .filter(a => a.selected)
                            .map((adapter, idx) => (
                              <Badge
                                key={idx}
                                variant={adapter.selected ? 'default' : 'outline'}
                                className="text-xs"
                              >
                                #{adapter.adapter_idx}
                              </Badge>
                            ))}
                        </div>
                      </TableCell>
                      <TableCell className="font-mono text-sm">
                        {step.adapters_fired
                          .filter(a => a.selected)
                          .map(a => a.gate_value.toFixed(3))
                          .join(', ')}
                      </TableCell>
                      <TableCell>
                        <Badge className={getEntropyColor(step.entropy)}>
                          {step.entropy.toFixed(3)}
                        </Badge>
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </div>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
};
