import React from 'react';
import { GlossaryTooltip } from './glossary-tooltip';

interface HelpTooltipProps {
  helpId?: string;
  content?: string;
  children?: React.ReactNode;
  className?: string;
  side?: 'top' | 'right' | 'bottom' | 'left';
  align?: 'start' | 'center' | 'end';
}

/**
 * HelpTooltip - Legacy wrapper around GlossaryTooltip
 *
 * @deprecated Use GlossaryTooltip directly for new code
 *
 * Maintains backward compatibility with existing usage:
 * - <HelpTooltip helpId="adapter-rank" />
 * - <HelpTooltip content="Custom help text" />
 *
 * All help text content has been migrated to the glossary system.
 * This component provides a compatibility layer for existing code.
 */
export function HelpTooltip({
  helpId,
  content,
  children,
  className,
  side = 'top',
  align = 'center'
}: HelpTooltipProps) {
  // If direct content provided, use override mode
  if (content) {
    return (
      <GlossaryTooltip
        brief={content}
        side={side}
        align={align}
        className={className}
        variant={children ? 'inline' : 'icon'}
      >
        {children}
      </GlossaryTooltip>
    );
  }

  // Otherwise lookup by helpId (maps to termId in glossary)
  return (
    <GlossaryTooltip
      termId={helpId}
      side={side}
      align={align}
      className={className}
      variant={children ? 'inline' : 'icon'}
    >
      {children}
    </GlossaryTooltip>
  );
}
