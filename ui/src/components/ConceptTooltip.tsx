import React from 'react';
import { GlossaryTooltip } from './ui/glossary-tooltip';

interface ConceptTooltipProps {
  concept: string;
  className?: string;
}

/**
 * ConceptTooltip Component
 *
 * @deprecated Use GlossaryTooltip directly for new code
 *
 * Displays a "?" icon that shows a tooltip with the concept definition
 * when hovered. Now delegates to the unified GlossaryTooltip system.
 *
 * Usage:
 *   <ConceptTooltip concept="tenant" />
 *   <ConceptTooltip concept="adapter" />
 */
export function ConceptTooltip({ concept, className = '' }: ConceptTooltipProps) {
  return (
    <GlossaryTooltip
      termId={concept}
      className={className}
      variant="icon"
      iconSize="md"
    />
  );
}
