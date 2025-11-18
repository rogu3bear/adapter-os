import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Badge } from './ui/badge';
import { Shield, Play, Hash } from 'lucide-react';
import apiClient from '../api/client';
import { ReplaySession } from '../api/types';
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
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center">
          <Play className="mr-2 h-5 w-5" />
          Replay Sessions
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
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
          </div>
        )}
      </CardContent>
    </Card>
  );
}

