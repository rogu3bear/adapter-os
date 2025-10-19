import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from './ui/dialog';
import { Input } from './ui/input';
import { Button } from './ui/button';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Badge } from './ui/badge';
import { Shield, Play, Hash } from 'lucide-react';
import apiClient from '../api/client';
import { ReplaySession, ReplayVerificationResponse } from '../api/types';
import { useTimestamp } from '../hooks/useTimestamp';
import { toast } from 'sonner';

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
  const [verifying, setVerifying] = useState(false);
  const [verifyResponse, setVerifyResponse] = useState<ReplayVerificationResponse | null>(null);
  const [createOpen, setCreateOpen] = useState(false);
  const [newCpid, setNewCpid] = useState('');
  const [newPlanId, setNewPlanId] = useState('');
  const [newBundleIds, setNewBundleIds] = useState('');

  useEffect(() => {
    const fetchSessions = async () => {
      try {
        const data = await apiClient.listReplaySessions(tenantId);
        setSessions(data);
      } catch (err) {
        toast.error('Failed to load replay sessions');
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
      setVerifyResponse(result);
      
      if (result.signature_valid && result.hash_chain_valid) {
        toast.success('Replay session verified successfully');
      } else {
        toast.error(`Verification failed: ${result.divergences.length} divergences found`);
      }
    } catch (err) {
      toast.error('Verification error');
    } finally {
      setVerifying(false);
    }
  };

  const currentSession = sessions.find(s => s.id === selectedSession);

  return (
    <div>
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center">
          <Play className="mr-2 h-5 w-5" />
          Replay Sessions
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="flex justify-end">
          <Button variant="outline" onClick={() => setCreateOpen(true)}>New Session</Button>
        </div>
        <Select value={selectedSession || ''} onValueChange={handleSessionSelect}>
          <SelectTrigger>
            <SelectValue placeholder="Select replay session" />
          </SelectTrigger>
          <SelectContent>
            {sessions.map(session => (
              <SelectItem key={session.id} value={session.id}>
                {session.cpid} @ {useTimestamp(session.snapshot_at)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>

        {currentSession && (
          <div className="space-y-3 border rounded p-3">
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

            <Button 
              onClick={handleVerify} 
              disabled={verifying}
              className="w-full"
              variant="outline"
            >
              <Shield className="mr-2 h-4 w-4" />
              {verifying ? 'Verifying...' : 'Verify Cryptographic Chain'}
            </Button>

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
            <label className="form-label">Tenant</label>
            <Input value={tenantId} readOnly />
          </div>
          <div>
            <label className="form-label">CPID</label>
            <Input value={newCpid} onChange={(e) => setNewCpid(e.target.value)} placeholder="cp_..." />
          </div>
          <div>
            <label className="form-label">Plan ID</label>
            <Input value={newPlanId} onChange={(e) => setNewPlanId(e.target.value)} placeholder="plan_..." />
          </div>
          <div>
            <label className="form-label">Telemetry Bundle IDs (comma-separated)</label>
            <Input value={newBundleIds} onChange={(e) => setNewBundleIds(e.target.value)} placeholder="b1,b2" />
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => setCreateOpen(false)}>Cancel</Button>
          <Button onClick={async () => {
            try {
              const ids = newBundleIds.split(',').map(s => s.trim()).filter(Boolean);
              const session = await apiClient.createReplaySession({ tenant_id: tenantId, cpid: newCpid, plan_id: newPlanId, telemetry_bundle_ids: ids });
              toast.success('Replay session created');
              setCreateOpen(false);
              setNewCpid(''); setNewPlanId(''); setNewBundleIds('');
              const data = await apiClient.listReplaySessions(tenantId);
              setSessions(data);
            } catch (err) {
              toast.error('Failed to create replay session');
            }
          }}>Create</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
    </div>
  );
}
