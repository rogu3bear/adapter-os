import React, { useState, useEffect, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './ui/table';
import { Alert, AlertDescription } from './ui/alert';
import apiClient from '@/api/client';
import { User, PromotionGate, DryRunPromotionResponse, PromotionHistoryEntry } from '@/api/types';
import { logger, toError } from '@/utils/logger';
import { ArrowUp, History, Undo2, Play, CheckCircle } from 'lucide-react';
import { errorRecoveryTemplates } from './ui/error-recovery';
import { EmptyState } from './ui/empty-state';
import { LoadingState } from './ui/loading-state';
import { GlossaryTooltip } from './ui/glossary-tooltip';
import { useRBAC } from '@/hooks/useRBAC';

interface PromotionProps {
  user: User;
  selectedTenant: string;
}

export function Promotion({ user, selectedTenant }: PromotionProps) {
  const { can } = useRBAC();
  const [cpid, setCpid] = useState('');
  const [planId, setPlanId] = useState('');
  const [gates, setGates] = useState<PromotionGate[]>([]);
  const [dryRunResult, setDryRunResult] = useState<DryRunPromotionResponse | null>(null);
  const [history, setHistory] = useState<PromotionHistoryEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [statusMessage, setStatusMessage] = useState<{ message: string; variant: 'success' | 'info' | 'warning' } | null>(null);
  const [errorRecovery, setErrorRecovery] = useState<React.ReactElement | null>(null);

  const canExecute = can('promotion:execute');
  const canView = can('promotion:view');

  useEffect(() => {
    if (loading) {
      logger.debug('Promotion: action in-flight', {
        component: 'Promotion',
        tenantId: selectedTenant,
      });
    }
  }, [loading, selectedTenant]);

  useEffect(() => {
    if (!loading && history.length === 0) {
      logger.info('Promotion: no history entries found', {
        component: 'Promotion',
        tenantId: selectedTenant,
      });
    }
  }, [history.length, loading, selectedTenant]);

  useEffect(() => {
    if (!loading && gates.length === 0) {
      logger.debug('Promotion: gate results empty', {
        component: 'Promotion',
        tenantId: selectedTenant,
      });
    }
  }, [gates.length, loading, selectedTenant]);

  const fetchHistory = useCallback(async () => {
    try {
      const data = await apiClient.getPromotionHistory();
      setHistory(data);
      setStatusMessage(null);
      setErrorRecovery(null);
      logger.info('Promotion: history loaded', {
        component: 'Promotion',
        tenantId: selectedTenant,
        entryCount: data.length,
      });
    } catch (err) {
      setStatusMessage({ message: 'Failed to load history.', variant: 'warning' });
      setErrorRecovery(
        errorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error('Failed to load history.'),
          () => fetchHistory()
        )
      );
      logger.error(
        'Promotion: history load failed',
        { component: 'Promotion', tenantId: selectedTenant },
        toError(err),
      );
    }
  }, [selectedTenant]);

  useEffect(() => {
    fetchHistory();
  }, [fetchHistory]);

  const handleDryRun = async () => {
    setLoading(true);
    try {
      const result = await apiClient.dryRunPromotion(cpid);
      setDryRunResult(result);
      setStatusMessage({ message: 'Dry run completed.', variant: 'info' });
      setError(null);
      logger.info('Promotion: dry run completed', {
        component: 'Promotion',
        tenantId: selectedTenant,
        cpid,
      });
    } catch (err) {
      setError('Dry run failed');
      setStatusMessage({ message: 'Dry run failed.', variant: 'warning' });
      setErrorRecovery(
        errorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error('Dry run failed.'),
          () => handleDryRun()
        )
      );
      logger.error(
        'Promotion: dry run failed',
        { component: 'Promotion', tenantId: selectedTenant, cpid },
        toError(err),
      );
    } finally {
      setLoading(false);
    }
  };

  const handleCheckGates = async () => {
    setLoading(true);
    try {
      const data = await apiClient.getPromotionGates(cpid);
      setGates(data);
      setStatusMessage({ message: 'Gate check completed.', variant: 'info' });
      setError(null);
      logger.info('Promotion: gate check completed', {
        component: 'Promotion',
        tenantId: selectedTenant,
        cpid,
        gateCount: data.length,
      });
    } catch (err) {
      setError('Gate check failed');
      setStatusMessage({ message: 'Gate check failed.', variant: 'warning' });
      setErrorRecovery(
        errorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error('Gate check failed.'),
          () => handleCheckGates()
        )
      );
      logger.error(
        'Promotion: gate check failed',
        { component: 'Promotion', tenantId: selectedTenant, cpid },
        toError(err),
      );
    } finally {
      setLoading(false);
    }
  };

  const handlePromote = async () => {
    setLoading(true);
    try {
      await apiClient.promote({ tenant_id: selectedTenant, cpid, plan_id: planId });
      setStatusMessage({ message: 'Promoted successfully.', variant: 'success' });
      logger.info('Promotion: promote executed', {
        component: 'Promotion',
        tenantId: selectedTenant,
        cpid,
        planId,
      });
      fetchHistory();
    } catch (err) {
      setError('Promotion failed');
      setStatusMessage({ message: 'Promotion failed.', variant: 'warning' });
      setErrorRecovery(
        errorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error('Promotion failed.'),
          () => handlePromote()
        )
      );
      logger.error(
        'Promotion: promote failed',
        { component: 'Promotion', tenantId: selectedTenant, cpid, planId },
        toError(err),
      );
    } finally {
      setLoading(false);
    }
  };

  const handleRollback = async () => {
    setLoading(true);
    try {
      await apiClient.rollback();
      setStatusMessage({ message: 'Rollback successful.', variant: 'success' });
      logger.warn('Promotion: rollback executed', {
        component: 'Promotion',
        tenantId: selectedTenant,
      });
      fetchHistory();
    } catch (err) {
      setError('Rollback failed');
      setStatusMessage({ message: 'Rollback failed.', variant: 'warning' });
      setErrorRecovery(
        errorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error('Rollback failed.'),
          () => handleRollback()
        )
      );
      logger.error(
        'Promotion: rollback failed',
        { component: 'Promotion', tenantId: selectedTenant },
        toError(err),
      );
    } finally {
      setLoading(false);
    }
  };

  const allGatesPassed = gates.every(g => g.status === 'passed');

  return (
    <div className="space-y-6">
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

      {loading && (
        <LoadingState
          size="sm"
          skeletonLines={0}
          title="Processing promotion request"
          description="Executing the selected promotion workflow."
          className="border-none bg-transparent p-0"
        />
      )}

      <Card>
        <CardHeader>
          <CardTitle>Promotion Controls</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div>
            <Label className="flex items-center gap-1">
              Policy ID
              <GlossaryTooltip termId="promotion-cpid" />
            </Label>
            <Input value={cpid} onChange={(e) => setCpid(e.target.value)} />
          </div>
          <div>
            <Label className="flex items-center gap-1">
              Plan ID
              <GlossaryTooltip termId="promotion-plan-id" />
            </Label>
            <Input value={planId} onChange={(e) => setPlanId(e.target.value)} />
          </div>
          <div className="flex gap-2 flex-wrap">
            <Button onClick={handleDryRun} disabled={loading || !canView}>
              <Play className="mr-2 h-4 w-4" /> Dry Run
              <GlossaryTooltip termId="promotion-dry-run" />
            </Button>
            <Button onClick={handleCheckGates} disabled={loading || !canView}>
              <CheckCircle className="mr-2 h-4 w-4" /> Check Gates
              <GlossaryTooltip termId="promotion-gates" />
            </Button>
            <Button onClick={handlePromote} disabled={loading || !allGatesPassed || !canExecute}>
              <ArrowUp className="mr-2 h-4 w-4" /> Promote
              <GlossaryTooltip termId="promotion-execute" />
            </Button>
            <Button variant="destructive" onClick={handleRollback} disabled={loading || !canExecute}>
              <Undo2 className="mr-2 h-4 w-4" /> Rollback
              <GlossaryTooltip termId="promotion-rollback" />
            </Button>
          </div>
          {!canExecute && (
            <Alert>
              <AlertDescription className="text-muted-foreground">
                You need the promotion:execute permission to promote or rollback adapters.
              </AlertDescription>
            </Alert>
          )}
          {error && <Alert variant="destructive"><AlertDescription>{error}</AlertDescription></Alert>}
        </CardContent>
      </Card>

      {/* Gate Visualization */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-1">
            Gate Status
            <GlossaryTooltip termId="promotion-gates" />
          </CardTitle>
        </CardHeader>
        <CardContent>
          {gates.length === 0 ? (
            <EmptyState
              icon={CheckCircle}
              title="No gate evaluations yet"
              description="Run Check Gates to evaluate promotion safety. Gate outcomes appear here."
            />
          ) : (
            <div className="space-y-2">
              {gates.map((gate, idx) => (
                <div key={idx} className="flex items-center justify-between rounded border p-2">
                  <span>{gate.name}</span>
                  <Badge variant={gate.status === 'passed' ? 'default' : 'destructive'}>{gate.status}</Badge>
                </div>
              ))}
            </div>
          )}
        </CardContent>
      </Card>

      {/* Dry Run Preview */}
      {dryRunResult && (
        <Card>
          <CardHeader>
            <CardTitle>Dry Run Preview</CardTitle>
          </CardHeader>
          <CardContent>
            <pre className="text-sm overflow-auto max-h-48">{JSON.stringify(dryRunResult, null, 2)}</pre>
          </CardContent>
        </Card>
      )}

      {/* Promotion History */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-1">
            Promotion History
            <GlossaryTooltip termId="promotion-history" />
          </CardTitle>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Policy ID</TableHead>
                <TableHead>By</TableHead>
                <TableHead>Date</TableHead>
                <TableHead>Status</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {history.length === 0 ? (
                <TableRow>
                  <TableCell colSpan={4}>
                    <EmptyState
                      icon={History}
                      title="No promotions recorded"
                      description="Run a promotion or rollback to populate the timeline."
                    />
                  </TableCell>
                </TableRow>
              ) : (
                history.map((entry, idx) => (
                  <TableRow key={idx}>
                    <TableCell>{entry.cpid}</TableCell>
                    <TableCell>{entry.promoted_by}</TableCell>
                    <TableCell>{new Date(entry.promoted_at).toLocaleString()}</TableCell>
                    <TableCell><Badge>{entry.status}</Badge></TableCell>
                  </TableRow>
                ))
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
    </div>
  );
}
