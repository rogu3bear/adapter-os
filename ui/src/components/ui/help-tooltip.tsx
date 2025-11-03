import React from 'react';
import { Tooltip, TooltipContent, TooltipTrigger } from './tooltip';
import { HelpCircle } from 'lucide-react';
import { cn } from './utils';
import { getHelpText } from '@/data/help-text';

interface HelpTooltipProps {
  helpId: string;
  children: React.ReactNode;
  className?: string;
  side?: 'top' | 'right' | 'bottom' | 'left';
  align?: 'start' | 'center' | 'end';
}

export function HelpTooltip({ 
  helpId, 
  children, 
  className,
  side = 'top',
  align = 'center'
}: HelpTooltipProps) {
  // Fallback help texts for items not yet in database
  const fallbackTexts: Record<string, string> = {
    'cpid': 'Control Plane ID: identifier that groups policies, plans, and telemetry.',
    'merkle-root': 'Root hash of a Merkle tree used to attest integrity of bundled events.',
    'schema-hash': 'Content hash of the policy schema version applied to a policy pack.',
    'tokens-per-second': 'Throughput: number of tokens processed per second across the system.',
    'latency-p95': 'Latency p95: 95th percentile end-to-end response latency in milliseconds.',
    'adapter-count': 'Total number of active code adapters loaded in the system.',
    'active-sessions': 'Concurrent active user or service sessions currently using the system.',
    'requires-admin': 'This action requires the Admin role. Contact an administrator for access.',
    'operations': 'Runtime management, plan execution, and system monitoring.',
    'settings': 'System configuration and administration.'
  };

  const helpItem = getHelpText(helpId);
  const helpText = helpItem?.content || fallbackTexts[helpId] || 'Help information not available.';

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        {children}
      </TooltipTrigger>
      <TooltipContent 
        side={side} 
        align={align}
        className={cn("max-w-xs", className)}
      >
        <div className="space-y-1">
          <div className="flex items-center gap-1">
            <HelpCircle className="h-3 w-3" />
            <span className="font-medium text-xs">Help</span>
          </div>
          <p className="text-xs leading-relaxed">{helpText}</p>
        </div>
      </TooltipContent>
    </Tooltip>
  );
}
