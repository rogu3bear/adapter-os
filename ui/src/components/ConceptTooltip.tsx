import React from 'react';
import { HelpCircle } from 'lucide-react';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from './ui/tooltip';
import { getConceptTooltip } from '../data/concept-tooltips';

interface ConceptTooltipProps {
  concept: string;
  className?: string;
}

/**
 * ConceptTooltip Component
 *
 * Displays a "?" icon that shows a tooltip with the concept definition
 * when hovered. Definitions come from docs/CONCEPTS.md via concept-tooltips.ts
 *
 * Usage:
 *   <ConceptTooltip concept="tenant" />
 *   <ConceptTooltip concept="adapter" />
 */
export function ConceptTooltip({ concept, className = '' }: ConceptTooltipProps) {
  const tooltip = getConceptTooltip(concept);

  if (!tooltip) {
    // eslint-disable-next-line no-console
    console.warn(`ConceptTooltip: No tooltip found for concept "${concept}"`);
    return null;
  }

  return (
    <TooltipProvider>
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            className={`inline-flex items-center justify-center text-muted-foreground hover:text-foreground transition-colors ${className}`}
            aria-label={`Help: ${tooltip.term}`}
          >
            <HelpCircle className="h-4 w-4" />
          </button>
        </TooltipTrigger>
        <TooltipContent className="max-w-xs">
          <div className="space-y-2">
            <p className="font-semibold">{tooltip.term}</p>
            <p className="text-sm">{tooltip.definition}</p>
            {tooltip.learnMoreUrl && (
              <a
                href={tooltip.learnMoreUrl}
                className="text-xs text-primary hover:underline block"
                target="_blank"
                rel="noopener noreferrer"
              >
                Learn more →
              </a>
            )}
          </div>
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}
