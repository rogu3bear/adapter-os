import React, { useState, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from './ui/dialog';
import { Input } from './ui/input';
import { Button } from './ui/button';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Badge } from './ui/badge';
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from './ui/accordion';
import { Shield, Play, Hash, Trash2 } from 'lucide-react';
import apiClient from '../api/client';
import { ReplaySession, ReplayVerificationResponse } from '../api/types';
import { useTimestamp } from '../hooks/useTimestamp';
import { toast } from 'sonner';
import { ErrorRecovery, errorRecoveryTemplates } from './ui/error-recovery';
import { HelpTooltip } from './ui/help-tooltip';
import { logger, toError } from '../utils/logger';
import { usePolling } from '../hooks/usePolling';
import { useRBAC } from '@/hooks/useRBAC';

import { useTenant } from '@/layout/LayoutProvider';

interface ReplayPanelProps {
  tenantId?: string;
  onSessionSelect: (session: ReplaySession | null) => void;
}

export function ReplayPanel({ tenantId: tenantProp, onSessionSelect }: ReplayPanelProps) {
  const { selectedTenant } = useTenant();
  const tenantId = tenantProp ?? selectedTenant;
  const [selectedSession, setSelectedSession] = useState<string | null>(null);
  const { can } = useRBAC();

  const [verifying, setVerifying] = useState(false);
  const [verifyResponse, setVerifyResponse] = useState<ReplayVerificationResponse | null>(null);
  const [createOpen, setCreateOpen] = useState(false);
  const [newCpid, setNewCpid] = useState('');
  const [newPlanId, setNewPlanId] = useState('');
  const [newBundleIds, setNewBundleIds] = useState('');

  // RBAC permissions
  const canViewAudit = can('audit:view');
  const canCreateSession = can('audit:view'); // Create replay sessions requires audit view
  const canDeleteSession = can('audit:view'); // Delete requires audit view

  // Use polling for session list
  const fetchSessions = useCallback(async () => {
    return apiClient.listReplaySessions(tenantId);
  }, [tenantId]);

  const {
    data: sessions,
    isLoading: loading,
    error: replayError,
    refetch
  } = usePolling<ReplaySession[]>(
    fetchSessions,
    'normal',
    {
      operationName: 'listReplaySessions',
      onError: (err) => {
        logger.error('Failed to load replay sessions', {
          component: 'ReplayPanel',
          operation: 'fetchSessions',
          tenantId,
        }, toError(err));
      }
    }
  );

  const handleDeleteSession = async (sessionId: string) => {
    if (!canDeleteSession) {
      toast.error('Permission denied: audit:view required to delete sessions');
      return;
    }

    try {
      await apiClient.deleteReplaySession(sessionId);
      toast.success('Replay session deleted');
      refetch();
      if (selectedSession === sessionId) {
        setSelectedSession(null);
        onSessionSelect(null);
      }
    } catch (err) {
      logger.error('Failed to delete replay session', {
        component: 'ReplayPanel',
        operation: 'deleteSession',
        sessionId,
        tenantId,
      }, toError(err));
      toast.error('Failed to delete session');
    }
  };

  const handleSessionSelect = async (sessionId: string) => {
    setSelectedSession(sessionId);
    const session = (sessions || []).find(s => s.id === sessionId);
    if (session) {
      onSessionSelect(session);
    }
  };

  const handleVerify = async () => {
    if (!selectedSession) return;

    if (!canViewAudit) {
      toast.error('Permission denied: audit:view required to verify sessions');
      return;
    }

    setVerifying(true);
    try {
      const result = await apiClient.verifyReplaySession(selectedSession);

      setVerifyResponse(result);

      if (result.signature_valid && result.hash_chain_valid) {
        toast.success('Verification passed');
      } else {
        logger.warn('Replay verification failed', {
          component: 'ReplayPanel',
          operation: 'verify',
          sessionId: selectedSession,
          tenantId,
          divergenceCount: result.divergences.length,
        });
        toast.error(`Verification failed: ${result.divergences.length} divergences found`);
      }
    } catch (err) {
      logger.error('Replay verification error', {
        component: 'ReplayPanel',
        operation: 'verify',
        sessionId: selectedSession,
        tenantId,
      }, toError(err));
      toast.error('Verification error');
    } finally {
      setVerifying(false);
    }
  };

  const currentSession = (sessions || []).find(s => s.id === selectedSession);


  if (replayError) {
    return (
      <ErrorRecovery
        error={replayError instanceof Error ? replayError.message : String(replayError)}
        onRetry={refetch}
      />
    );
  }

  return (
    <div>
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Play className="mr-2 h-5 w-5" />
            Replay Sessions
            <HelpTooltip helpId="replay-session" />
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">

          <div className="flex justify-end">
            <Button
              variant="outline"
              onClick={() => setCreateOpen(true)}
              disabled={!canCreateSession}
              title={!canCreateSession ? 'Requires audit:view permission' : undefined}
            >
              New Session
            </Button>
          </div>

          <Select value={selectedSession || ''} onValueChange={handleSessionSelect}>
            <SelectTrigger>
              <SelectValue placeholder="Select replay session" />
            </SelectTrigger>
            <SelectContent>
              {(sessions || []).filter(session => session.id && session.id !== '').map(session => (
                <SelectItem key={session.id} value={session.id}>
                  {session.cpid} @ {useTimestamp(session.snapshot_at)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>

          {currentSession && (
            <div className="space-y-3 border rounded p-3">

              <Accordion type="multiple" defaultValue={['basic']} className="w-full">
                <AccordionItem value="basic">
                  <AccordionTrigger>
                    <div className="flex items-center gap-2">
                      <span className="text-sm font-medium">Session Overview</span>
                    </div>
                  </AccordionTrigger>
                  <AccordionContent>
                    <div className="grid grid-cols-2 gap-2 text-sm pt-2">
                      <div>
                        <span className="text-muted-foreground">Policy ID:</span>
                        <div className="font-mono">{currentSession.cpid}</div>
                      </div>
                      <div>
                        <span className="text-muted-foreground">Plan ID:</span>
                        <div className="font-mono">{currentSession.plan_id}</div>
                      </div>
                      <div>
                        <span className="text-muted-foreground">Snapshot:</span>
                        <div>{useTimestamp(currentSession.snapshot_at)}</div>
                      </div>
                      <div>
                        <span className="text-muted-foreground">Bundles:</span>
                        <div>{currentSession.telemetry_bundle_ids.length}</div>
                      </div>
                    </div>
                  </AccordionContent>
                </AccordionItem>

                <AccordionItem value="hashes">
                  <AccordionTrigger>
                    <div className="flex items-center gap-2">
                      <Hash className="h-4 w-4" />
                      <span className="text-sm font-medium">Cryptographic Hashes</span>
                    </div>
                  </AccordionTrigger>
                  <AccordionContent>
                    <div className="space-y-1 pt-2">
                      <div className="flex items-center gap-2 text-xs">
                        <Hash className="h-3 w-3" />
                        <span className="text-muted-foreground">Manifest:</span>
                        <HelpTooltip helpId="replay-manifest-hash" />
                        <code className="font-mono border border-border px-1 rounded">{currentSession.manifest_hash_b3.substring(0, 16)}...</code>
                      </div>
                      <div className="flex items-center gap-2 text-xs">
                        <Hash className="h-3 w-3" />
                        <span className="text-muted-foreground">Policy:</span>
                        <HelpTooltip helpId="replay-policy-hash" />
                        <code className="font-mono border border-border px-1 rounded">{currentSession.policy_hash_b3.substring(0, 16)}...</code>
                      </div>
                      {currentSession.kernel_hash_b3 && (
                        <div className="flex items-center gap-2 text-xs">
                          <Hash className="h-3 w-3" />
                          <span className="text-muted-foreground">Kernel:</span>
                          <HelpTooltip helpId="replay-kernel-hash" />
                          <code className="font-mono border border-border px-1 rounded">{currentSession.kernel_hash_b3.substring(0, 16)}...</code>
                        </div>
                      )}
                    </div>
                  </AccordionContent>
                </AccordionItem>
              </Accordion>

              <div className="grid grid-cols-2 gap-2 text-sm">
                <div>
                  <span className="text-muted-foreground">Policy ID:</span>
                  <div className="font-mono">{currentSession.cpid}</div>
                </div>
                <div>
                  <span className="text-muted-foreground">Plan ID:</span>
                  <div className="font-mono">{currentSession.plan_id}</div>
                </div>
                <div>
                  <span className="text-muted-foreground">Snapshot:</span>
                  <div>{useTimestamp(currentSession.snapshot_at)}</div>
                </div>
                <div>
                  <span className="text-muted-foreground">Bundles:</span>
                  <div>{currentSession.telemetry_bundle_ids.length}</div>
                </div>
              </div>

              <div className="space-y-1">
                <div className="flex items-center gap-2 text-xs">
                  <Hash className="h-3 w-3" />
                  <span className="text-muted-foreground">Manifest:</span>
                  <code className="font-mono">{currentSession.manifest_hash_b3.substring(0, 16)}...</code>
                </div>
                <div className="flex items-center gap-2 text-xs">
                  <Hash className="h-3 w-3" />
                  <span className="text-muted-foreground">Policy:</span>
                  <code className="font-mono">{currentSession.policy_hash_b3.substring(0, 16)}...</code>
                </div>
                {currentSession.kernel_hash_b3 && (
                  <div className="flex items-center gap-2 text-xs">
                    <Hash className="h-3 w-3" />
                    <span className="text-muted-foreground">Kernel:</span>
                    <code className="font-mono">{currentSession.kernel_hash_b3.substring(0, 16)}...</code>
                  </div>
                )}
              </div>

              <div className="flex items-center gap-2">
                <Button
                  onClick={handleVerify}
                  disabled={verifying || !canViewAudit}
                  className="flex-1"
                  variant="outline"
                  title={!canViewAudit ? 'Requires audit:view permission' : undefined}
                >
                  <Shield className="mr-2 h-4 w-4" />
                  {verifying ? 'Verifying...' : 'Verify Cryptographic Chain'}
                </Button>
                <HelpTooltip helpId="replay-verification" />
                {canDeleteSession && (
                  <Button
                    onClick={() => handleDeleteSession(currentSession.id)}
                    variant="outline"
                    size="icon"
                    className="text-destructive hover:text-destructive"
                  >
                    <Trash2 className="h-4 w-4" />
                  </Button>
                )}
              </div>


              {verifyResponse && (
                <div className="space-y-2">
                  <div className="text-sm">
                    Result: {verifyResponse.signature_valid && verifyResponse.hash_chain_valid ? 'Pass' : 'Fail'}
                  </div>
                  {verifyResponse.divergences && verifyResponse.divergences.length > 0 && (
                    <details className="p-2 border rounded">
                      <summary className="cursor-pointer text-sm flex items-center gap-1">
                        {verifyResponse.divergences.length} divergences
                        <HelpTooltip helpId="replay-divergence" />
                      </summary>
                      <ul className="mt-2 space-y-1 text-xs font-mono">
                        {verifyResponse.divergences.map((d, idx) => (
                          <li key={idx}>
                            [{d.divergence_type}] expected={d.expected_hash.substring(0, 16)} actual={d.actual_hash.substring(0, 16)} — {d.context}
                          </li>
                        ))}
                      </ul>
                      <div className="mt-2 flex gap-2">
                        <Button
                          size="sm"
                          variant="outline"
                          onClick={() => {
                            const report = {
                              session_id: verifyResponse.session_id,
                              verified_at: verifyResponse.verified_at,
                              result: {
                                signature_valid: verifyResponse.signature_valid,
                                hash_chain_valid: verifyResponse.hash_chain_valid,
                                manifest_verified: verifyResponse.manifest_verified,
                                policy_verified: verifyResponse.policy_verified,
                                kernel_verified: verifyResponse.kernel_verified,
                              },
                              divergences: verifyResponse.divergences,
                            };
                            const dataStr = JSON.stringify(report, null, 2);
                            const blob = new Blob([dataStr], { type: 'application/json' });
                            const url = URL.createObjectURL(blob);
                            const link = document.createElement('a');
                            link.href = url;
                            link.download = `replay-verification-${verifyResponse.session_id}.json`;
                            document.body.appendChild(link);
                            link.click();
                            document.body.removeChild(link);
                            URL.revokeObjectURL(url);
                          }}
                        >
                          Download Report
                        </Button>
                      </div>
                    </details>
                  )}
                </div>
              )}

            </div>
          )}
        </CardContent>
      </Card>


      {/* Create Session Dialog */}
      <Dialog open={createOpen} onOpenChange={setCreateOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>New Replay Session</DialogTitle>
          </DialogHeader>
          <div className="space-y-3">
            <div>
              <label className="font-medium text-sm mb-1">Organization</label>
              <Input value={tenantId} readOnly />
            </div>
            <div>
              <label className="font-medium text-sm mb-1">Policy ID</label>
              <Input value={newCpid} onChange={(e) => setNewCpid(e.target.value)} placeholder="cp_..." />
            </div>
            <div>
              <label className="font-medium text-sm mb-1">Plan ID</label>
              <Input value={newPlanId} onChange={(e) => setNewPlanId(e.target.value)} placeholder="plan_..." />
            </div>
            <div>
              <label className="font-medium text-sm mb-1">Telemetry Bundle IDs (comma-separated)</label>
              <Input value={newBundleIds} onChange={(e) => setNewBundleIds(e.target.value)} placeholder="b1,b2" />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setCreateOpen(false)}>Cancel</Button>
            <Button
              disabled={!canCreateSession}
              onClick={async () => {
                if (!canCreateSession) {
                  toast.error('Permission denied: audit:view required to create sessions');
                  return;
                }
                try {
                  const ids = newBundleIds.split(',').map(s => s.trim()).filter(Boolean);
                  await apiClient.createReplaySession({ tenant_id: tenantId, cpid: newCpid, plan_id: newPlanId, telemetry_bundle_ids: ids });
                  toast.success('Replay session created');
                  setCreateOpen(false);
                  setNewCpid(''); setNewPlanId(''); setNewBundleIds('');
                  refetch();
                } catch (err) {
                  logger.error('Failed to create replay session', {
                    component: 'ReplayPanel',
                    operation: 'createSession',
                    tenantId,
                  }, toError(err));
                  toast.error('Failed to create session');
                }
              }}
            >
              Create
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
