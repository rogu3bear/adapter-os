import React, { useEffect, useMemo, useState } from 'react';
import FeatureLayout from '@/layout/FeatureLayout';
import { useTenant } from '@/providers/FeatureProviders';
import { useEvidenceApi } from '@/hooks/useEvidenceApi';
import type { CreateEvidenceRequest, Evidence, EvidenceStatus, ListEvidenceQuery } from '@/api/document-types';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { EvidenceStatusBadge } from '@/components/evidence/EvidenceStatusBadge';
import { Badge } from '@/components/ui/badge';
import { toast } from 'sonner';
import { Download, Filter, PlusCircle } from 'lucide-react';
import { Separator } from '@/components/ui/separator';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';

type TargetType = 'trace' | 'message';

const EVIDENCE_TYPES: CreateEvidenceRequest['evidence_type'][] = [
  'audit',
  'doc',
  'ticket',
  'commit',
  'policy_approval',
  'data_agreement',
  'review',
  'other',
];

export default function EvidencePage() {
  const { selectedTenant } = useTenant();
  const [targetType, setTargetType] = useState<TargetType>('trace');
  const [targetId, setTargetId] = useState('');
  const [evidenceType, setEvidenceType] = useState<CreateEvidenceRequest['evidence_type']>('audit');
  const [reference, setReference] = useState('');
  const [description, setDescription] = useState('');
  const [statusFilter, setStatusFilter] = useState<EvidenceStatus | 'all'>('all');

  const [filters, setFilters] = useState<ListEvidenceQuery>(() =>
    selectedTenant ? { tenant_id: selectedTenant } : {}
  );

  const mergedFilters = useMemo(
    () => ({
      ...filters,
      ...(statusFilter === 'all' ? {} : { status: statusFilter }),
    }),
    [filters, statusFilter]
  );

  useEffect(() => {
    setFilters((prev) => ({
      ...prev,
      tenant_id: selectedTenant,
    }));
  }, [selectedTenant]);

  const {
    evidence,
    createEvidence,
    isCreating,
    downloadEvidence,
    isDownloading,
    invalidateEvidence,
  } = useEvidenceApi(mergedFilters, { enabled: true });

  const evidenceItems = useMemo(() => {
    const data = evidence.data ?? [];
    if (statusFilter === 'all') return data;
    return data.filter((item) => item.status === statusFilter);
  }, [evidence.data, statusFilter]);

  const handleCreate = async () => {
    if (!targetId.trim()) {
      toast.error('Provide a trace or message ID');
      return;
    }

    const request: CreateEvidenceRequest = {
      tenant_id: selectedTenant,
      trace_id: targetType === 'trace' ? targetId.trim() : undefined,
      message_id: targetType === 'message' ? targetId.trim() : undefined,
      evidence_type: evidenceType,
      reference: reference.trim() || targetId.trim(),
      description: description.trim() || undefined,
      confidence: 'medium',
      metadata_json: JSON.stringify({
        source: 'evidence_center',
        target_type: targetType,
      }),
    };

    try {
      await createEvidence(request);
      toast.success('Evidence request submitted');
      setReference('');
      setDescription('');
      setTargetId('');
    } catch (error) {
      const code = (error as any)?.code || (error as any)?.status;
      const message = error instanceof Error ? error.message : 'Failed to create evidence';
      toast.error(code ? `Create failed (${code})` : 'Create failed', { description: message });
    }
  };

  const handleDownload = async (evidenceItem: Evidence) => {
    try {
      await downloadEvidence({
        evidenceId: evidenceItem.id,
        filename: evidenceItem.file_name || `evidence-${evidenceItem.id}.json`,
      });
    } catch {
      // handled by mutation onError
    }
  };

  return (
    <FeatureLayout
      title="Evidence"
      description="Export and track evidence bundles"
      brief="Compliance workflow for inference and chat evidence."
    >
      <div className="space-y-6">
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <PlusCircle className="h-5 w-5" />
              Create evidence
            </CardTitle>
            <CardDescription>
              Generate evidence bundles from an inference trace or chat message for the selected tenant.
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label>Target</Label>
                <div className="flex gap-2">
                  <Select value={targetType} onValueChange={(val: TargetType) => setTargetType(val)}>
                    <SelectTrigger className="w-32">
                      <SelectValue placeholder="Target type" />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="trace">Trace</SelectItem>
                      <SelectItem value="message">Message</SelectItem>
                    </SelectContent>
                  </Select>
                  <Input
                    placeholder={targetType === 'trace' ? 'trace-id' : 'message-id'}
                    value={targetId}
                    onChange={(e) => setTargetId(e.target.value)}
                  />
                </div>
              </div>
              <div className="space-y-2">
                <Label>Evidence type</Label>
                <Select value={evidenceType} onValueChange={(val: CreateEvidenceRequest['evidence_type']) => setEvidenceType(val)}>
                  <SelectTrigger>
                    <SelectValue placeholder="Select type" />
                  </SelectTrigger>
                  <SelectContent>
                    {EVIDENCE_TYPES.map((type) => (
                      <SelectItem key={type} value={type}>
                        {type}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <div className="space-y-2">
                <Label>Reference</Label>
                <Input
                  placeholder="Ticket, commit, or link"
                  value={reference}
                  onChange={(e) => setReference(e.target.value)}
                />
              </div>
              <div className="space-y-2">
                <Label>Description</Label>
                <Input
                  placeholder="Optional description"
                  value={description}
                  onChange={(e) => setDescription(e.target.value)}
                />
              </div>
            </div>
            <div className="flex items-center justify-between">
              <div className="text-sm text-muted-foreground">
                Tenant scope: {selectedTenant || 'None selected'}
              </div>
              <Button onClick={handleCreate} disabled={isCreating || !selectedTenant} className="gap-2">
                <PlusCircle className={`h-4 w-4 ${isCreating ? 'animate-spin' : ''}`} />
                Create evidence
              </Button>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Filter className="h-5 w-5" />
              Evidence list
            </CardTitle>
            <CardDescription>Verify availability, status, and download bundles.</CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="flex flex-wrap gap-3 items-center">
              <div className="space-y-1">
                <Label>Status</Label>
                <Select value={statusFilter} onValueChange={(val: EvidenceStatus | 'all') => setStatusFilter(val)}>
                  <SelectTrigger className="w-40">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="all">All</SelectItem>
                    <SelectItem value="queued">Queued</SelectItem>
                    <SelectItem value="building">Building</SelectItem>
                    <SelectItem value="ready">Ready</SelectItem>
                    <SelectItem value="failed">Failed</SelectItem>
                  </SelectContent>
                </Select>
              </div>
              <Button
                variant="outline"
                size="sm"
                onClick={() => invalidateEvidence()}
                className="mt-6"
              >
                Refresh
              </Button>
            </div>

            <Separator />

            <div className="rounded-md border">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Reference</TableHead>
                    <TableHead>Status</TableHead>
                    <TableHead>Type</TableHead>
                    <TableHead>Target</TableHead>
                    <TableHead>Created</TableHead>
                    <TableHead className="text-right">Actions</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {evidence.isLoading ? (
                    <TableRow>
                      <TableCell colSpan={6} className="text-sm text-muted-foreground">
                        Loading evidence...
                      </TableCell>
                    </TableRow>
                  ) : evidenceItems.length === 0 ? (
                    <TableRow>
                      <TableCell colSpan={6} className="text-sm text-muted-foreground">
                        No evidence found.
                      </TableCell>
                    </TableRow>
                  ) : (
                    evidenceItems.map((item) => (
                      <TableRow key={item.id}>
                        <TableCell className="font-medium">{item.reference}</TableCell>
                        <TableCell>
                          <div className="flex items-center gap-2">
                            <EvidenceStatusBadge status={item.status} />
                            {item.error_code && (
                              <Badge variant="destructive" className="text-xs">
                                {item.error_code}
                              </Badge>
                            )}
                          </div>
                        </TableCell>
                        <TableCell>
                          <Badge variant="outline">{item.evidence_type}</Badge>
                        </TableCell>
                        <TableCell className="text-xs text-muted-foreground">
                          {item.trace_id || item.message_id || '—'}
                        </TableCell>
                        <TableCell className="text-xs text-muted-foreground">
                          {new Date(item.created_at).toLocaleString()}
                        </TableCell>
                        <TableCell className="text-right">
                          <Button
                            size="sm"
                            variant="outline"
                            onClick={() => handleDownload(item)}
                            disabled={isDownloading}
                            className="gap-2"
                          >
                            <Download className={`h-4 w-4 ${isDownloading ? 'animate-spin' : ''}`} />
                            Download
                          </Button>
                        </TableCell>
                      </TableRow>
                    ))
                  )}
                </TableBody>
              </Table>
            </div>
          </CardContent>
        </Card>
      </div>
    </FeatureLayout>
  );
}

