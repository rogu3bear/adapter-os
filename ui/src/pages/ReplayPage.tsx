import { useState } from 'react';
import { Download, FileCheck2, ShieldCheck, UploadCloud } from 'lucide-react';
import { Link, useNavigate } from 'react-router-dom';
import FeatureLayout from '@/layout/FeatureLayout';
import { DensityProvider } from '@/contexts/DensityContext';
import { ReplayPanel } from '@/components/ReplayPanel';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { PageHeader as IaPageHeader } from '@/components/shared/PageHeader';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { useTenant } from '@/providers/FeatureProviders';
import { useRBAC } from '@/hooks/security/useRBAC';
import { useReplayTabRouter } from '@/hooks/navigation/useTabRouter';
import apiClient from '@/api/client';
import { toast } from 'sonner';
import { ReceiptVerificationResult, ReceiptReasonCode } from '@/api/api-types';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { ProofBar } from '@/components/receipts/ProofBar';

const digestRows = (result: ReceiptVerificationResult) => [
  result.context_digest,
  result.run_head_hash,
  result.output_digest,
  result.receipt_digest,
];

function downloadReport(result: ReceiptVerificationResult) {
  const dataStr = JSON.stringify(result, null, 2);
  const blob = new Blob([dataStr], { type: 'application/json' });
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = `replay-verification-${result.trace_id}.json`;
  document.body.appendChild(link);
  link.click();
  document.body.removeChild(link);
  URL.revokeObjectURL(url);
}

function formatDigest(hex: string) {
  if (!hex) return '—';
  return hex.length > 16 ? `${hex.slice(0, 16)}...` : hex;
}

function renderReasonBadge(reason: ReceiptReasonCode) {
  const variant = reason === 'TRACE_TAMPER' || reason === 'MISSING_RECEIPT' ? 'destructive' : 'secondary';
  return (
    <Badge key={reason} variant={variant} className="font-mono text-[11px]">
      {reason}
    </Badge>
  );
}

export default function ReplayPage() {
  const { selectedTenant } = useTenant();
  const { can } = useRBAC();
  const navigate = useNavigate();
  const { activeTab, setActiveTab, availableTabs, getTabPath } = useReplayTabRouter();
  const [traceId, setTraceId] = useState('');
  const [bundleFile, setBundleFile] = useState<File | null>(null);
  const [verification, setVerification] = useState<ReceiptVerificationResult | null>(null);
  const [verifyingTrace, setVerifyingTrace] = useState(false);
  const [verifyingBundle, setVerifyingBundle] = useState(false);

  const canVerify = can('audit:view');

  const handleTraceVerify = async () => {
    if (!traceId.trim()) {
      toast.error('Enter a trace_id to verify');
      return;
    }
    if (!canVerify) {
      toast.error('Permission denied: audit:view required');
      return;
    }
    setVerifyingTrace(true);
    try {
      const result = await apiClient.verifyTraceReceipt(traceId.trim());
      setVerification(result);
      toast[result.pass ? 'success' : 'error'](
        result.pass ? 'Trace verification passed' : 'Trace verification failed'
      );
    } catch (err) {
      toast.error('Trace verification error');
    } finally {
      setVerifyingTrace(false);
    }
  };

  const handleBundleVerify = async () => {
    if (!bundleFile) {
      toast.error('Upload a bundle to verify');
      return;
    }
    if (!canVerify) {
      toast.error('Permission denied: audit:view required');
      return;
    }
    setVerifyingBundle(true);
    try {
      const result = await apiClient.verifyEvidenceBundle(bundleFile);
      setVerification(result);
      toast[result.pass ? 'success' : 'error'](
        result.pass ? 'Bundle verification passed' : 'Bundle verification failed'
      );
    } catch (err) {
      toast.error('Bundle verification error');
    } finally {
      setVerifyingBundle(false);
    }
  };

  const handleOpenTrace = (id?: string | null) => {
    const target = id?.trim();
    if (!target) {
      toast.error('Trace ID is unavailable');
      return;
    }
    navigate(`/telemetry?tab=viewer&requestId=${encodeURIComponent(target)}`);
  };

  return (
    <DensityProvider pageKey="replay">
      <FeatureLayout
        title="Replay"
        description="Deterministic verification"
        brief="Replay and verify deterministic execution sessions"
        customHeader={
          <IaPageHeader
            cluster="Verify"
            title="Replay"
            description="Deterministic verification"
            brief="Replay and verify deterministic execution sessions"
          />
        }
      >
        <SectionErrorBoundary sectionName="Replay">
          <Tabs value={activeTab} onValueChange={(value) => setActiveTab(value as typeof activeTab)}>
            <TabsList className="w-full grid grid-cols-3 md:grid-cols-5">
              {availableTabs.map((tab) => (
                <TabsTrigger key={tab.id} value={tab.id} asChild>
                  <Link to={getTabPath(tab.id)}>{tab.label}</Link>
                </TabsTrigger>
              ))}
            </TabsList>

            <TabsContent value="runs" className="mt-6">
              <ReplayPanel tenantId={selectedTenant} onSessionSelect={() => {}} />
            </TabsContent>

            <TabsContent value="decision-trace" className="mt-6">
              <VerificationTab
                traceId={traceId}
                setTraceId={setTraceId}
                bundleFile={bundleFile}
                setBundleFile={setBundleFile}
                verification={verification}
                verifyingTrace={verifyingTrace}
                verifyingBundle={verifyingBundle}
                canVerify={canVerify}
                handleTraceVerify={handleTraceVerify}
                handleBundleVerify={handleBundleVerify}
                handleOpenTrace={handleOpenTrace}
              />
            </TabsContent>

            <TabsContent value="evidence" className="mt-6">
              <div className="text-sm text-muted-foreground">Evidence browser (coming soon)</div>
            </TabsContent>

            <TabsContent value="compare" className="mt-6">
              <div className="text-sm text-muted-foreground">Run comparison view (coming soon)</div>
            </TabsContent>

            <TabsContent value="export" className="mt-6">
              <div className="text-sm text-muted-foreground">Export tools (coming soon)</div>
            </TabsContent>
          </Tabs>
        </SectionErrorBoundary>
      </FeatureLayout>
    </DensityProvider>
  );
}

