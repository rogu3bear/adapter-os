import React from 'react';
import { ShieldCheck } from 'lucide-react';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';

interface Props {
  isVerified: boolean;
  timestamp?: string;
}

export function ProofBadge({ isVerified, timestamp }: Props) {
  if (!isVerified) return null;

  return (
    <Tooltip>
      <TooltipTrigger>
        <div className="flex items-center gap-1 text-green-600">
          <ShieldCheck className="h-4 w-4" />
        </div>
      </TooltipTrigger>
      <TooltipContent>
        <p>Response verified</p>
        {timestamp && <p className="text-xs">at {new Date(timestamp).toLocaleString()}</p>}
      </TooltipContent>
    </Tooltip>
  );
}
