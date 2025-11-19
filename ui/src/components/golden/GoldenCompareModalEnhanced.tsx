import React, { useEffect, useMemo, useState } from 'react';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from '../ui/dialog';
import { Button } from '../ui/button';
import { Badge } from '../ui/badge';
import { Alert, AlertDescription } from '../ui/alert';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '../ui/table';
import apiClient from '../../api/client';
import { GoldenCompareRequest, Strictness, VerificationReport, LayerDivergence } from '../../api/types';
import { ErrorRecoveryTemplates } from '../ui/error-recovery';
import { EpsilonHeatmap } from './EpsilonHeatmap';
import { StatisticalSummary } from './StatisticalSummary';
import { LayerDetailModal } from './LayerDetailModal';

interface GoldenCompareModalEnhancedProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  bundleId: string | null;
}

export function GoldenCompareModalEnhanced({ open, onOpenChange, bundleId }: GoldenCompareModalEnhancedProps) {
  const [goldenRuns, setGoldenRuns] = useState<string[]>([]);
  const [selectedGolden, setSelectedGolden] = useState<string>('');
  const [strictness, setStrictness] = useState<Strictness>('epsilon-tolerant');
  const [verifyToolchain, setVerifyToolchain] = useState<boolean>(true);
  const [verifyAdapters, setVerifyAdapters] = useState<boolean>(true);
  const [verifySignature, setVerifySignature] = useState<boolean>(true);
  const [verifyDevice, setVerifyDevice] = useState<boolean>(false);
  const [compareResult, setCompareResult] = useState<VerificationReport | null>(null);
  const [adapterFilter, setAdapterFilter] = useState<string | null>(null);
  const [compareError, setCompareError] = useState<Error | null>(null);
  const [validationWarning, setValidationWarning] = useState<string | null>(null);
  const [verificationStatus, setVerificationStatus] = useState<'passed' | 'failed' | null>(null);
  const [limitRows, setLimitRows] = useState<boolean>(true);
  const [sortKey, setSortKey] = useState<'rel' | 'g_l2' | 'c_l2'>('rel');
  const [sortDir, setSortDir] = useState<'desc' | 'asc'>('desc');
  const [viewMode, setViewMode] = useState<'table' | 'heatmap' | 'stats'>('table');
  const [selectedLayerDetail, setSelectedLayerDetail] = useState<LayerDivergence | null>(null);

  useEffect(() => {
    if (!open) return;
    // Reset state when opened
    setCompareResult(null);
    setAdapterFilter(null);
    setCompareError(null);
    setValidationWarning(null);
    setVerificationStatus(null);
    setLimitRows(true);
    setViewMode('table');
    // Load golden run names
    (async () => {
      try {
        const runs = await apiClient.listGoldenRuns();
        setGoldenRuns(runs);
        if (runs.length) setSelectedGolden((prev) => prev || runs[0]);
      } catch (err) {
        const msg = err instanceof Error ? err.message : 'Failed to load golden baselines';
        const error = err instanceof Error ? err : new Error(msg);
        setCompareError(error);
      }
    })();
  }, [open]);

  const runGoldenCompare = async () => {
    setValidationWarning(null);
    setCompareError(null);
    setVerificationStatus(null);

    if (!bundleId) {
      setValidationWarning('Select a telemetry bundle to begin verification.');
      return;
    }
    if (!selectedGolden) {
      setValidationWarning('Please select a golden baseline before running the comparison.');
      return;
    }
    const req: GoldenCompareRequest = {
      golden: selectedGolden,
      bundle_id: bundleId,
      strictness,
      verify_toolchain: verifyToolchain,
      verify_adapters: verifyAdapters,
      verify_signature: verifySignature,
      verify_device: verifyDevice,
    };
    try {
      const res = await apiClient.goldenCompare(req);
      setCompareResult(res);
      setCompareError(null);
      setVerificationStatus(res.passed ? 'passed' : 'failed');
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Golden comparison failed';
      const friendly = /404|not found/i.test(msg)
        ? 'Baseline or bundle not found (404)'
        : msg;
      setCompareError(new Error(friendly));
      setCompareResult(null);
    }
  };

  const adapterPrefixes = useMemo(() => {
    const set = new Set<string>();
    if (!compareResult) return [] as string[];
    for (const div of compareResult.epsilon_comparison.divergent_layers) {
      if (div.layer_id.startsWith('adapter:')) {
        const parts = div.layer_id.split('/');
        if (parts.length > 0) set.add(parts[0]);
      }
    }
    return Array.from(set);
  }, [compareResult]);

  const filteredDivergences = useMemo(() => {
    if (!compareResult) return [] as VerificationReport['epsilon_comparison']['divergent_layers'];
    const base = compareResult.epsilon_comparison.divergent_layers
      .filter(div => !adapterFilter || div.layer_id.startsWith(adapterFilter + '/'));
    const sorted = [...base].sort((a, b) => {
      const dir = sortDir === 'desc' ? -1 : 1;
      if (sortKey === 'rel') return (a.relative_error - b.relative_error) * dir;
      if (sortKey === 'g_l2') return (a.golden.l2_error - b.golden.l2_error) * dir;
      return (a.current.l2_error - b.current.l2_error) * dir;
    });
    if (!limitRows) return sorted;
    return sorted.slice(0, 100);
  }, [compareResult, adapterFilter, limitRows, sortDir, sortKey]);

  const exportCsv = (all: boolean = false) => {
    if (!compareResult) return;
    const rows = (all ? compareResult.epsilon_comparison.divergent_layers : filteredDivergences);
    const header = [
      'layer_id','relative_error','golden_l2','current_l2','golden_max','current_max','golden_mean','current_mean'
    ];
    const lines = [header.join(',')];
    for (const d of rows) {
      lines.push([
        JSON.stringify(d.layer_id),
        d.relative_error,
        d.golden.l2_error,
        d.current.l2_error,
        d.golden.max_error,
        d.current.max_error,
        d.golden.mean_error,
        d.current.mean_error,
      ].join(','));
    }
    const blob = new Blob([lines.join('\n')], { type: 'text/csv' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `epsilon-divergences${all ? '-all' : ''}.csv`;
    a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <>
      <Dialog open={open} onOpenChange={onOpenChange}>
        <DialogContent className="max-w-6xl max-h-[90vh] overflow-auto">
          <DialogHeader>
            <DialogTitle>Compare to Golden Baseline</DialogTitle>
          </DialogHeader>

          <div className="space-y-4">
            {compareError && ErrorRecoveryTemplates.genericError(
              compareError,
              () => {
                setCompareError(null);
                void runGoldenCompare();
              }
            )}

            {validationWarning && (
              <Alert className="border-amber-200 bg-amber-50">
                <AlertDescription className="text-amber-700">
                  {validationWarning}
                </AlertDescription>
              </Alert>
            )}

            {verificationStatus && (
              <Alert className={
                verificationStatus === 'passed'
                  ? 'border-green-200 bg-green-50'
                  : 'border-red-200 bg-red-50'
              }>
                <AlertDescription className={
                  verificationStatus === 'passed'
                    ? 'text-green-700'
                    : 'text-red-700'
                }>
                  {verificationStatus === 'passed'
                    ? 'Verification passed. No significant divergences detected.'
                    : 'Verification failed. Review divergent layers below.'}
                </AlertDescription>
              </Alert>
            )}

            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <div className="mb-4">
                <label className="font-medium text-sm mb-1">Golden Baseline</label>
                <select
                  className="w-full p-2 border rounded"
                  value={selectedGolden}
                  onChange={(e) => setSelectedGolden(e.target.value)}
                >
                  <option value="" disabled>Select baseline...</option>
                  {goldenRuns.map((name) => (
                    <option key={name} value={name}>{name}</option>
                  ))}
                </select>
              </div>

              <div className="mb-4">
                <label className="font-medium text-sm mb-1">Strictness</label>
                <select
                  className="w-full p-2 border rounded"
                  value={strictness}
                  onChange={(e) => setStrictness(e.target.value as Strictness)}
                >
                  <option value="epsilon-tolerant">Epsilon-tolerant (default)</option>
                  <option value="bitwise">Bitwise</option>
                  <option value="statistical">Statistical</option>
                </select>
              </div>

              <div className="mb-4">
                <label className="font-medium text-sm mb-1">Verification Toggles</label>
                <div className="flex flex-wrap gap-3">
                  <label className="flex items-center gap-2 text-sm"><input type="checkbox" checked={verifyToolchain} onChange={e=>setVerifyToolchain(e.target.checked)} />Toolchain</label>
                  <label className="flex items-center gap-2 text-sm"><input type="checkbox" checked={verifyAdapters} onChange={e=>setVerifyAdapters(e.target.checked)} />Adapters</label>
                  <label className="flex items-center gap-2 text-sm"><input type="checkbox" checked={verifySignature} onChange={e=>setVerifySignature(e.target.checked)} />Signature</label>
                  <label className="flex items-center gap-2 text-sm"><input type="checkbox" checked={verifyDevice} onChange={e=>setVerifyDevice(e.target.checked)} />Device</label>
                </div>
              </div>
            </div>

            <div className="flex gap-2">
              <Button onClick={runGoldenCompare} disabled={!bundleId || !selectedGolden}>
                Run Compare
              </Button>
            </div>

            {compareResult && (
              <div className="space-y-4">
                <Alert variant={compareResult.passed ? 'default' : 'destructive'}>
                  <AlertDescription>
                    {compareResult.passed ? '✓ Verification PASSED' : '✗ Verification FAILED'}
                  </AlertDescription>
                </Alert>

                <div className="flex flex-wrap gap-2">
                  <Badge variant={compareResult.bundle_hash_match ? 'default' : 'secondary'}>Bundle Hash {compareResult.bundle_hash_match ? 'Match' : 'Differs'}</Badge>
                  <Badge variant={compareResult.signature_verified ? 'default' : 'secondary'}>Signature {compareResult.signature_verified ? 'Verified' : 'Not Verified'}</Badge>
                  <Badge variant={compareResult.toolchain_compatible ? 'default' : 'destructive'}>Toolchain {compareResult.toolchain_compatible ? 'Compatible' : 'Mismatch'}</Badge>
                  <Badge variant={compareResult.adapters_compatible ? 'default' : 'destructive'}>Adapters {compareResult.adapters_compatible ? 'Match' : 'Mismatch'}</Badge>
                  <Badge variant={compareResult.device_compatible ? 'default' : 'secondary'}>Device {compareResult.device_compatible ? 'Match' : 'Different'}</Badge>
                </div>

                {compareResult.messages && compareResult.messages.length > 0 && (
                  <div className="space-y-1">
                    {compareResult.messages.map((m, idx) => (
                      <div key={idx} className="text-xs text-muted-foreground">{m}</div>
                    ))}
                  </div>
                )}

                {/* View mode selector */}
                <div className="flex items-center justify-between border-b pb-2">
                  <div className="flex items-center gap-2">
                    <Button
                      variant={viewMode === 'table' ? 'default' : 'outline'}
                      size="sm"
                      onClick={() => setViewMode('table')}
                    >
                      Table View
                    </Button>
                    <Button
                      variant={viewMode === 'heatmap' ? 'default' : 'outline'}
                      size="sm"
                      onClick={() => setViewMode('heatmap')}
                    >
                      Heatmap
                    </Button>
                    <Button
                      variant={viewMode === 'stats' ? 'default' : 'outline'}
                      size="sm"
                      onClick={() => setViewMode('stats')}
                    >
                      Statistics
                    </Button>
                  </div>

                  {adapterPrefixes.length > 0 && (
                    <div className="flex items-center gap-2">
                      <span className="text-sm text-muted-foreground">Filter by adapter:</span>
                      {adapterPrefixes.map(prefix => (
                        <Badge
                          key={prefix}
                          onClick={() => setAdapterFilter(adapterFilter === prefix ? null : prefix)}
                          className={`cursor-pointer ${adapterFilter === prefix ? 'bg-blue-600 text-white' : ''}`}
                        >
                          {prefix}
                        </Badge>
                      ))}
                      {adapterFilter && (
                        <Button variant="outline" size="sm" onClick={() => setAdapterFilter(null)}>Clear</Button>
                      )}
                    </div>
                  )}
                </div>

                {compareResult.epsilon_comparison.divergent_layers.length === 0 ? (
                  <Alert>
                    <AlertDescription>No divergences detected. Outputs match within tolerance.</AlertDescription>
                  </Alert>
                ) : (
                  <>
                    {viewMode === 'table' && (
                      <div className="space-y-4">
                        <div className="flex items-center gap-3">
                          <span className="text-sm text-muted-foreground">
                            Divergences: {compareResult.epsilon_comparison.divergent_layers.length.toLocaleString()}
                          </span>
                          <div className="flex items-center gap-2 text-sm">
                            <span className="text-muted-foreground">Sort</span>
                            <select className="border rounded px-2 py-1" value={sortKey} onChange={e => setSortKey(e.target.value as any)}>
                              <option value="rel">Rel Error</option>
                              <option value="g_l2">Golden L2</option>
                              <option value="c_l2">Current L2</option>
                            </select>
                            <select className="border rounded px-2 py-1" value={sortDir} onChange={e => setSortDir(e.target.value as any)}>
                              <option value="desc">Desc</option>
                              <option value="asc">Asc</option>
                            </select>
                          </div>
                          {compareResult.epsilon_comparison.divergent_layers.length > 100 && (
                            <Button variant="outline" size="sm" onClick={() => setLimitRows(!limitRows)}>
                              {limitRows ? 'Show All' : 'Show Top 100'}
                            </Button>
                          )}
                          <Button variant="outline" size="sm" onClick={() => exportCsv(false)}>Export CSV (Shown)</Button>
                          <Button variant="outline" size="sm" onClick={() => exportCsv(true)}>Export CSV (All)</Button>
                        </div>

                        <div className="overflow-auto">
                          <Table className="table-standard">
                            <TableHeader>
                              <TableRow>
                                <TableHead>Layer</TableHead>
                                <TableHead>Rel Error</TableHead>
                                <TableHead>Golden L2</TableHead>
                                <TableHead>Current L2</TableHead>
                                <TableHead>Golden Max</TableHead>
                                <TableHead>Current Max</TableHead>
                                <TableHead>Golden Mean</TableHead>
                                <TableHead>Current Mean</TableHead>
                              </TableRow>
                            </TableHeader>
                            <TableBody>
                              {filteredDivergences.map(div => (
                                <TableRow
                                  key={div.layer_id}
                                  className="cursor-pointer hover:bg-muted/50"
                                  onClick={() => setSelectedLayerDetail(div)}
                                >
                                  <TableCell className="font-mono text-xs">{div.layer_id}</TableCell>
                                  <TableCell>{div.relative_error.toExponential(2)}</TableCell>
                                  <TableCell>{div.golden.l2_error.toExponential(2)}</TableCell>
                                  <TableCell>{div.current.l2_error.toExponential(2)}</TableCell>
                                  <TableCell>{div.golden.max_error.toExponential(2)}</TableCell>
                                  <TableCell>{div.current.max_error.toExponential(2)}</TableCell>
                                  <TableCell>{div.golden.mean_error.toExponential(2)}</TableCell>
                                  <TableCell>{div.current.mean_error.toExponential(2)}</TableCell>
                                </TableRow>
                              ))}
                            </TableBody>
                          </Table>
                        </div>
                      </div>
                    )}

                    {viewMode === 'heatmap' && (
                      <EpsilonHeatmap
                        divergences={filteredDivergences}
                        tolerance={compareResult.epsilon_comparison.tolerance}
                        adapterFilter={adapterFilter}
                        onAdapterClick={(prefix) => setAdapterFilter(adapterFilter === prefix ? null : prefix)}
                      />
                    )}

                    {viewMode === 'stats' && (
                      <StatisticalSummary
                        divergences={compareResult.epsilon_comparison.divergent_layers}
                        tolerance={compareResult.epsilon_comparison.tolerance}
                      />
                    )}
                  </>
                )}
              </div>
            )}
          </div>

          <DialogFooter>
            <Button variant="outline" onClick={() => onOpenChange(false)}>Close</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {selectedLayerDetail && (
        <LayerDetailModal
          layer={selectedLayerDetail}
          tolerance={compareResult?.epsilon_comparison.tolerance || 1e-5}
          open={!!selectedLayerDetail}
          onOpenChange={(open) => !open && setSelectedLayerDetail(null)}
        />
      )}
    </>
  );
}

export default GoldenCompareModalEnhanced;
