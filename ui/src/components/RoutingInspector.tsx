import React, { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './ui/table';
import { Badge } from './ui/badge';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { useTimestamp } from '../hooks/useTimestamp';
<<<<<<< HEAD
import { RoutingDecision } from '../api/types';
import apiClient from '../api/client';
import { useTenant } from '../providers/FeatureProviders';
=======
import apiClient from '../api/client';

interface RoutingDecision {
  id: string;
  timestamp: string;
  input_hash: string;
  adapters: string[];
  gates: number[];
  total_score: number;
  k_value: number;
  entropy: number;
}
>>>>>>> integration-branch

interface RoutingInspectorProps {
  className?: string;
}

export const RoutingInspector: React.FC<RoutingInspectorProps> = ({ className }) => {
  const [limit, setLimit] = useState(50);
  const [filter, setFilter] = useState('all');
  const [searchHash, setSearchHash] = useState('');
  const { selectedTenant } = useTenant();

  const { data: decisions, isLoading, error } = useQuery<RoutingDecision[]>({
    queryKey: ['/v1/routing/decisions', limit, filter, selectedTenant],
    queryFn: async () => {
<<<<<<< HEAD
      return apiClient.getRoutingDecisions({
        limit,
        tenant: selectedTenant || 'default',
=======
      // Citation: ui/src/api/client.ts L809-L817
      return apiClient.getRoutingDecisions({
        limit,
>>>>>>> integration-branch
        // Note: filter and searchHash parameters would need to be added to the API client method
      });
    },
    refetchInterval: 5000, // Refresh every 5 seconds
    retry: 1, // Only retry once on failure
    retryDelay: 1000,
  });


  const formatGates = (gates: number[] = []) => {
    return gates.map(g => g.toFixed(3)).join(', ');
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

  if (isLoading) {
    return (
      <Card className={className}>
        <CardHeader>
          <CardTitle>Routing Decisions</CardTitle>
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
          <CardTitle>Routing Decisions</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex items-center justify-center h-32">
            <div className="text-red-500">Error loading routing decisions</div>
          </div>
        </CardContent>
      </Card>
    );
  }

  return (
    <Card className={className}>
      <CardHeader>
        <CardTitle>Routing Decisions</CardTitle>
        <div className="flex flex-col sm:flex-row gap-4 mt-4">
          <div className="flex-1">
            <Input
              placeholder="Search by input hash..."
              value={searchHash}
              onChange={(e) => setSearchHash(e.target.value)}
            />
          </div>
          <Select value={filter} onValueChange={setFilter}>
            <SelectTrigger className="w-32">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">All</SelectItem>
              <SelectItem value="k0">K=0</SelectItem>
              <SelectItem value="low_entropy">Low Entropy</SelectItem>
              <SelectItem value="high_score">High Score</SelectItem>
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
                <TableHead>Timestamp</TableHead>
                <TableHead>Input Hash</TableHead>
                <TableHead>K</TableHead>
                <TableHead>Adapters</TableHead>
                <TableHead>Gates</TableHead>
                <TableHead>Score</TableHead>
                <TableHead>Entropy</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
<<<<<<< HEAD
              {decisions?.map((decision) => {
                const kValue = decision.k_value ?? decision.adapters.length ?? 0;
                const totalScore = decision.total_score ?? 0;
                const entropy = decision.entropy ?? 0;
                const gates = decision.gates ?? [];
                const inputHash = (decision.input_hash ?? decision.prompt_hash ?? '').slice(0, 16);

                return (
                  <TableRow key={decision.id}>
                    <TableCell className="font-mono text-sm">
                      {useTimestamp(decision.timestamp)}
                    </TableCell>
                    <TableCell className="font-mono text-sm">
                      {inputHash ? `${inputHash}...` : '—'}
                    </TableCell>
                    <TableCell>
                      <Badge className={getKValueColor(kValue)}>
                        K={kValue}
                      </Badge>
                    </TableCell>
                    <TableCell>
                      <div className="flex flex-wrap gap-1">
                        {decision.adapters.map((adapter, index) => (
                          <Badge key={index} variant="outline" className="text-xs">
                            {adapter}
                          </Badge>
                        ))}
                      </div>
                    </TableCell>
                    <TableCell className="font-mono text-sm">
                      {formatGates(gates)}
                    </TableCell>
                    <TableCell className="font-mono text-sm">
                      {totalScore.toFixed(3)}
                    </TableCell>
                    <TableCell>
                      <Badge className={getEntropyColor(entropy)}>
                        {entropy.toFixed(3)}
                      </Badge>
                    </TableCell>
                  </TableRow>
                );
              })}
=======
              {decisions?.map((decision) => (
                <TableRow key={decision.id}>
                  <TableCell className="font-mono text-sm">
                    {useTimestamp(decision.timestamp)}
                  </TableCell>
                  <TableCell className="font-mono text-sm">
                    {decision.input_hash.slice(0, 16)}...
                  </TableCell>
                  <TableCell>
                    <Badge className={getKValueColor(decision.k_value)}>
                      K={decision.k_value}
                    </Badge>
                  </TableCell>
                  <TableCell>
                    <div className="flex flex-wrap gap-1">
                      {decision.adapters.map((adapter, index) => (
                        <Badge key={index} variant="outline" className="text-xs">
                          {adapter}
                        </Badge>
                      ))}
                    </div>
                  </TableCell>
                  <TableCell className="font-mono text-sm">
                    {formatGates(decision.gates)}
                  </TableCell>
                  <TableCell className="font-mono text-sm">
                    {decision.total_score.toFixed(3)}
                  </TableCell>
                  <TableCell>
                    <Badge className={getEntropyColor(decision.entropy)}>
                      {decision.entropy.toFixed(3)}
                    </Badge>
                  </TableCell>
                </TableRow>
              ))}
>>>>>>> integration-branch
            </TableBody>
          </Table>
        </div>
        
        {decisions?.length === 0 && (
          <div className="text-center py-8 text-muted-foreground">
            No routing decisions found
          </div>
        )}
      </CardContent>
    </Card>
  );
};
