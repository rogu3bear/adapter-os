import React from 'react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Badge } from '@/components/ui/badge';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
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

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl max-h-[80vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>Router Decision Details</DialogTitle>
          <DialogDescription>
            Adapters selected by the K-sparse router for this message
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4 mt-4">
          {/* Summary */}
          <div className="grid grid-cols-2 gap-4">
            <div>
              <p className="text-sm font-medium text-muted-foreground">K-sparse Value</p>
              <p className="text-lg font-semibold">{decision.k_value || adapters.length}</p>
            </div>
            {decision.entropy !== undefined && (
              <div>
                <p className="text-sm font-medium text-muted-foreground">Entropy</p>
                <p className="text-lg font-semibold">{decision.entropy.toFixed(4)}</p>
              </div>
            )}
            {decision.tau !== undefined && (
              <div>
                <p className="text-sm font-medium text-muted-foreground">Temperature (τ)</p>
                <p className="text-lg font-semibold">{decision.tau.toFixed(4)}</p>
              </div>
            )}
            {decision.latency_ms !== undefined && (
              <div>
                <p className="text-sm font-medium text-muted-foreground">Router Latency</p>
                <p className="text-lg font-semibold">{decision.latency_ms.toFixed(2)}ms</p>
              </div>
            )}
          </div>

          {/* Selected Adapters */}
          <div>
            <h3 className="text-sm font-semibold mb-2">Selected Adapters</h3>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Adapter ID</TableHead>
                  <TableHead>Score</TableHead>
                  {candidates.length > 0 && candidates[0]?.gate_q15 !== undefined && (
                    <TableHead>Gate (Q15)</TableHead>
                  )}
                  {candidates.length > 0 && candidates[0]?.gate_float !== undefined && (
                    <TableHead>Gate (Float)</TableHead>
                  )}
                </TableRow>
              </TableHeader>
              <TableBody>
                {adapters.map((adapterId, idx) => {
                  const candidate = candidates.find((c: any) => c.adapter_id === adapterId);
                  const score = scores[adapterId] || candidate?.raw_score || 0;
                  
                  return (
                    <TableRow key={adapterId}>
                      <TableCell className="font-mono text-sm">{adapterId}</TableCell>
                      <TableCell>{score.toFixed(4)}</TableCell>
                      {candidate?.gate_q15 !== undefined && (
                        <TableCell>{candidate.gate_q15}</TableCell>
                      )}
                      {candidate?.gate_float !== undefined && (
                        <TableCell>{candidate.gate_float.toFixed(4)}</TableCell>
                      )}
                    </TableRow>
                  );
                })}
              </TableBody>
            </Table>
          </div>

          {/* All Candidates (if available) */}
          {candidates.length > adapters.length && (
            <div>
              <h3 className="text-sm font-semibold mb-2">All Candidates</h3>
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Adapter ID</TableHead>
                    <TableHead>Score</TableHead>
                    <TableHead>Selected</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {candidates.map((candidate: any) => (
                    <TableRow key={candidate.adapter_id}>
                      <TableCell className="font-mono text-sm">{candidate.adapter_id}</TableCell>
                      <TableCell>{candidate.raw_score?.toFixed(4) || '-'}</TableCell>
                      <TableCell>
                        {candidate.selected ? (
                          <Badge variant="default">Selected</Badge>
                        ) : (
                          <Badge variant="outline">Not Selected</Badge>
                        )}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </div>
          )}

          {/* Request ID */}
          {decision.request_id && (
            <div className="pt-2 border-t">
              <p className="text-xs text-muted-foreground">
                Request ID: <span className="font-mono">{decision.request_id}</span>
              </p>
            </div>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}

