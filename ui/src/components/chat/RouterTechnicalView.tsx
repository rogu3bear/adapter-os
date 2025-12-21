/**
 * RouterTechnicalView - Technical details of router decision
 *
 * Shows the full technical information including Q15 gates,
 * entropy values, hash chain, and all candidate adapters.
 */

import React, { useState } from 'react';
import { ChevronDown, ChevronRight, Copy, CheckCircle } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible';
import { PROOF_TERMS } from '@/constants/terminology';

interface Q15Gate {
  adapterId: string;
  adapterName: string;
  gateValue: number;
  rank: number;
}

interface RouterTechnicalData {
  /** Q15 quantized gate values */
  q15Gates: Q15Gate[];
  /** Shannon entropy of the distribution */
  entropy: number;
  /** Hash chain for verification */
  hashChain: {
    inputHash: string;
    outputHash: string;
    timestamp: string;
  };
  /** All candidates considered */
  allCandidates: Array<{
    id: string;
    name: string;
    score: number;
    selected: boolean;
  }>;
  /** Raw decision JSON */
  rawDecision?: Record<string, unknown>;
}

interface RouterTechnicalViewProps {
  data: RouterTechnicalData;
}

function MetricRow({
  label,
  technicalLabel,
  value,
  description,
}: {
  label: string;
  technicalLabel: string;
  value: string | number;
  description: string;
}) {
  return (
    <div className="flex justify-between items-start py-2 border-b last:border-0">
      <div>
        <div className="font-medium text-sm">{label}</div>
        <div className="text-xs text-muted-foreground">{technicalLabel}</div>
      </div>
      <div className="text-right">
        <div className="font-mono text-sm">{value}</div>
        <div className="text-xs text-muted-foreground max-w-[200px]">
          {description}
        </div>
      </div>
    </div>
  );
}

function HashDisplay({ label, hash }: { label: string; hash: string }) {
  const [copied, setCopied] = useState(false);

  const handleCopy = () => {
    navigator.clipboard.writeText(hash);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="flex items-center justify-between p-2 bg-muted/30 rounded">
      <div>
        <div className="text-xs text-muted-foreground">{label}</div>
        <div className="font-mono text-xs truncate max-w-[250px]">{hash}</div>
      </div>
      <Button variant="ghost" size="sm" onClick={handleCopy}>
        {copied ? (
          <CheckCircle className="h-4 w-4 text-green-500" />
        ) : (
          <Copy className="h-4 w-4" />
        )}
      </Button>
    </div>
  );
}

export default function RouterTechnicalView({ data }: RouterTechnicalViewProps) {
  const [showAllCandidates, setShowAllCandidates] = useState(false);
  const [showRawJson, setShowRawJson] = useState(false);

  return (
    <div className="space-y-4">
      {/* Metrics Grid */}
      <div className="border rounded-lg p-4">
        <h4 className="font-medium mb-3">Technical Metrics</h4>
        <MetricRow
          label={PROOF_TERMS.entropy.friendly}
          technicalLabel={PROOF_TERMS.entropy.technical}
          value={data.entropy.toFixed(4)}
          description={PROOF_TERMS.entropy.description}
        />
        <MetricRow
          label="Gate Count"
          technicalLabel="Q15 Quantized Gates"
          value={data.q15Gates.length}
          description="Number of adapters evaluated"
        />
      </div>

      {/* Q15 Gates Table */}
      <Collapsible>
        <CollapsibleTrigger asChild>
          <Button variant="ghost" className="w-full justify-between">
            <span>Q15 Gates Table</span>
            <ChevronDown className="h-4 w-4" />
          </Button>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Rank</TableHead>
                <TableHead>Adapter</TableHead>
                <TableHead className="text-right">Gate Value</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {data.q15Gates.map((gate) => (
                <TableRow key={gate.adapterId}>
                  <TableCell>{gate.rank}</TableCell>
                  <TableCell className="font-mono text-sm">
                    {gate.adapterName}
                  </TableCell>
                  <TableCell className="text-right font-mono">
                    {gate.gateValue.toFixed(6)}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </CollapsibleContent>
      </Collapsible>

      {/* Hash Chain */}
      <Collapsible>
        <CollapsibleTrigger asChild>
          <Button variant="ghost" className="w-full justify-between">
            <span>Hash Chain (Audit Trail)</span>
            <ChevronDown className="h-4 w-4" />
          </Button>
        </CollapsibleTrigger>
        <CollapsibleContent className="space-y-2 pt-2">
          <HashDisplay label="Input Hash" hash={data.hashChain.inputHash} />
          <HashDisplay label="Output Hash" hash={data.hashChain.outputHash} />
          <div className="text-xs text-muted-foreground text-center">
            Timestamp: {data.hashChain.timestamp}
          </div>
        </CollapsibleContent>
      </Collapsible>

      {/* All Candidates */}
      <Collapsible open={showAllCandidates} onOpenChange={setShowAllCandidates}>
        <CollapsibleTrigger asChild>
          <Button variant="ghost" className="w-full justify-between">
            <span>All Candidates ({data.allCandidates.length})</span>
            {showAllCandidates ? (
              <ChevronDown className="h-4 w-4" />
            ) : (
              <ChevronRight className="h-4 w-4" />
            )}
          </Button>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Adapter</TableHead>
                <TableHead className="text-right">Score</TableHead>
                <TableHead className="text-right">Status</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {data.allCandidates.map((candidate) => (
                <TableRow key={candidate.id}>
                  <TableCell className="font-mono text-sm">
                    {candidate.name}
                  </TableCell>
                  <TableCell className="text-right font-mono">
                    {(candidate.score * 100).toFixed(2)}%
                  </TableCell>
                  <TableCell className="text-right">
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
        </CollapsibleContent>
      </Collapsible>

      {/* Raw JSON */}
      {data.rawDecision && (
        <Collapsible open={showRawJson} onOpenChange={setShowRawJson}>
          <CollapsibleTrigger asChild>
            <Button variant="ghost" className="w-full justify-between">
              <span>Raw Decision JSON</span>
              {showRawJson ? (
                <ChevronDown className="h-4 w-4" />
              ) : (
                <ChevronRight className="h-4 w-4" />
              )}
            </Button>
          </CollapsibleTrigger>
          <CollapsibleContent>
            <pre className="p-4 bg-muted rounded-lg overflow-auto max-h-64 text-xs">
              {JSON.stringify(data.rawDecision, null, 2)}
            </pre>
          </CollapsibleContent>
        </Collapsible>
      )}
    </div>
  );
}
