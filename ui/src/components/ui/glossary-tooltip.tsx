'use client';

import * as React from 'react';
import { HelpCircle } from 'lucide-react';
import { Tooltip, TooltipContent, TooltipTrigger } from './tooltip';
import { GlossarySheet } from './glossary-sheet';
import { cn } from './utils';
import { getGlossaryEntry, type GlossaryEntry } from '@/data/glossary';

interface GlossaryTooltipProps {
  /** Lookup term from glossary by ID */
  termId?: string;

  /** Override: provide term directly (bypass glossary lookup) */
  term?: string;
  brief?: string;
  detailed?: string;

  /** Visual variants */
  variant?: 'icon' | 'inline' | 'underline';

  /** Icon size */
  iconSize?: 'sm' | 'md' | 'lg';

  /** Tooltip positioning */
  side?: 'top' | 'right' | 'bottom' | 'left';
  align?: 'start' | 'center' | 'end';

  /** Custom trigger content (for inline/underline variants) */
  children?: React.ReactNode;

  /** Additional className for trigger */
  className?: string;
}

export function GlossaryTooltip({
  termId,
  term,
  brief,
  detailed,
  variant = 'icon',
  iconSize = 'md',
  side = 'top',
  align = 'center',
  children,
  className,
}: GlossaryTooltipProps) {
  const [sheetOpen, setSheetOpen] = React.useState(false);

  // Get entry from glossary or use provided props
  const entry: GlossaryEntry | null = React.useMemo(() => {
    if (termId) {
      return getGlossaryEntry(termId) ?? null;
    }
    if (term) {
      return {
        id: term.toLowerCase().replace(/\s+/g, '-'),
        term,
        category: 'ui-fields' as const,
        content: {
          brief: brief || '',
          detailed,
        },
      };
    }
    return null;
  }, [termId, term, brief, detailed]);

  if (!entry) {
    return null;
  }

  const hasDetailed = Boolean(entry.content?.detailed);

  const iconSizeClasses = {
    sm: 'h-3 w-3',
    md: 'h-4 w-4',
    lg: 'h-5 w-5',
  };

  const renderTrigger = () => {
    switch (variant) {
      case 'icon':
        return (
          <HelpCircle
            className={cn(
              iconSizeClasses[iconSize],
              'text-muted-foreground hover:text-foreground transition-colors cursor-help',
              className
            )}
          />
        );

      case 'inline':
        return (
          <span
            className={cn(
              'cursor-help text-foreground hover:text-primary transition-colors',
              className
            )}
          >
            {children || entry.term}
          </span>
        );

      case 'underline':
        return (
          <span
            className={cn(
              'cursor-help border-b border-dotted border-muted-foreground hover:border-primary hover:text-primary transition-colors',
              className
            )}
          >
            {children || entry.term}
          </span>
        );

      default:
        return null;
    }
  };

  const handleLearnMoreClick = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setSheetOpen(true);
  };

  return (
    <>
      <Tooltip>
        <TooltipTrigger asChild>
          <span className="inline-flex items-center">
            {renderTrigger()}
          </span>
        </TooltipTrigger>
        <TooltipContent side={side} align={align} className="max-w-xs">
          <div className="space-y-2">
            <div className="font-semibold">{entry.term}</div>
            <div className="text-sm text-muted-foreground">{entry.content?.brief}</div>
            {hasDetailed && (
              <button
                onClick={handleLearnMoreClick}
                className="text-sm text-primary hover:underline focus:outline-hidden focus:ring-2 focus:ring-primary focus:ring-offset-2 rounded"
              >
                Learn more →
              </button>
            )}
          </div>
        </TooltipContent>
      </Tooltip>

      {hasDetailed && (
        <GlossarySheet
          open={sheetOpen}
          onOpenChange={setSheetOpen}
          entry={entry}
        />
      )}
    </>
  );
}
