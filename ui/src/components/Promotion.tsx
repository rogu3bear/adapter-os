import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './ui/table';
import { ArrowUp, CheckCircle, XCircle, AlertTriangle, Play, History } from 'lucide-react';
import apiClient from '../api/client';
import { PromotionGate, User, DryRunPromotionResponse, PromotionHistoryEntry } from '../api/types';
import { Alert, AlertDescription } from './ui/alert';
import { toast } from 'sonner';

interface PromotionProps {
  user: User;
  selectedTenant: string;
}

export function Promotion({ user, selectedTenant }: PromotionProps) {
  const [cpid, setCpid] = useState('');
  const [gates, setGates] = useState<PromotionGate[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);
  const [dryRunResult, setDryRunResult] = useState<DryRunPromotionResponse | null>(null);
  const [history, setHistory] = useState<PromotionHistoryEntry[]>([]);
  const [loadingHistory, setLoadingHistory] = useState(false);

  useEffect(() => {
    fetchHistory();
  }, []);

  const fetchHistory = async () => {
    setLoadingHistory(true);
    try {
      const data = await apiClient.getPromotionHistory();
      setHistory(data);
    } catch (err) {
      console.error('Failed to fetch promotion history:', err);
    } finally {
      setLoadingHistory(false);
    }
  };

  const handleDryRun = async () => {
    if (!cpid) {
      toast.error('Please enter a CPID');
      return;
    }
    setLoading(true);
    setError(null);
    setDryRunResult(null);
    try {
      const result = await apiClient.dryRunPromotion(cpid);
      setDryRunResult(result);
      toast.success('Dry run complete');
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Dry run failed';
      setError(errorMsg);
      toast.error(errorMsg);
    } finally {
      setLoading(false);
    }
  };

  const checkGates = async () => {
    if (!cpid) {
      toast.error('Please enter a CPID');
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const data = await apiClient.getPromotionGates(cpid);
      setGates(data);
      toast.success('Gates checked successfully');
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to check gates';
      setError(errorMsg);
      toast.error(errorMsg);
    } finally {
      setLoading(false);
    }
  };

  const handlePromote = async () => {
    if (!cpid) {
      toast.error('Please enter a CPID');
      return;
    }
    setLoading(true);
    setError(null);
    setSuccess(null);
    try {
      await apiClient.promote({ cpid });
      const successMsg = `Successfully promoted ${cpid}`;
      setSuccess(successMsg);
      toast.success(successMsg);
      setGates([]);
      setCpid('');
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Promotion failed';
      setError(errorMsg);
      toast.error(errorMsg);
    } finally {
      setLoading(false);
    }
  };

  const allGatesPassed = gates.every(g => g.status === 'passed');

  return (
    <div className="space-y-6">
      <div className="section-header">
        <h1 className="section-title">Control Plane Promotion</h1>
        <p className="section-description">
          Promote validated control plane configurations to production
        </p>
      </div>

      {error && (
        <Alert variant="destructive">
          <XCircle className="icon-standard" />
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}

      {success && (
        <Alert>
          <CheckCircle className="icon-standard" />
          <AlertDescription>{success}</AlertDescription>
        </Alert>
      )}

      <Card className="card-standard">
        <CardHeader>
          <CardTitle>Promotion Gate Check</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="form-field">
            <Label htmlFor="cpid" className="form-label">Control Plane ID (CPID)</Label>
            <div className="flex-standard">
              <Input
                id="cpid"
                value={cpid}
                onChange={(e) => setCpid(e.target.value)}
                placeholder="Enter CPID (e.g., cp-20240315-abc123)"
              />
              <Button onClick={checkGates} disabled={!cpid || loading}>
                Check Gates
              </Button>
              <Button onClick={handleDryRun} disabled={!cpid || loading} variant="outline">
                <Play className="icon-standard mr-2" />
                Dry Run
              </Button>
            </div>
          </div>

          {dryRunResult && (
            <div className="space-y-3 border-t pt-4">
              <h3 className="font-medium">Dry Run Results</h3>
              <Alert variant={dryRunResult.would_promote ? 'default' : 'destructive'}>
                <AlertDescription>
                  {dryRunResult.would_promote ? 
                    '✓ Would promote successfully' : 
                    `✗ Would NOT promote (${dryRunResult.validation_errors.length} errors)`}
                </AlertDescription>
              </Alert>
              <div className="space-y-2">
                {dryRunResult.gate_results.map((gate, idx) => (
                  <div key={idx} className="flex-between p-2 border rounded">
                    <div className="flex-center">
                      {gate[1] ? (
                        <CheckCircle className="icon-standard text-green-600" />
                      ) : (
                        <XCircle className="icon-standard text-red-600" />
                      )}
                      <span className="text-sm font-medium">{gate[0]}</span>
                    </div>
                    {gate[2] && <span className="text-xs text-muted-foreground">{gate[2]}</span>}
                  </div>
                ))}
              </div>
            </div>
          )}

          {gates.length > 0 && (
            <div className="space-y-3">
              <h3 className="font-medium">Gate Status</h3>
              {gates.map((gate, idx) => (
                <div
                  key={idx}
                  className="flex-between p-3 border rounded-lg"
                >
                  <div className="flex-center">
                    {gate.status === 'passed' && (
                      <CheckCircle className="icon-large text-green-600" />
                    )}
                    {gate.status === 'failed' && (
                      <XCircle className="icon-large text-red-600" />
                    )}
                    {gate.status === 'pending' && (
                      <AlertTriangle className="icon-large text-yellow-600" />
                    )}
                    <div>
                      <p className="font-medium">{gate.name}</p>
                      <p className="text-sm text-muted-foreground">{gate.message}</p>
                    </div>
                  </div>
                  <Badge
                    variant={
                      gate.status === 'passed'
                        ? 'default'
                        : gate.status === 'failed'
                        ? 'destructive'
                        : 'secondary'
                    }
                  >
                    {gate.status}
                  </Badge>
                </div>
              ))}

              <Button
                onClick={handlePromote}
                disabled={!allGatesPassed || loading}
                className="w-full"
              >
                <ArrowUp className="icon-standard mr-2" />
                {allGatesPassed ? 'Promote to Production' : 'Gates Must Pass to Promote'}
              </Button>
            </div>
          )}
        </CardContent>
      </Card>

      <Card className="card-standard">
        <CardHeader>
          <CardTitle className="flex-center">
            <History className="icon-large mr-2" />
            Promotion History
          </CardTitle>
        </CardHeader>
        <CardContent>
          {loadingHistory ? (
            <div className="text-center py-4 text-muted-foreground">Loading history...</div>
          ) : (
            <Table className="table-standard">
              <TableHeader>
                <TableRow>
                  <TableHead>CPID</TableHead>
                  <TableHead>Promoted By</TableHead>
                  <TableHead>Previous CPID</TableHead>
                  <TableHead>Gate Results</TableHead>
                  <TableHead>Promoted At</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {history.map((entry) => (
                  <TableRow key={entry.cpid}>
                    <TableCell className="table-cell-standard font-mono text-sm">{entry.cpid}</TableCell>
                    <TableCell className="table-cell-standard">{entry.promoted_by}</TableCell>
                    <TableCell className="table-cell-standard font-mono text-xs">
                      {entry.previous_cpid || 'N/A'}
                    </TableCell>
                    <TableCell className="table-cell-standard">
                      <Badge variant="default">{entry.gate_results_summary}</Badge>
                    </TableCell>
                    <TableCell className="table-cell-standard text-sm text-muted-foreground">
                      {new Date(entry.promoted_at).toLocaleString()}
                    </TableCell>
                  </TableRow>
                ))}
                {history.length === 0 && (
                  <TableRow>
                    <TableCell colSpan={5} className="table-cell-standard text-center text-muted-foreground">
                      No promotion history available
                    </TableCell>
                  </TableRow>
                )}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>
    </div>
  );
}