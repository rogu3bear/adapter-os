import React, { useMemo, useState } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import FeatureLayout from '@/layout/FeatureLayout';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { replayTabOrder, replayTabToPath, ReplayTab, resolveReplayTab } from '@/pages/Replay/tabs';
import { ReplayPanel } from '@/components/ReplayPanel';
import apiClient from '@/api/client';
import type { ReplaySession } from '@/api/types';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Input } from '@/components/ui/input';
import { useToast } from '@/hooks/use-toast';

export default function ReplayShell() {
  const location = useLocation();
  const navigate = useNavigate();
  const { toast } = useToast();
  const [selectedSession, setSelectedSession] = useState<ReplaySession | null>(null);
  const [compareId, setCompareId] = useState<string>('');

  const activeTab: ReplayTab = useMemo(
    () => resolveReplayTab(location.pathname, location.hash),
    [location.hash, location.pathname],
  );

  const tabPath = (tab: ReplayTab) => replayTabToPath(tab);

  const { data: sessionDetail } = useQuery({
    queryKey: ['replay-session', selectedSession?.id],
    queryFn: () => apiClient.getReplaySession(selectedSession?.id as string),
    enabled: Boolean(selectedSession?.id),
  });

  const { data: allSessions } = useQuery({
    queryKey: ['replay-sessions'],
    queryFn: () => apiClient.listReplaySessions(),
  });

  return (
    <FeatureLayout
      title="Replay"
      description="Deterministic replay and evidence"
      maxWidth="xl"
    >
      <Tabs
        value={activeTab}
        onValueChange={(value: string) => {
          const tab = value as ReplayTab;
          const next = tabPath(tab);
          const nextLocation = next.split('#')[0];
          if (nextLocation !== location.pathname || location.hash !== '') {
            navigate(next);
          }
        }}
      >
        <TabsList className="w-full grid grid-cols-2 md:grid-cols-5">
          {replayTabOrder.map(tab => (
            <TabsTrigger key={tab} value={tab}>
              {tab === 'runs' && 'Runs'}
              {tab === 'decision-trace' && 'Decision Trace'}
              {tab === 'evidence' && 'Evidence'}
              {tab === 'compare' && 'Compare'}
              {tab === 'export' && 'Export'}
            </TabsTrigger>
          ))}
        </TabsList>

        <TabsContent value="runs" className="mt-6">
          <ReplayPanel
            onSessionSelect={(session) => {
              setSelectedSession(session);
              if (session) {
                toast({ title: 'Session selected', description: session.cpid });
              }
            }}
          />
        </TabsContent>
        <TabsContent value="decision-trace" className="mt-6">
          <DecisionTraceTab session={sessionDetail ?? selectedSession ?? null} />
        </TabsContent>
        <TabsContent value="evidence" className="mt-6">
          <EvidenceTab session={sessionDetail ?? selectedSession ?? null} />
        </TabsContent>
        <TabsContent value="compare" className="mt-6">
          <CompareTab
            session={sessionDetail ?? selectedSession ?? null}
            sessions={allSessions ?? []}
            compareId={compareId}
            onCompareIdChange={setCompareId}
          />
        </TabsContent>
        <TabsContent value="export" className="mt-6">
          <ExportTab session={sessionDetail ?? selectedSession ?? null} />
        </TabsContent>
      </Tabs>
    </FeatureLayout>
  );
}

function DecisionTraceTab({ session }: { session: ReplaySession | null }) {
  if (!session) {
    return <div className="text-sm text-muted-foreground">Select a replay session from Runs.</div>;
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Routing decision trace</CardTitle>
      </CardHeader>
      <CardContent className="space-y-2 text-sm">
        <div className="grid grid-cols-2 gap-3">
          <div>
            <div className="text-muted-foreground">Policy ID</div>
            <div className="font-mono">{session.cpid}</div>
          </div>
          <div>
            <div className="text-muted-foreground">Plan ID</div>
            <div className="font-mono">{session.plan_id}</div>
          </div>
          <div>
            <div className="text-muted-foreground">Manifest hash</div>
            <div className="font-mono">{session.manifest_hash_b3}</div>
          </div>
          <div>
            <div className="text-muted-foreground">Policy hash</div>
            <div className="font-mono">{session.policy_hash_b3}</div>
          </div>
          {session.kernel_hash_b3 ? (
            <div>
              <div className="text-muted-foreground">Kernel hash</div>
              <div className="font-mono">{session.kernel_hash_b3}</div>
            </div>
          ) : null}
        </div>
        {session.config ? (
          <div className="space-y-1">
            <div className="text-muted-foreground">Sampling params</div>
            <div className="font-mono text-xs break-all">{JSON.stringify(session.config)}</div>
          </div>
        ) : null}
      </CardContent>
    </Card>
  );
}

