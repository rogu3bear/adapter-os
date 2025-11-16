import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
<<<<<<< HEAD
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from './ui/dialog';
import { Input } from './ui/input';
import { Button } from './ui/button';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Badge } from './ui/badge';
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from './ui/accordion';
import { Shield, Play, Hash } from 'lucide-react';
import apiClient from '../api/client';
import { ReplaySession, ReplayVerificationResponse } from '../api/types';
// 【ui/src/components/ReplayPanel.tsx§1-19】 - Replace toast errors with ErrorRecovery
import { useTimestamp } from '../hooks/useTimestamp';
import { toast } from 'sonner';
import { ErrorRecovery, ErrorRecoveryTemplates } from './ui/error-recovery';
import { logger, toError } from '../utils/logger';
=======
import { Button } from './ui/button';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Badge } from './ui/badge';
import { Shield, Play, Hash } from 'lucide-react';
import apiClient from '../api/client';
import { ReplaySession } from '../api/types';
import { useTimestamp } from '../hooks/useTimestamp';
import { toast } from 'sonner';
>>>>>>> integration-branch

import { useTenant } from '@/layout/LayoutProvider';

interface ReplayPanelProps {
  tenantId?: string;
  onSessionSelect: (session: ReplaySession | null) => void;
}

export function ReplayPanel({ tenantId: tenantProp, onSessionSelect }: ReplayPanelProps) {
  const { selectedTenant } = useTenant();
  const tenantId = tenantProp ?? selectedTenant;
  const [sessions, setSessions] = useState<ReplaySession[]>([]);
  const [selectedSession, setSelectedSession] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
<<<<<<< HEAD
  const [replayError, setReplayError] = useState<Error | null>(null);
  const [verifying, setVerifying] = useState(false);
  const [verifyResponse, setVerifyResponse] = useState<ReplayVerificationResponse | null>(null);
  const [createOpen, setCreateOpen] = useState(false);
  const [newCpid, setNewCpid] = useState('');
  const [newPlanId, setNewPlanId] = useState('');
  const [newBundleIds, setNewBundleIds] = useState('');
=======
  const [verifying, setVerifying] = useState(false);
>>>>>>> integration-branch

  useEffect(() => {
    const fetchSessions = async () => {
      try {
<<<<<<< HEAD
        setReplayError(null);
        const data = await apiClient.listReplaySessions(tenantId);
        setSessions(data);
      } catch (err) {
        const error = err instanceof Error ? err : new Error('Failed to load replay sessions');
        logger.error('Failed to load replay sessions', {
          component: 'ReplayPanel',
          operation: 'fetchSessions',
          tenantId,
        }, toError(err));
        setReplayError(error);
=======
        const data = await apiClient.listReplaySessions(tenantId);
        setSessions(data);
      } catch (err) {
        toast.error('Failed to load replay sessions');
>>>>>>> integration-branch
      } finally {
        setLoading(false);
      }
    };
    fetchSessions();
  }, [tenantId]);

  const handleSessionSelect = async (sessionId: string) => {
    setSelectedSession(sessionId);
    const session = sessions.find(s => s.id === sessionId);
    if (session) {
      onSessionSelect(session);
    }
  };

  const handleVerify = async () => {
    if (!selectedSession) return;
    
    setVerifying(true);
    try {
      const result = await apiClient.verifyReplaySession(selectedSession);
<<<<<<< HEAD
      setVerifyResponse(result);
      
      if (result.signature_valid && result.hash_chain_valid) {
        // Verification success shown in UI
      } else {
        const verificationError = new Error(`Verification failed: ${result.divergences.length} divergences found`);
        logger.warn('Replay verification failed', {
          component: 'ReplayPanel',
          operation: 'verify',
          sessionId: selectedSession,
          tenantId,
          divergenceCount: result.divergences.length,
        });
        setReplayError(verificationError);
      }
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Verification error');
      logger.error('Replay verification error', {
        component: 'ReplayPanel',
        operation: 'verify',
        sessionId: selectedSession,
        tenantId,
      }, toError(err));
      setReplayError(error);
=======
      
      if (result.signature_valid && result.hash_chain_valid) {
        toast.success('Replay session verified successfully');
      } else {
        toast.error(`Verification failed: ${result.divergences.length} divergences found`);
      }
    } catch (err) {
      toast.error('Verification error');
>>>>>>> integration-branch
    } finally {
      setVerifying(false);
    }
  };

  const currentSession = sessions.find(s => s.id === selectedSession);

<<<<<<< HEAD
  if (replayError) {
    return (
      <ErrorRecovery
        title="Replay Panel Error"
        message={replayError.message}
        recoveryActions={[
          { label: 'Retry Loading', action: () => {
            setReplayError(null);
            // Trigger refetch by re-running useEffect
            window.location.reload();
          }},
          { label: 'View Logs', action: () => {/* Navigate to logs */} }
        ]}
      />
    );
  }

  return (
    <div>
=======
  return (
>>>>>>> integration-branch
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center">
          <Play className="mr-2 h-5 w-5" />
          Replay Sessions
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
<<<<<<< HEAD
        <div className="flex justify-end">
          <Button variant="outline" onClick={() => setCreateOpen(true)}>New Session</Button>
        </div>
=======
>>>>>>> integration-branch
        <Select value={selectedSession || ''} onValueChange={handleSessionSelect}>
          <SelectTrigger>
            <SelectValue placeholder="Select replay session" />
          </SelectTrigger>
          <SelectContent>
<<<<<<< HEAD
            {sessions.filter(session => session.id && session.id !== '').map(session => (
=======
            {sessions.map(session => (
>>>>>>> integration-branch
              <SelectItem key={session.id} value={session.id}>
                {session.cpid} @ {useTimestamp(session.snapshot_at)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>

        {currentSession && (
          <div className="space-y-3 border rounded p-3">
<<<<<<< HEAD
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
                      <span className="text-muted-foreground">CPID:</span>
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
                      <code className="font-mono border border-border px-1 rounded">{currentSession.manifest_hash_b3.substring(0, 16)}...</code>
                    </div>
                    <div className="flex items-center gap-2 text-xs">
                      <Hash className="h-3 w-3" />
                      <span className="text-muted-foreground">Policy:</span>
                      <code className="font-mono border border-border px-1 rounded">{currentSession.policy_hash_b3.substring(0, 16)}...</code>
                    </div>
                    {currentSession.kernel_hash_b3 && (
                      <div className="flex items-center gap-2 text-xs">
                        <Hash className="h-3 w-3" />
                        <span className="text-muted-foreground">Kernel:</span>
                        <code className="font-mono border border-border px-1 rounded">{currentSession.kernel_hash_b3.substring(0, 16)}...</code>
                      </div>
                    )}
                  </div>
                </AccordionContent>
              </AccordionItem>
            </Accordion>
=======
            <div className="grid grid-cols-2 gap-2 text-sm">
              <div>
                <span className="text-muted-foreground">CPID:</span>
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
>>>>>>> integration-branch

            <Button 
              onClick={handleVerify} 
              disabled={verifying}
              className="w-full"
              variant="outline"
            >
              <Shield className="mr-2 h-4 w-4" />
              {verifying ? 'Verifying...' : 'Verify Cryptographic Chain'}
            </Button>
<<<<<<< HEAD

            {verifyResponse && (
              <div className="space-y-2">
                <div className="text-sm">
                  Result: {verifyResponse.signature_valid && verifyResponse.hash_chain_valid ? 'Pass' : 'Fail'}
                </div>
                {verifyResponse.divergences && verifyResponse.divergences.length > 0 && (
                  <details className="p-2 border rounded">
                    <summary className="cursor-pointer text-sm">{verifyResponse.divergences.length} divergences</summary>
                    <ul className="mt-2 space-y-1 text-xs font-mono">
                      {verifyResponse.divergences.map((d, idx) => (
                        <li key={idx}>
                          [{d.divergence_type}] expected={d.expected_hash.substring(0,16)} actual={d.actual_hash.substring(0,16)} — {d.context}
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
=======
>>>>>>> integration-branch
          </div>
        )}
      </CardContent>
    </Card>
<<<<<<< HEAD

    {/* Create Session Dialog */}
    <Dialog open={createOpen} onOpenChange={setCreateOpen}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>New Replay Session</DialogTitle>
        </DialogHeader>
        <div className="space-y-3">
          <div>
            <label className="font-medium text-sm mb-1">Tenant</label>
            <Input value={tenantId} readOnly />
          </div>
          <div>
            <label className="font-medium text-sm mb-1">CPID</label>
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
          <Button onClick={async () => {
            try {
              const ids = newBundleIds.split(',').map(s => s.trim()).filter(Boolean);
              const session = await apiClient.createReplaySession({ tenant_id: tenantId, cpid: newCpid, plan_id: newPlanId, telemetry_bundle_ids: ids });
              // Success shown in UI updates
              setCreateOpen(false);
              setNewCpid(''); setNewPlanId(''); setNewBundleIds('');
              const data = await apiClient.listReplaySessions(tenantId);
              setSessions(data);
            } catch (err) {
              const error = err instanceof Error ? err : new Error('Failed to create replay session');
              setReplayError(error);
            }
          }}>Create</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
    </div>
  );
}
=======
  );
}

>>>>>>> integration-branch
