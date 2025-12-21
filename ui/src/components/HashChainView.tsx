import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Badge } from './ui/badge';
import { ArrowRight, CheckCircle, XCircle } from 'lucide-react';

interface HashChainViewProps {
  manifestHash: string;
  kernelHash?: string;
  policyHash: string;
  verified?: boolean;
}

export function HashChainView({ manifestHash, kernelHash, policyHash, verified }: HashChainViewProps) {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center justify-between">
          <span>Deterministic Hash Chain</span>
          {verified !== undefined && (
            <Badge variant={verified ? "default" : "destructive"}>
              {verified ? <><CheckCircle className="mr-1 h-3 w-3" /> Verified</> : <><XCircle className="mr-1 h-3 w-3" /> Invalid</>}
            </Badge>
          )}
        </CardTitle>
      </CardHeader>
      <CardContent>
        <div className="flex items-center gap-3 overflow-x-auto">
          <HashBlock label="Manifest" hash={manifestHash} />
          <ArrowRight className="h-5 w-5 text-muted-foreground flex-shrink-0" />
          {kernelHash && (
            <>
              <HashBlock label="Kernel" hash={kernelHash} />
              <ArrowRight className="h-5 w-5 text-muted-foreground flex-shrink-0" />
            </>
          )}
          <HashBlock label="Policy" hash={policyHash} />
        </div>
      </CardContent>
    </Card>
  );
}

function HashBlock({ label, hash }: { label: string; hash: string }) {
  return (
    <div className="flex flex-col items-center gap-1 p-3 border rounded bg-muted/30 w-full max-w-[240px]">
      <div className="text-xs text-muted-foreground font-medium">{label}</div>
      <code className="text-xs font-mono break-all text-center">{hash.substring(0, 16)}...</code>
      <button 
        className="text-xs text-primary hover:underline"
        onClick={() => navigator.clipboard.writeText(hash)}
      >
        Copy Full
      </button>
    </div>
  );
}

