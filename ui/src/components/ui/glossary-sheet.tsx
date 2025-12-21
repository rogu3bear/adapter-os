'use client';

import * as React from 'react';
import { BookOpen, Link2, X } from 'lucide-react';
import DOMPurify from 'dompurify';
import {
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
  SheetDescription,
} from './sheet';
import { Badge } from './badge';
import { Button } from './button';
import { ScrollArea } from './scroll-area';
import { Separator } from './separator';
import { cn } from '@/lib/utils';
import type { GlossaryEntry } from '@/data/glossary';
import { getGlossaryEntry, getRelatedTerms, categoryMeta } from '@/data/glossary';

interface GlossarySheetProps {
  entry: GlossaryEntry | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onNavigate?: (termId: string) => void;
}

export function GlossarySheet({
  entry,
  open,
  onOpenChange,
  onNavigate,
}: GlossarySheetProps) {
  if (!entry) {
    return null;
  }

  const category = categoryMeta[entry.category];
  // Use getRelatedTerms which handles the {id: string} structure
  const relatedTerms = getRelatedTerms(entry.id);

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent className="sm:max-w-lg">
        <SheetHeader>
          <div className="flex items-start justify-between gap-4">
            <div className="flex-1 space-y-2">
              <SheetTitle className="text-2xl">{entry.term}</SheetTitle>
              <Badge
                variant="outline"
                className="font-medium"
              >
                <BookOpen className="mr-1 h-3 w-3" />
                {category?.label || entry.category}
              </Badge>
            </div>
          </div>
          <SheetDescription className="text-base leading-relaxed">
            {entry.content.brief}
          </SheetDescription>
        </SheetHeader>

        <Separator className="my-6" />

        <ScrollArea className="h-[calc(100vh-16rem)] pr-4">
          <div className="space-y-6">
            {entry.content.detailed && (
              <div className="prose prose-sm max-w-none dark:prose-invert">
                <div
                  className="leading-relaxed"
                  dangerouslySetInnerHTML={{
                    __html: DOMPurify.sanitize(
                      entry.content.detailed.replace(/\n/g, '<br />'),
                      {
                        ALLOWED_TAGS: ['br', 'p', 'strong', 'em', 'code', 'pre', 'a', 'ul', 'ol', 'li', 'blockquote', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6'],
                        ALLOWED_ATTR: ['href', 'target', 'rel'],
                      }
                    ),
                  }}
                />
              </div>
            )}

            {relatedTerms.length > 0 && (
              <div className="space-y-3">
                <div className="flex items-center gap-2 text-sm font-medium text-muted-foreground">
                  <Link2 className="h-4 w-4" />
                  Related Terms
                </div>
                <div className="flex flex-wrap gap-2">
                  {relatedTerms.map((relatedEntry) => {
                    if (!relatedEntry) return null;
                    return (
                      <Badge
                        key={relatedEntry.id}
                        variant="secondary"
                        className="cursor-pointer transition-colors hover:bg-primary hover:text-primary-foreground"
                        onClick={() => {
                          if (onNavigate) {
                            onNavigate(relatedEntry.id);
                          }
                        }}
                      >
                        {relatedEntry.term}
                      </Badge>
                    );
                  })}
                </div>
              </div>
            )}
          </div>
        </ScrollArea>
      </SheetContent>
    </Sheet>
  );
}