interface VerificationTabProps {
  traceId: string;
  setTraceId: (id: string) => void;
  bundleFile: File | null;
  setBundleFile: (file: File | null) => void;
  verification: ReceiptVerificationResult | null;
  verifyingTrace: boolean;
  verifyingBundle: boolean;
  canVerify: boolean;
  handleTraceVerify: () => void;
  handleBundleVerify: () => void;
  handleOpenTrace: (id?: string | null) => void;
}

function VerificationTab({
  traceId,
  setTraceId,
  bundleFile,
  setBundleFile,
  verification,
  verifyingTrace,
  verifyingBundle,
  canVerify,
  handleTraceVerify,
  handleBundleVerify,
  handleOpenTrace,
}: VerificationTabProps) {
  return (
    <div className="space-y-6">
      <div className="grid gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <ShieldCheck className="h-4 w-4" />
              Verify by trace_id
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-3">
            <div className="space-y-1">
              <label className="text-sm font-medium">Trace ID</label>
              <Input
                placeholder="trace_..."
                value={traceId}
                onChange={(e) => setTraceId(e.target.value)}
              />
            </div>
            <Button
              onClick={handleTraceVerify}
              disabled={verifyingTrace || !canVerify}
              className="w-full"
              variant="outline"
            >
              {verifyingTrace ? 'Verifying...' : 'Verify Trace Receipt'}
            </Button>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <UploadCloud className="h-4 w-4" />
              Verify evidence bundle
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-3">
            <div className="space-y-1">
              <label className="text-sm font-medium">Upload bundle (.json or .zip)</label>
              <Input
                type="file"
                accept=".json,.zip,.ndjson,application/zip,application/json"
                onChange={(e) => setBundleFile(e.target.files?.[0] ?? null)}
              />
              {bundleFile && <div className="text-xs text-muted-foreground">{bundleFile.name}</div>}
            </div>
            <Button
              onClick={handleBundleVerify}
              disabled={verifyingBundle || !canVerify}
              className="w-full"
              variant="outline"
            >
              {verifyingBundle ? 'Verifying...' : 'Verify Evidence Bundle'}
            </Button>
          </CardContent>
        </Card>
      </div>

      {verification && (
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <FileCheck2 className="h-4 w-4" />
              Verification result
              <Badge variant={verification.pass ? 'default' : 'destructive'}>
                {verification.pass ? 'PASS' : 'FAIL'}
              </Badge>
              <Badge variant="outline">{verification.source}</Badge>
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <ProofBar
              traceId={verification.trace_id}
              receiptDigest={verification.receipt_digest?.computed_hex}
              backendUsed={undefined}
              determinismMode={undefined}
              evidenceAvailable={false}
              onOpenTrace={() => handleOpenTrace(verification.trace_id)}
            />
            <div className="flex flex-wrap gap-2">
              {verification.reasons.length === 0 && (
                <Badge variant="secondary">No mismatches</Badge>
              )}
              {verification.reasons.map(renderReasonBadge)}
            </div>
            {verification.mismatched_token !== undefined && verification.mismatched_token !== null && (
              <div className="text-sm text-muted-foreground">
                First mismatched token: {verification.mismatched_token}
              </div>
            )}

            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Field</TableHead>
                  <TableHead>Expected</TableHead>
                  <TableHead>Computed</TableHead>
                  <TableHead>Status</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {digestRows(verification).map((diff) => (
                  <TableRow key={diff.field}>
                    <TableCell className="font-mono text-xs">{diff.field}</TableCell>
                    <TableCell className="font-mono text-xs">{formatDigest(diff.expected_hex)}</TableCell>
                    <TableCell className="font-mono text-xs">{formatDigest(diff.computed_hex)}</TableCell>
                    <TableCell>
                      <Badge variant={diff.matches ? 'secondary' : 'destructive'}>
                        {diff.matches ? 'match' : 'mismatch'}
                      </Badge>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>

            <div className="flex items-center justify-between">
              <div className="text-xs text-muted-foreground">
                Verified at {verification.verified_at}
              </div>
              <Button
                size="sm"
                variant="outline"
                className="flex items-center gap-2"
                onClick={() => downloadReport(verification)}
              >
                <Download className="h-3 w-3" />
                Download report JSON
              </Button>
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
