import React from 'react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { Badge } from '@/components/ui/badge';
import RouterSummaryView from './RouterSummaryView';
import RouterTechnicalView from './RouterTechnicalView';
import type { ExtendedRouterDecision } from '@/api/types';

interface RouterDetailsModalProps {
  decision: ExtendedRouterDecision;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function RouterDetailsModal({
  decision,
  open,
  onOpenChange,
}: RouterDetailsModalProps) {
  const adapters = decision.selected_adapters || [];
  const scores = decision.scores || {};
  const candidates = decision.candidates || [];

  // Calculate average confidence from scores
  const calculateConfidence = () => {
    const selectedScores = adapters.map(id => scores[id] || 0);
    if (selectedScores.length === 0) return 0;
    return selectedScores.reduce((a, b) => a + b, 0) / selectedScores.length;
  };

  // Export audit data as JSON file
  const handleExportAudit = () => {
    const auditData = {
      timestamp: decision.timestamp,
      request_id: decision.request_id,
      selected_adapters: decision.selected_adapters,
      scores: decision.scores,
      k_value: decision.k_value,
      entropy: decision.entropy,
      tau: decision.tau,
      latency_ms: decision.latency_ms,
      candidates: decision.candidates,
      stack_hash: decision.stack_hash,
    };

    const blob = new Blob([JSON.stringify(auditData, null, 2)], {
      type: 'application/json',
    });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `router-audit-${decision.request_id || Date.now()}.json`;
    a.click();
    URL.revokeObjectURL(url);
  };

  // Transform data for RouterSummaryView
  const summaryData = {
    confidence: calculateConfidence() * 100, // Convert to 0-100 scale
    selectedAdapters: adapters.map(id => {
      const candidate = candidates.find((c: any) => c.adapter_id === id);
      return {
        id,
        name: id, // Use adapter ID as name for now
        score: scores[id] || candidate?.raw_score || 0,
        state: undefined, // Optional field
      };
    }),
    reasoning: undefined, // Can be added if available in future
    timestamp: decision.timestamp,
  };

  // Transform data for RouterTechnicalView
  const technicalData = {
    q15Gates: candidates
      .filter((c: any) => adapters.includes(c.adapter_id))
      .map((c: any, idx: number) => ({
        adapterId: c.adapter_id,
        adapterName: c.adapter_id,
        gateValue: c.gate_float || 0,
        rank: idx + 1,
      })),
    entropy: decision.entropy || 0,
    hashChain: {
      inputHash: decision.stack_hash || 'N/A',
      outputHash: decision.request_id || 'N/A',
      timestamp: decision.timestamp,
    },
    allCandidates: candidates.map((c: any) => ({
      id: c.adapter_id,
      name: c.adapter_id,
      score: c.raw_score || 0,
      selected: c.selected || adapters.includes(c.adapter_id),
    })),
    rawDecision: decision as unknown as Record<string, unknown>,
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-3xl max-h-[85vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>Router Decision Details</DialogTitle>
          <DialogDescription>
            Adapters selected by the K-sparse router for this message
          </DialogDescription>
        </DialogHeader>

        <Tabs defaultValue="summary" className="mt-4">
          <TabsList className="grid w-full grid-cols-2">
            <TabsTrigger value="summary">Summary</TabsTrigger>
            <TabsTrigger value="technical">Technical Proof</TabsTrigger>
          </TabsList>

          <TabsContent value="summary" className="mt-4">
            <RouterSummaryView
              decision={summaryData}
              onExportAudit={handleExportAudit}
            />
          </TabsContent>

          <TabsContent value="technical" className="mt-4">
            <RouterTechnicalView data={technicalData} />
          </TabsContent>
        </Tabs>

        {/* Request ID footer (preserved from original) */}
        {decision.request_id && (
          <div className="pt-2 mt-4 border-t">
            <p className="text-xs text-muted-foreground">
              Request ID: <span className="font-mono">{decision.request_id}</span>
            </p>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}

