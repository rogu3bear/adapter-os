/**
 * CollapsibleSidebar - Expandable sidebar for Chat/CLI
 *
 * Desktop (lg+): Collapsible right panel (400px expanded, 48px collapsed)
 * Mobile (<lg): FAB button + bottom sheet
 *
 * Persists expanded state in localStorage.
 */

import React, { useState, useEffect, useCallback } from 'react';
import { MessageSquare, ChevronLeft, ChevronRight, X } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { cn } from '@/components/ui/utils';

interface CollapsibleSidebarProps {
  children: React.ReactNode;
  defaultExpanded?: boolean;
  className?: string;
}

const STORAGE_KEY = 'aos-sidebar-expanded';

export function CollapsibleSidebar({
  children,
  defaultExpanded = true,
  className,
}: CollapsibleSidebarProps) {
  const [isExpanded, setIsExpanded] = useState(defaultExpanded);
  const [isMobileOpen, setIsMobileOpen] = useState(false);
  const [isMounted, setIsMounted] = useState(false);

  // Load persisted state on mount
  useEffect(() => {
    setIsMounted(true);
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored !== null) {
      setIsExpanded(stored === 'true');
    }
  }, []);

  // Persist state changes
  const toggleExpanded = useCallback(() => {
    setIsExpanded((prev) => {
      const newValue = !prev;
      localStorage.setItem(STORAGE_KEY, String(newValue));
      return newValue;
    });
  }, []);

  const toggleMobileOpen = useCallback(() => {
    setIsMobileOpen((prev) => !prev);
  }, []);

  // Close mobile sheet on escape
  useEffect(() => {
    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && isMobileOpen) {
        setIsMobileOpen(false);
      }
    };
    window.addEventListener('keydown', handleEscape);
    return () => window.removeEventListener('keydown', handleEscape);
  }, [isMobileOpen]);

  // Prevent body scroll when mobile sheet is open
  useEffect(() => {
    if (isMobileOpen) {
      document.body.style.overflow = 'hidden';
    } else {
      document.body.style.overflow = '';
    }
    return () => {
      document.body.style.overflow = '';
    };
  }, [isMobileOpen]);

  // Prevent SSR hydration mismatch
  if (!isMounted) {
    return null;
  }

  return (
    <>
      {/* Desktop Sidebar */}
      <div
        className={cn(
          'hidden lg:flex flex-col h-full transition-all duration-300 ease-in-out',
          isExpanded ? 'w-[400px]' : 'w-12',
          className
        )}
      >
        {/* Collapse Toggle */}
        <div className="flex items-center justify-between p-2 border-b bg-slate-50">
          {isExpanded && (
            <span className="text-sm font-medium text-slate-700 px-2">
              Assistant
            </span>
          )}
          <Button
            variant="ghost"
            size="sm"
            onClick={toggleExpanded}
            className="h-8 w-8 p-0"
            aria-label={isExpanded ? 'Collapse sidebar' : 'Expand sidebar'}
          >
            {isExpanded ? (
              <ChevronRight className="h-4 w-4" />
            ) : (
              <ChevronLeft className="h-4 w-4" />
            )}
          </Button>
        </div>

        {/* Content */}
        {isExpanded ? (
          <div className="flex-1 overflow-hidden">{children}</div>
        ) : (
          <div className="flex-1 flex flex-col items-center py-4 gap-2">
            <Button
              variant="ghost"
              size="sm"
              onClick={toggleExpanded}
              className="h-10 w-10 p-0 rounded-lg"
              aria-label="Open chat"
            >
              <MessageSquare className="h-5 w-5 text-slate-600" />
            </Button>
          </div>
        )}
      </div>

      {/* Mobile FAB */}
      <div className="lg:hidden fixed bottom-6 right-6 z-40">
        <Button
          size="lg"
          onClick={toggleMobileOpen}
          className="h-14 w-14 rounded-full shadow-lg bg-blue-600 hover:bg-blue-700"
          aria-label="Open assistant"
        >
          <MessageSquare className="h-6 w-6" />
        </Button>
      </div>

      {/* Mobile Bottom Sheet */}
      {isMobileOpen && (
        <>
          {/* Backdrop */}
          <div
            className="lg:hidden fixed inset-0 bg-black/50 z-40 animate-in fade-in duration-200"
            onClick={toggleMobileOpen}
            aria-hidden="true"
          />

          {/* Sheet */}
          <div
            className={cn(
              'lg:hidden fixed inset-x-0 bottom-0 z-50 bg-white rounded-t-2xl shadow-xl',
              'animate-in slide-in-from-bottom duration-300',
              'h-[85vh] flex flex-col'
            )}
          >
            {/* Handle */}
            <div className="flex items-center justify-center py-2">
              <div className="w-10 h-1 bg-slate-300 rounded-full" />
            </div>

            {/* Header */}
            <div className="flex items-center justify-between px-4 pb-3 border-b">
              <span className="text-sm font-medium text-slate-700">
                Assistant
              </span>
              <Button
                variant="ghost"
                size="sm"
                onClick={toggleMobileOpen}
                className="h-8 w-8 p-0"
                aria-label="Close"
              >
                <X className="h-4 w-4" />
              </Button>
            </div>

            {/* Content */}
            <div className="flex-1 overflow-hidden">{children}</div>
          </div>
        </>
      )}
    </>
  );
}
