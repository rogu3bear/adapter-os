import React from 'react';
import { cn } from '@/lib/utils';

interface DemoRecoveryHintsProps {
  className?: string;
}

export function DemoRecoveryHints({ className }: DemoRecoveryHintsProps) {
  return (
    <div className={cn('rounded-md border bg-muted/30 p-3 text-sm', className)}>
      <div className="font-medium text-foreground">Try:</div>
      <div className="mt-1 text-muted-foreground">
        <span className="font-mono">reset-demo</span> / <span className="font-mono">seed-demo</span> /{' '}
        <span className="font-mono">dev-up</span>
      </div>
      <div className="mt-2 space-y-1 text-xs text-muted-foreground">
        <div className="font-mono break-all">reset-demo: ./aosctl db reset --force</div>
        <div className="font-mono break-all">seed-demo: ./aosctl db seed-fixtures --skip-reset</div>
        <div className="font-mono break-all">
          dev-up: AOS_E2E_RESET_DB=0 VITE_ENABLE_DEV_BYPASS=true pnpm --dir ui dev
        </div>
      </div>
    </div>
  );
}

