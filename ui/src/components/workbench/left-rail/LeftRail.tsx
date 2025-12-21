/**
 * LeftRail - Container for the left rail with tabs and content
 *
 * Manages tab switching and scroll position preservation.
 * Each tab's content is rendered in a separate scrollable container
 * to preserve independent scroll positions.
 */

import { useRef, useCallback, useEffect, ReactNode } from 'react';
import { useWorkbench, type LeftRailTab } from '@/contexts/WorkbenchContext';
import { LeftRailTabs } from './LeftRailTabs';
import { cn } from '@/lib/utils';

interface LeftRailProps {
  /** Content for the Sessions tab */
  sessionsContent: ReactNode;
  /** Content for the Datasets tab */
  datasetsContent: ReactNode;
  /** Content for the Stacks tab */
  stacksContent: ReactNode;
}

export function LeftRail({
  sessionsContent,
  datasetsContent,
  stacksContent,
}: LeftRailProps) {
  const {
    activeLeftTab,
    setActiveLeftTab,
    saveScrollPosition,
    getScrollPosition,
  } = useWorkbench();

  // Refs for each tab's scroll container
  const sessionsRef = useRef<HTMLDivElement>(null);
  const datasetsRef = useRef<HTMLDivElement>(null);
  const stacksRef = useRef<HTMLDivElement>(null);

  const getRefForTab = useCallback((tab: LeftRailTab) => {
    switch (tab) {
      case 'sessions':
        return sessionsRef;
      case 'datasets':
        return datasetsRef;
      case 'stacks':
        return stacksRef;
    }
  }, []);

  // Save scroll position when scrolling
  const handleScroll = useCallback(
    (tab: LeftRailTab) => (e: React.UIEvent<HTMLDivElement>) => {
      saveScrollPosition(tab, e.currentTarget.scrollTop);
    },
    [saveScrollPosition]
  );

  // Restore scroll position when tab becomes active
  useEffect(() => {
    const ref = getRefForTab(activeLeftTab);
    const savedPosition = getScrollPosition(activeLeftTab);
    if (ref.current && savedPosition > 0) {
      ref.current.scrollTop = savedPosition;
    }
  }, [activeLeftTab, getRefForTab, getScrollPosition]);

  const renderTabPanel = (
    tab: LeftRailTab,
    ref: React.RefObject<HTMLDivElement>,
    content: ReactNode
  ) => (
    <div
      key={tab}
      id={`${tab}-panel`}
      role="tabpanel"
      aria-labelledby={`tab-${tab}`}
      className={cn(
        'flex-1 overflow-y-auto overflow-x-hidden',
        activeLeftTab === tab ? 'block' : 'hidden'
      )}
      ref={ref as React.RefObject<HTMLDivElement>}
      onScroll={handleScroll(tab)}
      data-testid={`${tab}-panel`}
    >
      {content}
    </div>
  );

  return (
    <div className="flex h-full flex-col" data-testid="left-rail">
      <LeftRailTabs activeTab={activeLeftTab} onTabChange={setActiveLeftTab} />

      {renderTabPanel('sessions', sessionsRef, sessionsContent)}
      {renderTabPanel('datasets', datasetsRef, datasetsContent)}
      {renderTabPanel('stacks', stacksRef, stacksContent)}
    </div>
  );
}
