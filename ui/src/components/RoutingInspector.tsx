import React, { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './ui/table';
import { Badge } from './ui/badge';
import { Input } from './ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { ErrorRecovery, errorRecoveryTemplates } from './ui/error-recovery';
import { HelpTooltip } from './ui/help-tooltip';
import { TransformedRoutingDecision, RouterCandidateInfo } from '../api/types';
import apiClient from '../api/client';
import { useTenant } from '../providers/FeatureProviders';
import { useRBAC } from '@/hooks/useRBAC';

interface RoutingInspectorProps {
  className?: string;
}

export const RoutingInspector: React.FC<RoutingInspectorProps> = ({ className }) => {
  const [limit, setLimit] = useState(50);
  const [filter, setFilter] = useState('all');
  const [searchHash, setSearchHash] = useState('');
  const [selectedDecision, setSelectedDecision] = useState<TransformedRoutingDecision | null>(null);
  const { selectedTenant } = useTenant();
  const { can, userRole } = useRBAC();

  const { data, isLoading, error, refetch } = useQuery({
    queryKey: ['/v1/routing/decisions', limit, filter, selectedTenant],
    queryFn: async () => {
      return apiClient.getRoutingDecisions({
        limit,
        tenant_id: selectedTenant || 'default',
        anomalies_only: filter === 'anomalies',
      });
    },
    refetchInterval: 5000, // Refresh every 5 seconds
    retry: 1,
    retryDelay: 1000,
  });

  const decisions = data || [];

  // Filter decisions based on search hash
  const filteredDecisions = searchHash
    ? decisions.filter(d =>
        d.stack_hash?.toLowerCase().includes(searchHash.toLowerCase()) ||
        d.request_id?.toLowerCase().includes(searchHash.toLowerCase())
      )
    : decisions;

  const formatTimestamp = (ts: string) => {
    try {
      const date = new Date(ts);
      return date.toLocaleTimeString();
    } catch {
      return ts;
    }
  };

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
          <CardTitle>Routing Inspector</CardTitle>
          <div className="flex flex-col sm:flex-row gap-4 mt-4">
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
        </CardHeader>
        <CardContent>
          <div className="rounded-md border">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Time</TableHead>
                  <TableHead>Step</TableHead>
                  <TableHead>
                    <span className="flex items-center gap-1">
                      K
                      <HelpTooltip content="Number of adapters selected by K-sparse routing. Higher K increases expressiveness but adds compute overhead." />
                    </span>
                  </TableHead>
                  <TableHead>Adapters</TableHead>
                  <TableHead>Gates</TableHead>
                  <TableHead>
                    <span className="flex items-center gap-1">
                      Entropy
                      <HelpTooltip content="Shannon entropy of gate distribution. Higher entropy indicates more uniform adapter selection. Low entropy may indicate collapsed routing." />
                    </span>
                  </TableHead>
                  <TableHead>
                    <span className="flex items-center gap-1">
                      Overhead
                      <HelpTooltip content="Routing overhead as percentage of inference time. Budget limit is 8%. Values above indicate performance issues." />
                    </span>
                  </TableHead>
                  <TableHead>
                    <span className="flex items-center gap-1">
                      Latency
                      <HelpTooltip content="Router decision latency in microseconds. Lower values indicate faster adapter selection." />
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
                        onClick={() => setSelectedDecision(decision)}
                      >
                        <TableCell className="font-mono text-sm">
                          {formatTimestamp(decision.timestamp)}
                        </TableCell>
                        <TableCell className="font-mono text-sm">
                          {decision.step}
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
                  {selectedDecision.entropy.toFixed(4)}
                </div>
              </div>
              <div>
                <div className="text-sm font-medium">Temperature (τ)</div>
                <div className="text-sm text-muted-foreground">
                  {selectedDecision.tau.toFixed(4)}
                </div>
              </div>
              <div>
                <div className="text-sm font-medium">Entropy Floor</div>
                <div className="text-sm text-muted-foreground">
                  {selectedDecision.entropy_floor.toFixed(4)}
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
