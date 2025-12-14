import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './ui/table';
import { Alert, AlertDescription } from './ui/alert';
import apiClient from '@/api/client';
import { GoldenRunSummary, GoldenCompareResult } from '@/api/types';
import { logger, toError } from '@/utils/logger';
import { errorRecoveryTemplates } from './ui/error-recovery';
import { GlossaryTooltip } from './ui/glossary-tooltip';
import { useRBAC } from '@/hooks/security/useRBAC';

export function GoldenRuns() {
  const { can } = useRBAC();
  const [names, setNames] = useState<string[]>([]);
  const [selected, setSelected] = useState<string>('');
  const [summary, setSummary] = useState<GoldenRunSummary | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedRuns, setSelectedRuns] = useState<string[]>([]);
  const [compareResult, setCompareResult] = useState<GoldenCompareResult | null>(null);
  const [statusMessage, setStatusMessage] = useState<{ message: string; variant: 'success' | 'info' | 'warning' } | null>(null);
  const [errorRecovery, setErrorRecovery] = useState<React.ReactElement | null>(null);

  const canViewGolden = can('golden:view');
  const canCreateGolden = can('golden:create');
  const canCompareGolden = can('golden:compare');

  const loadRuns = useCallback(async () => {
    try {
      const runs = await apiClient.listGoldenRuns();
      setNames(runs);
      if (runs.length) setSelected(runs[0]);
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Failed to load golden baselines';
      setError(msg);
      setStatusMessage({ message: msg, variant: 'warning' });
      setErrorRecovery(
        errorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error(msg),
          () => {
            setLoading(true);
            loadRuns();
          }
        )
      );
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadRuns();
  }, [loadRuns]);

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
        setStatusMessage({ message: msg, variant: 'warning' });
        setErrorRecovery(
          errorRecoveryTemplates.genericError(
            err instanceof Error ? err : new Error(msg),
            () => {
              setLoading(true);
              loadRuns();
            }
          )
        );
        setSummary(null);
      }
    })();
  }, [selected, loadRuns]);

  const toggleRunSelection = (runId: string) => {
    setSelectedRuns((prev) => {
      if (prev.includes(runId)) {
        return prev.filter((id) => id !== runId);
      }
      if (prev.length >= 2) {
        setStatusMessage({ message: 'Select at most two runs to compare.', variant: 'warning' });
        return prev;
      }
      return [...prev, runId];
    });
  };

  const handleCompare = async () => {
    if (selectedRuns.length !== 2) {
      setStatusMessage({ message: 'Select exactly two runs to compare.', variant: 'warning' });
      return;
    }
    setLoading(true);
    try {
      const [runA, runB] = selectedRuns;
      const result = await apiClient.compareGoldenRuns(runA, runB);
      setCompareResult(result);
    } catch (error) {
      logger.error('Golden run comparison failed', {
        component: 'GoldenRuns',
        operation: 'compareGoldenRuns',
        runA: selectedRuns[0],
        runB: selectedRuns[1],
      }, toError(error));
      setStatusMessage({ message: error instanceof Error ? error.message : 'Failed to compare golden runs', variant: 'warning' });
      setErrorRecovery(
        errorRecoveryTemplates.genericError(
          error instanceof Error ? error : new Error(error instanceof Error ? error.message : 'Failed to compare golden runs'),
          () => handleCompare()
        )
      );
    } finally {
      setLoading(false);
    }
  };

  const adapterList = useMemo(() => summary?.adapters || [], [summary]);

  if (loading) return <div className="text-center p-8">Loading golden baselines...</div>;

  return (
    <div className="space-y-6">
      <div className="flex-between flex items-center justify-between mb-6">
        <div>
          <h1 className="text-2xl font-bold flex items-center gap-2">
            Golden Baselines
            <GlossaryTooltip termId="golden-run" />
          </h1>
          <p className="text-sm text-muted-foreground">Browse baselines and epsilon summaries</p>
        </div>
      </div>

      {errorRecovery && (
        <div>
          {errorRecovery}
        </div>
      )}

      {statusMessage && (
        <Alert
          className={
            statusMessage.variant === 'success'
              ? 'border-green-200 bg-green-50'
              : statusMessage.variant === 'warning'
                ? 'border-amber-200 bg-amber-50'
                : 'border-blue-200 bg-blue-50'
          }
        >
          <AlertDescription
            className={
              statusMessage.variant === 'success'
                ? 'text-green-700'
                : statusMessage.variant === 'warning'
                  ? 'text-amber-700'
                  : 'text-blue-700'
            }
          >
            {statusMessage.message}
          </AlertDescription>
        </Alert>
      )}

      {error && (
        <Alert variant="destructive">
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}

      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <Card className="card-standard md:col-span-1">
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              Baselines
              <GlossaryTooltip termId="goldenRun" variant="icon" />
            </CardTitle>
          </CardHeader>
          <CardContent>
            {names.length === 0 ? (
              <div className="text-sm text-muted-foreground">No baselines available.</div>
            ) : (
              <div className="space-y-2">
                <div className="flex items-center gap-1 mb-1">
                  <span className="text-xs font-medium">Select Baseline</span>
                  <GlossaryTooltip termId="golden-baseline" />
                </div>
                <select
                  className="w-full p-2 border rounded"
                  value={selected}
                  onChange={(e) => setSelected(e.target.value)}
                >
                  <option value="">Select baseline</option>
                  {names.map((n) => (
                    <option key={n} value={n}>{n}</option>
                  ))}
                </select>
                <div className="border rounded p-2 space-y-1">
                  <div className="text-xs font-semibold text-muted-foreground uppercase flex items-center gap-1">
                    Compare Runs
                    <GlossaryTooltip termId="golden-comparison" />
                  </div>
                  {names.map((runName) => (
                    <label key={runName} className="flex items-center gap-2 text-sm">
                      <input
                        type="checkbox"
                        checked={selectedRuns.includes(runName)}
                        onChange={() => toggleRunSelection(runName)}
                        disabled={!canCompareGolden}
                      />
                      <span>{runName}</span>
                    </label>
                  ))}
                  <p className="text-[11px] text-muted-foreground">
                    Select up to two runs to generate a comparison report.
                  </p>
                </div>
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
                    <div className="text-muted-foreground">Policy ID</div>
                    <div className="font-mono">{summary.cpid}</div>
                  </div>
                  <div>
                    <div className="text-muted-foreground">Plan ID</div>
                    <div className="font-mono">{summary.plan_id}</div>
                  </div>
                  <div>
                    <div className="text-muted-foreground">Layers</div>
                    <div>{summary.layer_count?.toLocaleString() ?? 'N/A'}</div>
                  </div>
                  <div>
                    <div className="text-muted-foreground">Max ε</div>
                    <div>{summary.max_epsilon?.toExponential(2) ?? 'N/A'}</div>
                  </div>
                  <div>
                    <div className="text-muted-foreground">Mean ε</div>
                    <div>{summary.mean_epsilon?.toExponential(2) ?? 'N/A'}</div>
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
      <Button onClick={handleCompare} disabled={selectedRuns.length !== 2 || !canCompareGolden}>
        Compare Selected
        <GlossaryTooltip termId="golden-comparison" />
      </Button>
      {!canCompareGolden && (
        <Alert className="mt-2">
          <AlertDescription className="text-muted-foreground">
            You need the golden:compare permission to compare golden runs.
          </AlertDescription>
        </Alert>
      )}
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
            {compareResult.metrics?.map((metric) => (
              <TableRow key={metric.key}>
                <TableCell>{metric.key}</TableCell>
                <TableCell>{metric.value1}</TableCell>
                <TableCell>{metric.value2}</TableCell>
                <TableCell>{metric.diff}</TableCell>
              </TableRow>
            ))}
            {(!compareResult.metrics || compareResult.metrics.length === 0) && (
              <TableRow>
                <TableCell colSpan={4} className="text-center text-muted-foreground">
                  No comparison metrics available.
                </TableCell>
              </TableRow>
            )}
          </TableBody>
        </Table>
      )}
    </div>
  );
}

export default GoldenRuns;
