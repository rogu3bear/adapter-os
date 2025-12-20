'use client';

import * as React from 'react';
import { HelpCircle } from 'lucide-react';
import { GlossarySheet } from './glossary-sheet';
import { HoverCard, HoverCardContent, HoverCardTrigger } from './hover-card';
import { cn } from '@/lib/utils';
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
    // Support providing term directly
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
    // Support brief-only mode (anonymous tooltip with just help text)
    if (brief) {
      return {
        id: 'inline-help',
        term: '',
        category: 'ui-fields' as const,
        content: {
          brief,
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

  const triggerLabel = entry.term || entry.content?.brief || 'Glossary info';

  return (
    <>
      <HoverCard openDelay={0} closeDelay={150}>
        <HoverCardTrigger asChild>
          <span
            role="button"
            tabIndex={0}
            aria-label={triggerLabel}
            className="inline-flex items-center rounded focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-primary focus-visible:ring-offset-2"
          >
            {renderTrigger()}
          </span>
        </HoverCardTrigger>
        <HoverCardContent side={side} align={align} className="w-fit max-w-md p-[calc(var(--base-unit)*2)] text-sm text-balance shadow-lg">
          <div className="space-y-2">
            {entry.term && <div className="font-semibold">{entry.term}</div>}
            <div className={entry.term ? "text-sm text-muted-foreground" : "text-sm"}>
              {entry.content?.brief}
            </div>
            {hasDetailed && (
              <button
                type="button"
                onClick={handleLearnMoreClick}
                className="text-sm text-primary hover:underline focus:outline-hidden focus:ring-2 focus:ring-primary focus:ring-offset-2 rounded"
              >
                Learn more →
              </button>
            )}
          </div>
        </HoverCardContent>
      </HoverCard>

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