function EvidenceTab({ session }: { session: ReplaySession | null }) {
  if (!session) {
    return <div className="text-sm text-muted-foreground">Select a replay session from Runs.</div>;
  }

  if (!session.telemetry_bundle_ids || session.telemetry_bundle_ids.length === 0) {
    return <div className="text-sm text-muted-foreground">No telemetry bundles recorded for this session.</div>;
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Evidence bundles</CardTitle>
      </CardHeader>
      <CardContent>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Bundle ID</TableHead>
              <TableHead>Context</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {session.telemetry_bundle_ids.map((bundleId) => (
              <TableRow key={bundleId}>
                <TableCell className="font-mono text-xs">{bundleId}</TableCell>
                <TableCell className="text-sm text-muted-foreground">Telemetry snapshot used for replay</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  );
}

function CompareTab({
  session,
  sessions,
  compareId,
  onCompareIdChange,
}: {
  session: ReplaySession | null;
  sessions: ReplaySession[];
  compareId: string;
  onCompareIdChange: (id: string) => void;
}) {
  const [compareSession, setCompareSession] = useState<ReplaySession | null>(null);

  useQuery({
    queryKey: ['replay-compare', compareId],
    queryFn: () => apiClient.getReplaySession(compareId),
    enabled: Boolean(compareId),
    onSuccess: (data) => setCompareSession(data),
  });

  if (!session) {
    return <div className="text-sm text-muted-foreground">Select a replay session from Runs.</div>;
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Compare replay sessions</CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
          <div>
            <div className="text-muted-foreground text-sm">Baseline session</div>
            <div className="font-mono">{session.id}</div>
          </div>
          <div className="space-y-1">
            <div className="text-muted-foreground text-sm">Compare against</div>
            <Input
              list="replay-session-options"
              placeholder="Session ID"
              value={compareId}
              onChange={(e) => onCompareIdChange(e.target.value)}
            />
            <datalist id="replay-session-options">
              {sessions.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.id}
                </option>
              ))}
            </datalist>
          </div>
        </div>
        {compareSession ? (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Field</TableHead>
                <TableHead>Baseline</TableHead>
                <TableHead>Compare</TableHead>
                <TableHead>Status</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {[
                { label: 'Manifest hash', a: session.manifest_hash_b3, b: compareSession.manifest_hash_b3 },
                { label: 'Policy hash', a: session.policy_hash_b3, b: compareSession.policy_hash_b3 },
                { label: 'Kernel hash', a: session.kernel_hash_b3, b: compareSession.kernel_hash_b3 },
              ].map((row) => (
                <TableRow key={row.label}>
                  <TableCell>{row.label}</TableCell>
                  <TableCell className="font-mono text-xs">{row.a ?? '—'}</TableCell>
                  <TableCell className="font-mono text-xs">{row.b ?? '—'}</TableCell>
                  <TableCell>
                    <Badge variant={row.a === row.b ? 'outline' : 'secondary'}>
                      {row.a === row.b ? 'Match' : 'Diff'}
                    </Badge>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        ) : (
          <div className="text-sm text-muted-foreground">Choose a session to compare.</div>
        )}
      </CardContent>
    </Card>
  );
}

function ExportTab({ session }: { session: ReplaySession | null }) {
  if (!session) {
    return <div className="text-sm text-muted-foreground">Select a replay session from Runs.</div>;
  }

  const exportJson = () => {
    const blob = new Blob([JSON.stringify(session, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = `replay-${session.id}.json`;
    link.click();
    URL.revokeObjectURL(url);
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>Export replay</CardTitle>
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="text-sm text-muted-foreground">
          Export the selected session for external analysis or debug bundles.
        </div>
        <div className="flex gap-2">
          <Button onClick={exportJson}>Download JSON</Button>
        </div>
      </CardContent>
    </Card>
  );
}

