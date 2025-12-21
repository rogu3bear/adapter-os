import { useMemo, useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Copy } from 'lucide-react';
import type { TraceResponseV1 } from '@/api/types';

interface TraceTokenTableProps {
  tokens: TraceResponseV1['tokens'];
}

function copy(text: string) {
  if (navigator?.clipboard?.writeText) {
    navigator.clipboard.writeText(text);
  }
}

export function TraceTokenTable({ tokens }: TraceTokenTableProps) {
  const [adapterFilter, setAdapterFilter] = useState<string>('all');

  const adapterOptions = useMemo(() => {
    const set = new Set<string>();
    tokens.forEach((token) => token.selected_adapter_ids.forEach((id) => set.add(id)));
    return Array.from(set);
  }, [tokens]);

  const filteredTokens = useMemo(
    () => (adapterFilter === 'all' ? tokens : tokens.filter((t) => t.selected_adapter_ids.includes(adapterFilter))),
    [adapterFilter, tokens]
  );

  return (
    <Card>
      <CardHeader className="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
        <div>
          <CardTitle className="text-base">Token Decisions</CardTitle>
          <CardDescription>Per-token routing decisions and policy digests.</CardDescription>
        </div>
        <div className="flex items-center gap-3">
          <div className="text-xs text-muted-foreground">
            Showing {filteredTokens.length} of {tokens.length} tokens
          </div>
          <Select value={adapterFilter} onValueChange={(value) => setAdapterFilter(value)}>
            <SelectTrigger className="w-[220px]">
              <SelectValue placeholder="Filter by adapter" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">All adapters</SelectItem>
              {adapterOptions.map((adapterId) => (
                <SelectItem key={adapterId} value={adapterId}>
                  {adapterId}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      </CardHeader>
      <CardContent>
        {!filteredTokens.length ? (
          <div className="text-sm text-muted-foreground">No tokens match this adapter filter.</div>
        ) : (
          <ScrollArea className="w-full">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="w-[90px]">Token #</TableHead>
                  <TableHead>Token ID</TableHead>
                  <TableHead>Adapters / Gates (Q15)</TableHead>
                  <TableHead>Decision hash</TableHead>
                  <TableHead>Policy mask digest</TableHead>
                  <TableHead>Fusion</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {filteredTokens.map((token) => (
                  <TableRow key={token.token_index}>
                    <TableCell className="font-mono">{token.token_index}</TableCell>
                    <TableCell className="font-mono text-xs text-muted-foreground">
                      {token.token_id ?? '—'}
                    </TableCell>
                    <TableCell>
                      <div className="flex flex-wrap gap-2">
                        {token.selected_adapter_ids.map((adapterId, idx) => (
                          <Badge key={`${adapterId}-${idx}`} variant="outline" className="text-xs">
                            {adapterId} · {token.gates_q15[idx] ?? '—'}
                          </Badge>
                        ))}
                      </div>
                    </TableCell>
                    <TableCell className="font-mono text-xs">
                      <div className="flex items-center gap-2">
                        <span className="truncate">{token.decision_hash}</span>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-7 w-7"
                          onClick={() => copy(token.decision_hash)}
                          aria-label="Copy decision hash"
                        >
                          <Copy className="h-4 w-4" />
                        </Button>
                      </div>
                    </TableCell>
                    <TableCell className="font-mono text-xs">
                      <div className="flex items-center gap-2">
                        <span className="truncate">{token.policy_mask_digest}</span>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-7 w-7"
                          onClick={() => copy(token.policy_mask_digest)}
                          aria-label="Copy policy mask digest"
                        >
                          <Copy className="h-4 w-4" />
                        </Button>
                      </div>
                    </TableCell>
                    <TableCell className="text-xs text-muted-foreground space-y-1">
                      {token.fusion_interval_id && <div>Interval: {token.fusion_interval_id}</div>}
                      {token.fused_weight_hash && <div>Fused hash: {token.fused_weight_hash}</div>}
                      {!token.fusion_interval_id && !token.fused_weight_hash && '—'}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </ScrollArea>
        )}
      </CardContent>
    </Card>
  );
}
