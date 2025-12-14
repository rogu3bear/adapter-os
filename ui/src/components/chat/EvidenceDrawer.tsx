/**
 * EvidenceDrawer - Sliding drawer for evidence display
 *
 * Shows Rulebook (citations) and Calculation (receipts) tabs
 * in a right-side drawer. Stays open across message navigation.
 */

import { useEffect, useCallback } from 'react';
import { ScrollText, Calculator, Activity, Pin, PinOff, ArrowDown } from 'lucide-react';
import {
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
  SheetDescription,
} from '@/components/ui/sheet';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Button } from '@/components/ui/button';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { useEvidenceDrawer, type EvidenceDrawerTab } from '@/contexts/EvidenceDrawerContext';
import { RulebookTab } from './drawer/RulebookTab';
import { CalculationTab } from './drawer/CalculationTab';
import { TraceTab } from './drawer/TraceTab';

interface EvidenceDrawerProps {
  /** Callback when user clicks to view a document */
  onViewDocument?: (
    documentId: string,
    pageNumber?: number,
    highlightText?: string
  ) => void;
}

export function EvidenceDrawer({ onViewDocument }: EvidenceDrawerProps) {
  const {
    isOpen,
    activeMessageId,
    activeTab,
    currentEvidence,
    currentRouterDecision,
    currentRequestId,
    currentTraceId,
    currentProofDigest,
    currentIsVerified,
    currentVerifiedAt,
    currentThroughputStats,
    isPinned,
    latestMessageId,
    closeDrawer,
    setActiveTab,
    togglePin,
    jumpToLatest,
  } = useEvidenceDrawer();

  // Computed values for UI state
  const isViewingLatest = activeMessageId === latestMessageId;
  const showJumpToLatest = isPinned && latestMessageId && !isViewingLatest;

  // Tab order for keyboard navigation
  const tabOrder: EvidenceDrawerTab[] = ['rulebook', 'calculation', 'trace'];

  // Handle keyboard navigation
  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (!isOpen) return;

      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        closeDrawer();
      } else if (e.key === 'ArrowLeft') {
        e.preventDefault();
        const currentIndex = tabOrder.indexOf(activeTab);
        const prevIndex = (currentIndex - 1 + tabOrder.length) % tabOrder.length;
        setActiveTab(tabOrder[prevIndex]);
      } else if (e.key === 'ArrowRight') {
        e.preventDefault();
        const currentIndex = tabOrder.indexOf(activeTab);
        const nextIndex = (currentIndex + 1) % tabOrder.length;
        setActiveTab(tabOrder[nextIndex]);
      }
    },
    [isOpen, activeTab, closeDrawer, setActiveTab]
  );

  useEffect(() => {
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [handleKeyDown]);

  // Handle second Esc to focus chat input
  useEffect(() => {
    const handleSecondEsc = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && !isOpen) {
        const chatInput = document.querySelector(
          '[data-testid="chat-input"], textarea[placeholder*="message"]'
        ) as HTMLElement | null;
        chatInput?.focus();
      }
    };

    window.addEventListener('keydown', handleSecondEsc);
    return () => window.removeEventListener('keydown', handleSecondEsc);
  }, [isOpen]);

  const handleOpenChange = (open: boolean) => {
    if (!open) {
      closeDrawer();
    }
  };

  const handleTabChange = (value: string) => {
    setActiveTab(value as EvidenceDrawerTab);
  };

  return (
    <Sheet open={isOpen} onOpenChange={handleOpenChange}>
      <SheetContent
        side="right"
        className="w-[90vw] sm:w-[400px] sm:max-w-[450px] flex flex-col"
      >
        <SheetHeader>
          <div className="flex items-center justify-between">
            <SheetTitle className="flex items-center gap-2">
              Evidence
            </SheetTitle>
            <TooltipProvider>
              <div className="flex items-center gap-1">
                {/* Jump to Latest button */}
                {showJumpToLatest && (
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={jumpToLatest}
                        className="h-8 px-2 gap-1"
                      >
                        <ArrowDown className="h-4 w-4" />
                        <span className="text-xs">Latest</span>
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent>Jump to latest message</TooltipContent>
                  </Tooltip>
                )}

                {/* Pin toggle */}
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      variant={isPinned ? 'secondary' : 'ghost'}
                      size="sm"
                      onClick={togglePin}
                      className="h-8 w-8 p-0"
                      aria-pressed={isPinned}
                      aria-label={isPinned ? 'Unpin to resume auto-follow' : 'Pin to stay on this message'}
                    >
                      {isPinned ? (
                        <Pin className="h-4 w-4" />
                      ) : (
                        <PinOff className="h-4 w-4 text-muted-foreground" />
                      )}
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>
                    {isPinned ? 'Unpin to resume auto-follow' : 'Pin to stay on this message'}
                  </TooltipContent>
                </Tooltip>
              </div>
            </TooltipProvider>
          </div>
          {activeMessageId && (
            <SheetDescription className="truncate flex items-center gap-2">
              <span>Message: {activeMessageId.slice(0, 8)}...</span>
              {isPinned && (
                <span className="text-xs bg-muted px-1.5 py-0.5 rounded">Pinned</span>
              )}
            </SheetDescription>
          )}
        </SheetHeader>

        <Tabs
          value={activeTab}
          onValueChange={handleTabChange}
          className="flex-1 flex flex-col min-h-0"
        >
          {/* Responsive tabs: icons only on mobile, icons + text on larger screens */}
          <TabsList className="w-full grid grid-cols-3 h-auto">
            <TabsTrigger value="rulebook" className="py-2 px-1 sm:px-3 text-xs sm:text-sm">
              <ScrollText className="h-4 w-4 sm:mr-1.5" />
              <span className="hidden sm:inline">Rulebook</span>
              <span className="sr-only sm:hidden">Rulebook</span>
            </TabsTrigger>
            <TabsTrigger value="calculation" className="py-2 px-1 sm:px-3 text-xs sm:text-sm">
              <Calculator className="h-4 w-4 sm:mr-1.5" />
              <span className="hidden sm:inline">Calculation</span>
              <span className="sr-only sm:hidden">Calculation</span>
            </TabsTrigger>
            <TabsTrigger value="trace" className="py-2 px-1 sm:px-3 text-xs sm:text-sm">
              <Activity className="h-4 w-4 sm:mr-1.5" />
              <span className="hidden sm:inline">Trace</span>
              <span className="sr-only sm:hidden">Trace</span>
            </TabsTrigger>
          </TabsList>

          <ScrollArea className="flex-1 mt-4">
            <TabsContent value="rulebook" className="m-0">
              <RulebookTab
                evidence={currentEvidence}
                onViewDocument={onViewDocument}
              />
            </TabsContent>

            <TabsContent value="calculation" className="m-0">
              <CalculationTab
                requestId={currentRequestId}
                routerDecision={currentRouterDecision}
                traceId={currentTraceId}
                proofDigest={currentProofDigest}
                isVerified={currentIsVerified}
                verifiedAt={currentVerifiedAt}
                throughputStats={currentThroughputStats}
              />
            </TabsContent>

            <TabsContent value="trace" className="m-0">
              <TraceTab traceId={currentTraceId} />
            </TabsContent>
          </ScrollArea>
        </Tabs>
      </SheetContent>
    </Sheet>
  );
}

export default EvidenceDrawer;
