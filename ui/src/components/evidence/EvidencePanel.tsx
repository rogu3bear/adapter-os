import React from 'react';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { EvidenceStatusBadge } from '@/components/evidence/EvidenceStatusBadge';
import { useEvidenceApi } from '@/hooks/useEvidenceApi';
import type { Evidence } from '@/api/document-types';
import { toast } from 'sonner';
import { Download, PlusCircle, RefreshCw, AlertCircle } from 'lucide-react';

interface EvidencePanelProps {
  traceId: string;
  tenantId?: string;
  receiptDigest?: string;
}

const EVIDENCE_DESCRIPTION =
  'Compliance-ready bundles for this inference run. Track build status and download when ready.';

export function EvidencePanel({ traceId, tenantId, receiptDigest }: EvidencePanelProps) {
  const {
    evidence,
    createEvidence,
    isCreating,
    downloadEvidence,
    isDownloading,
    invalidateEvidence,
  } = useEvidenceApi(
    traceId
      ? {
          trace_id: traceId,
          tenant_id: tenantId,
        }
      : undefined,
    { enabled: Boolean(traceId) }
  );

  const entries = evidence.data ?? [];

  const handleCreateEvidence = async () => {
    try {
      await createEvidence({
        trace_id: traceId,
        tenant_id: tenantId,
        evidence_type: 'audit',
        reference: receiptDigest || traceId,
        confidence: 'high',
        description: 'Inference evidence bundle',
        metadata_json: JSON.stringify({
          trace_id: traceId,
          receipt_digest: receiptDigest,
          source: 'inference_playground',
        }),
      });
      toast.success('Evidence creation queued');
    } catch (error) {
      const code = (error as any)?.code || (error as any)?.status;
      const message = error instanceof Error ? error.message : 'Failed to create evidence';
      toast.error(code ? `Evidence creation failed (${code})` : 'Evidence creation failed', {
        description: message,
      });
    }
  };

  const handleDownload = async (entry: Evidence) => {
    try {
      await downloadEvidence({
        evidenceId: entry.id,
        filename: entry.file_name || `evidence-${entry.id}.json`,
      });
    } catch {
      // Error handled in mutation onError
    }
  };

  return (
    <Card data-testid="evidence-panel">
      <CardHeader className="pb-3">
        <div className="flex items-start justify-between gap-2">
          <div>
            <CardTitle className="text-base">Evidence</CardTitle>
            <CardDescription>{EVIDENCE_DESCRIPTION}</CardDescription>
          </div>
          <div className="flex gap-2">
            <Button
              size="sm"
              variant="outline"
              onClick={() => invalidateEvidence()}
              disabled={evidence.isFetching}
              className="gap-2"
            >
              <RefreshCw className={`h-4 w-4 ${evidence.isFetching ? 'animate-spin' : ''}`} />
              Refresh
            </Button>
            <Button
              size="sm"
              onClick={handleCreateEvidence}
              disabled={!traceId || isCreating}
              className="gap-2"
            >
              <PlusCircle className={`h-4 w-4 ${isCreating ? 'animate-spin' : ''}`} />
              Create evidence
            </Button>
          </div>
        </div>
      </CardHeader>
      <CardContent className="space-y-3">
        {evidence.isLoading ? (
          <div className="text-sm text-muted-foreground flex items-center gap-2">
            <RefreshCw className="h-4 w-4 animate-spin" />
            Loading evidence...
          </div>
        ) : entries.length === 0 ? (
          <div className="text-sm text-muted-foreground flex items-center gap-2">
            <AlertCircle className="h-4 w-4" />
            No evidence found for this trace yet.
          </div>
        ) : (
          <div className="space-y-2">
            {entries.map((entry) => (
              <div
                key={entry.id}
                className="border rounded-md p-3 flex flex-col gap-2 md:flex-row md:items-center md:justify-between"
              >
                <div className="space-y-1">
                  <div className="flex items-center gap-2">
                    <EvidenceStatusBadge status={entry.status} />
                    <Badge variant="outline">{entry.evidence_type}</Badge>
                    {entry.error_code && (
                      <Badge variant="destructive" className="text-xs">
                        {entry.error_code}
                      </Badge>
                    )}
                  </div>
                  <div className="text-sm font-medium">{entry.reference}</div>
                  <div className="text-xs text-muted-foreground">
                    {entry.description || 'No description'}
                  </div>
                  <div className="text-xs text-muted-foreground">
                    Created {new Date(entry.created_at).toLocaleString()}
                    {entry.updated_at ? ` · Updated ${new Date(entry.updated_at).toLocaleString()}` : ''}
                  </div>
                </div>
                <div className="flex gap-2 items-center">
                  <Button
                    size="sm"
                    variant="outline"
                    className="gap-2"
                    onClick={() => handleDownload(entry)}
                    disabled={isDownloading}
                  >
                    <Download className={`h-4 w-4 ${isDownloading ? 'animate-spin' : ''}`} />
                    Download
                  </Button>
                  {entry.bundle_size_bytes != null && (
                    <span className="text-xs text-muted-foreground">
                      {(entry.bundle_size_bytes / 1024).toFixed(1)} KB
                    </span>
                  )}
                </div>
              </div>
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  );
}

