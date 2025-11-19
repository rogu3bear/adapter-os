import React, { useMemo, useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '../ui/card';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '../ui/table';
import { Button } from '../ui/button';
import { Badge } from '../ui/badge';
import { Input } from '../ui/input';
import { LayerDivergence } from '../../api/types';

interface LayerComparisonTableProps {
  divergences: LayerDivergence[];
  tolerance: number;
  adapterFilter?: string | null;
  onLayerClick?: (layer: LayerDivergence) => void;
}

type SortKey = 'layer_id' | 'relative_error' | 'golden_l2' | 'current_l2' | 'status';
type SortDirection = 'asc' | 'desc';

export function LayerComparisonTable({
  divergences,
  tolerance,
  adapterFilter,
  onLayerClick,
}: LayerComparisonTableProps) {
  const [sortKey, setSortKey] = useState<SortKey>('relative_error');
  const [sortDirection, setSortDirection] = useState<SortDirection>('desc');
  const [searchTerm, setSearchTerm] = useState('');
  const [statusFilter, setStatusFilter] = useState<'all' | 'pass' | 'fail'>('all');
  const [limitRows, setLimitRows] = useState(true);

  const handleSort = (key: SortKey) => {
    if (sortKey === key) {
      setSortDirection(sortDirection === 'asc' ? 'desc' : 'asc');
    } else {
      setSortKey(key);
      setSortDirection('desc');
    }
  };

  const filteredAndSorted = useMemo(() => {
    let filtered = divergences;

    // Apply adapter filter
    if (adapterFilter) {
      filtered = filtered.filter(d => d.layer_id.startsWith(adapterFilter + '/'));
    }

    // Apply search filter
    if (searchTerm) {
      const term = searchTerm.toLowerCase();
      filtered = filtered.filter(d => d.layer_id.toLowerCase().includes(term));
    }

    // Apply status filter
    if (statusFilter !== 'all') {
      filtered = filtered.filter(d => {
        const passes = d.relative_error <= tolerance;
        return statusFilter === 'pass' ? passes : !passes;
      });
    }

    // Sort
    const sorted = [...filtered].sort((a, b) => {
      let comparison = 0;

      switch (sortKey) {
        case 'layer_id':
          comparison = a.layer_id.localeCompare(b.layer_id);
          break;
        case 'relative_error':
          comparison = a.relative_error - b.relative_error;
          break;
        case 'golden_l2':
          comparison = a.golden.l2_error - b.golden.l2_error;
          break;
        case 'current_l2':
          comparison = a.current.l2_error - b.current.l2_error;
          break;
        case 'status':
          const aPass = a.relative_error <= tolerance;
          const bPass = b.relative_error <= tolerance;
          comparison = (aPass ? 0 : 1) - (bPass ? 0 : 1);
          break;
      }

      return sortDirection === 'asc' ? comparison : -comparison;
    });

    // Limit rows if enabled
    return limitRows ? sorted.slice(0, 100) : sorted;
  }, [divergences, adapterFilter, searchTerm, statusFilter, sortKey, sortDirection, tolerance, limitRows]);

  const exportCsv = () => {
    const header = [
      'layer_id',
      'relative_error',
      'golden_l2',
      'current_l2',
      'golden_max',
      'current_max',
      'golden_mean',
      'current_mean',
      'status',
    ];
    const lines = [header.join(',')];

    for (const d of filteredAndSorted) {
      const status = d.relative_error <= tolerance ? 'pass' : 'fail';
      lines.push([
        JSON.stringify(d.layer_id),
        d.relative_error,
        d.golden.l2_error,
        d.current.l2_error,
        d.golden.max_error,
        d.current.max_error,
        d.golden.mean_error,
        d.current.mean_error,
        status,
      ].join(','));
    }

    const blob = new Blob([lines.join('\n')], { type: 'text/csv' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'layer-comparison.csv';
    a.click();
    URL.revokeObjectURL(url);
  };

  const SortButton = ({ column, label }: { column: SortKey; label: string }) => (
    <button
      onClick={() => handleSort(column)}
      className="flex items-center gap-1 hover:text-foreground transition-colors"
    >
      {label}
      {sortKey === column && (
        <span className="text-xs">
          {sortDirection === 'asc' ? '↑' : '↓'}
        </span>
      )}
    </button>
  );

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between">
          <CardTitle>Layer Comparison</CardTitle>
          <div className="flex items-center gap-2">
            <Button variant="outline" size="sm" onClick={exportCsv}>
              Export CSV
            </Button>
            {divergences.length > 100 && (
              <Button variant="outline" size="sm" onClick={() => setLimitRows(!limitRows)}>
                {limitRows ? 'Show All' : 'Show Top 100'}
              </Button>
            )}
          </div>
        </div>
      </CardHeader>
      <CardContent className="space-y-4">
        {/* Filters */}
        <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
          <div>
            <label className="text-sm text-muted-foreground mb-1">Search Layer</label>
            <Input
              type="text"
              placeholder="Filter by layer name..."
              value={searchTerm}
              onChange={(e) => setSearchTerm(e.target.value)}
            />
          </div>
          <div>
            <label className="text-sm text-muted-foreground mb-1">Status</label>
            <select
              className="w-full p-2 border rounded"
              value={statusFilter}
              onChange={(e) => setStatusFilter(e.target.value as any)}
            >
              <option value="all">All</option>
              <option value="pass">Pass</option>
              <option value="fail">Fail</option>
            </select>
          </div>
          <div className="flex items-end">
            <Button
              variant="outline"
              onClick={() => {
                setSearchTerm('');
                setStatusFilter('all');
                setSortKey('relative_error');
                setSortDirection('desc');
              }}
            >
              Reset Filters
            </Button>
          </div>
        </div>

        {/* Statistics */}
        <div className="text-sm text-muted-foreground">
          Showing {filteredAndSorted.length.toLocaleString()} of {divergences.length.toLocaleString()} layers
        </div>

        {/* Table */}
        <div className="overflow-auto border rounded-lg">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>
                  <SortButton column="layer_id" label="Layer ID" />
                </TableHead>
                <TableHead>
                  <SortButton column="relative_error" label="Rel Error" />
                </TableHead>
                <TableHead>
                  <SortButton column="golden_l2" label="Golden L2" />
                </TableHead>
                <TableHead>
                  <SortButton column="current_l2" label="Current L2" />
                </TableHead>
                <TableHead>Golden Max</TableHead>
                <TableHead>Current Max</TableHead>
                <TableHead>Golden Mean</TableHead>
                <TableHead>Current Mean</TableHead>
                <TableHead>
                  <SortButton column="status" label="Status" />
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {filteredAndSorted.length === 0 ? (
                <TableRow>
                  <TableCell colSpan={9} className="text-center text-muted-foreground">
                    No layers match the current filters.
                  </TableCell>
                </TableRow>
              ) : (
                filteredAndSorted.map((div) => {
                  const passes = div.relative_error <= tolerance;
                  return (
                    <TableRow
                      key={div.layer_id}
                      className="cursor-pointer hover:bg-muted/50"
                      onClick={() => onLayerClick?.(div)}
                    >
                      <TableCell className="font-mono text-xs max-w-xs truncate" title={div.layer_id}>
                        {div.layer_id}
                      </TableCell>
                      <TableCell className="font-mono">
                        {div.relative_error.toExponential(2)}
                      </TableCell>
                      <TableCell className="font-mono">
                        {div.golden.l2_error.toExponential(2)}
                      </TableCell>
                      <TableCell className="font-mono">
                        {div.current.l2_error.toExponential(2)}
                      </TableCell>
                      <TableCell className="font-mono">
                        {div.golden.max_error.toExponential(2)}
                      </TableCell>
                      <TableCell className="font-mono">
                        {div.current.max_error.toExponential(2)}
                      </TableCell>
                      <TableCell className="font-mono">
                        {div.golden.mean_error.toExponential(2)}
                      </TableCell>
                      <TableCell className="font-mono">
                        {div.current.mean_error.toExponential(2)}
                      </TableCell>
                      <TableCell>
                        <Badge variant={passes ? 'default' : 'destructive'}>
                          {passes ? 'Pass' : 'Fail'}
                        </Badge>
                      </TableCell>
                    </TableRow>
                  );
                })
              )}
            </TableBody>
          </Table>
        </div>
      </CardContent>
    </Card>
  );
}
