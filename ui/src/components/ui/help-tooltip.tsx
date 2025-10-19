import React from 'react';
import { Tooltip, TooltipContent, TooltipTrigger } from './tooltip';
import { HelpCircle } from 'lucide-react';
import { cn } from './utils';

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
  // For now, we'll use a simple help text lookup
  // In a real implementation, this would use the help text database
  const getHelpText = (id: string) => {
    const helpTexts: Record<string, string> = {
      'dashboard': 'System overview showing health metrics, adapter counts, and performance indicators.',
      'adapters': 'Manage LoRA adapters for specialized AI capabilities.',
      'policies': 'Configure security and compliance policies.',
      'operations': 'Runtime management, plan execution, and system monitoring.',
      'settings': 'System configuration and administration.',
      'plans': 'Execution plan compilation for adapter loading.',
      'promotion': 'Control plane promotion gates with policy compliance.',
      'telemetry': 'Event bundle management and system monitoring.',
      'inference': 'Interactive inference testing and model validation.',
      'alerts': 'System alerts and health monitoring.',
      'lora': 'Low-Rank Adaptation for efficient model fine-tuning.',
      'adapter': 'Specialized AI component for domain-specific tasks.',
      'control-plane': 'Management layer for adapter orchestration.',
      'tenant': 'Isolated workspace with dedicated resources.',
      'deterministic': 'Reproducible system behavior with identical outputs.',
      'zero-egress': 'Security mode blocking outbound network connections.',
      'policy-pack': 'Collection of security and compliance rules.',
      'telemetry-bundle': 'Compressed system events and audit logs.',
      'router': 'Component selecting best adapters for requests.',
      'k-sparse': 'Routing strategy selecting top K adapters.',
      // Newly added UX clarifications
      'cpid': 'Control Plane ID: identifier that groups policies, plans, and telemetry.',
      'merkle-root': 'Root hash of a Merkle tree used to attest integrity of bundled events.',
      'schema-hash': 'Content hash of the policy schema version applied to a policy pack.',
      'tokens-per-second': 'Throughput: number of tokens processed per second across the system.',
      'latency-p95': 'Latency p95: 95th percentile end-to-end response latency in milliseconds.',
      'adapter-count': 'Total number of active code adapters loaded in the system.',
      'active-sessions': 'Concurrent active user or service sessions currently using the system.',
      'requires-admin': 'This action requires the Admin role. Contact an administrator for access.'
    };
    return helpTexts[id] || 'Help information not available.';
  };

  const helpText = getHelpText(helpId);

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
