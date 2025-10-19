import React, { useEffect, useMemo, useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './ui/table';
import { Alert, AlertDescription } from './ui/alert';
import apiClient from '../api/client';
import { GoldenRun, GoldenCompareResult } from '../api/types';
import { toast } from 'sonner';

export function GoldenRuns() {
  const [names, setNames] = useState<string[]>([]);
  const [selected, setSelected] = useState<string>('');
  const [summary, setSummary] = useState<GoldenRun | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedRuns, setSelectedRuns] = useState<GoldenRun[]>([]);
  const [compareResult, setCompareResult] = useState<GoldenCompareResult | null>(null);

  useEffect(() => {
    (async () => {
      try {
        const runs = await apiClient.listGoldenRuns();
        setNames(runs);
        if (runs.length) setSelected(runs[0]);
      } catch (err) {
        const msg = err instanceof Error ? err.message : 'Failed to load golden baselines';
        setError(msg);
        toast.error(msg);
      } finally {
        setLoading(false);
      }
    })();
  }, []);

  useEffect(() => {
    if (!selected) {
      setSummary(null);
      return;
    }
    (async () => {
      try {
        const s = await apiClient.getGoldenRun(selected);
        setSummary(s);
      } catch (err) {
        const msg = err instanceof Error ? err.message : 'Failed to load baseline summary';
        setError(msg);
        toast.error(msg);
        setSummary(null);
      }
    })();
  }, [selected]);

  const handleCompare = async () => {
    if (selectedRuns.length !== 2) return;
    try {
      const result = await apiClient.compareGoldenRuns(selectedRuns[0].id, selectedRuns[1].id);
      setCompareResult(result);
    } catch (error) {
      console.error('Compare failed', error);
    }
  };

  const adapterList = useMemo(() => summary?.adapters || [], [summary]);

  if (loading) return <div className="text-center p-8">Loading golden baselines...</div>;

  return (
    <div className="space-y-6">
      <div className="flex-between section-header">
        <div>
          <h1 className="section-title">Golden Baselines</h1>
          <p className="section-description">Browse baselines and epsilon summaries</p>
        </div>
      </div>

      {error && (
        <Alert variant="destructive">
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}

      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <Card className="card-standard md:col-span-1">
          <CardHeader>
            <CardTitle>Baselines</CardTitle>
          </CardHeader>
          <CardContent>
            {names.length === 0 ? (
              <div className="text-sm text-muted-foreground">No baselines available.</div>
            ) : (
              <div className="space-y-1">
                <select
                  className="w-full p-2 border rounded"
                  value={selected}
                  onChange={(e) => setSelected(e.target.value)}
                >
                  {names.map((n) => (
                    <option key={n} value={n}>{n}</option>
                  ))}
                </select>
              </div>
            )}
          </CardContent>
        </Card>

        <Card className="card-standard md:col-span-2">
          <CardHeader>
            <CardTitle>Summary</CardTitle>
          </CardHeader>
          <CardContent>
            {!summary ? (
              <div className="text-sm text-muted-foreground">Select a baseline to view details.</div>
            ) : (
              <div className="space-y-4">
                <div className="grid grid-cols-2 gap-3 text-sm">
                  <div>
                    <div className="text-muted-foreground">Name</div>
                    <div className="font-mono">{summary.name}</div>
                  </div>
                  <div>
                    <div className="text-muted-foreground">Run ID</div>
                    <div className="font-mono">{summary.run_id}</div>
                  </div>
                  <div>
                    <div className="text-muted-foreground">CPID</div>
                    <div className="font-mono">{summary.cpid}</div>
                  </div>
                  <div>
                    <div className="text-muted-foreground">Plan ID</div>
                    <div className="font-mono">{summary.plan_id}</div>
                  </div>
                  <div>
                    <div className="text-muted-foreground">Layers</div>
                    <div>{summary.layer_count.toLocaleString()}</div>
                  </div>
                  <div>
                    <div className="text-muted-foreground">Max ε</div>
                    <div>{summary.max_epsilon.toExponential(2)}</div>
                  </div>
                  <div>
                    <div className="text-muted-foreground">Mean ε</div>
                    <div>{summary.mean_epsilon.toExponential(2)}</div>
                  </div>
                  <div>
                    <div className="text-muted-foreground">Created</div>
                    <div>{new Date(summary.created_at).toLocaleString()}</div>
                  </div>
                </div>

                <div className="flex flex-wrap gap-2">
                  <Badge variant={summary.has_signature ? 'default' : 'secondary'}>
                    Signature {summary.has_signature ? 'Present' : 'Missing'}
                  </Badge>
                </div>

                <div>
                  <div className="text-muted-foreground mb-1">Toolchain</div>
                  <div className="text-sm">{summary.toolchain_summary}</div>
                </div>

                <div>
                  <div className="text-muted-foreground mb-1">Adapters</div>
                  <div className="flex flex-wrap gap-2">
                    {adapterList.length === 0 ? (
                      <span className="text-sm text-muted-foreground">None</span>
                    ) : (
                      adapterList.map(a => (
                        <Badge key={a} variant="secondary">{a}</Badge>
                      ))
                    )}
                  </div>
                </div>
              </div>
            )}
          </CardContent>
        </Card>
      </div>
      <Button onClick={handleCompare} disabled={selectedRuns.length !== 2}>
        Compare Selected
      </Button>
      {compareResult && (
        <Table>
          <TableHead>
            <TableRow>
              <TableCell>Metric</TableCell>
              <TableCell>Run 1</TableCell>
              <TableCell>Run 2</TableCell>
              <TableCell>Diff</TableCell>
            </TableRow>
          </TableHead>
          <TableBody>
            {compareResult.metrics.map((metric) => (
              <TableRow key={metric.key}>
                <TableCell>{metric.key}</TableCell>
                <TableCell>{metric.value1}</TableCell>
                <TableCell>{metric.value2}</TableCell>
                <TableCell>{metric.diff}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      )}
    </div>
  );
}

export default GoldenRuns;

