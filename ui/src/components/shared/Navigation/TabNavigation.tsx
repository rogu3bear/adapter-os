import React from 'react';
import { Link, useLocation, useNavigate } from 'react-router-dom';
import { LucideIcon } from 'lucide-react';

import { cn } from '../../ui/utils';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '../../ui/tabs';
import { Badge } from '../../ui/badge';

export interface TabItem {
  /** Unique identifier for the tab */
  id: string;
  /** Display label */
  label: string;
  /** Optional icon */
  icon?: LucideIcon;
  /** Route path for navigation (if using route-based tabs) */
  href?: string;
  /** Badge content (e.g., count) */
  badge?: string | number;
  /** Badge variant */
  badgeVariant?: 'default' | 'secondary' | 'destructive' | 'outline';
  /** Disabled state */
  disabled?: boolean;
  /** Tab content (for controlled tabs) */
  content?: React.ReactNode;
}

export interface TabNavigationProps {
  /** Array of tab items */
  tabs: TabItem[];
  /** Currently active tab ID (for controlled mode) */
  activeTab?: string;
  /** Callback when tab changes (for controlled mode) */
  onTabChange?: (tabId: string) => void;
  /** Use React Router for navigation */
  useRouting?: boolean;
  /** Base path for routing (tabs will navigate to basePath + tab.href) */
  basePath?: string;
  /** Match route mode: 'exact' or 'prefix' */
  routeMatch?: 'exact' | 'prefix';
  /** Tab list alignment */
  align?: 'start' | 'center' | 'end';
  /** Full width tabs */
  fullWidth?: boolean;
  /** Additional CSS classes for container */
  className?: string;
  /** Additional CSS classes for tab list */
  tabListClassName?: string;
  /** Render tab content below tabs */
  renderContent?: boolean;
}

/**
 * TabNavigation - Flexible tab-based sub-navigation component
 *
 * Supports both controlled tabs and route-based navigation with
 * optional badges and icons.
 *
 * @example Route-based tabs:
 * ```tsx
 * <TabNavigation
 *   tabs={[
 *     { id: 'overview', label: 'Overview', href: '/overview', icon: LayoutDashboard },
 *     { id: 'settings', label: 'Settings', href: '/settings', icon: Settings, badge: 2 },
 *   ]}
 *   useRouting
 *   basePath="/adapters/my-adapter"
 * />
 * ```
 *
 * @example Controlled tabs:
 * ```tsx
 * <TabNavigation
 *   tabs={[
 *     { id: 'code', label: 'Code', content: <CodeEditor /> },
 *     { id: 'preview', label: 'Preview', content: <Preview /> },
 *   ]}
 *   activeTab={activeTab}
 *   onTabChange={setActiveTab}
 *   renderContent
 * />
 * ```
 */
export function TabNavigation({
  tabs,
  activeTab,
  onTabChange,
  useRouting = false,
  basePath = '',
  routeMatch = 'exact',
  align = 'start',
  fullWidth = false,
  className,
  tabListClassName,
  renderContent = false,
}: TabNavigationProps) {
  const location = useLocation();
  const navigate = useNavigate();

  // Determine active tab from route or prop
  const getActiveTabId = (): string => {
    if (activeTab) return activeTab;

    if (useRouting) {
      const currentPath = location.pathname;

      // Find matching tab based on route
      for (const tab of tabs) {
        if (!tab.href) continue;

        const fullPath = basePath + tab.href;

        if (routeMatch === 'exact') {
          if (currentPath === fullPath) return tab.id;
        } else {
          if (currentPath.startsWith(fullPath)) return tab.id;
        }
      }

      // Default to first tab
      return tabs[0]?.id ?? '';
    }

    return tabs[0]?.id ?? '';
  };

  const currentTabId = getActiveTabId();

  const handleTabChange = (tabId: string) => {
    if (onTabChange) {
      onTabChange(tabId);
    }

    if (useRouting) {
      const tab = tabs.find(t => t.id === tabId);
      if (tab?.href) {
        navigate(basePath + tab.href);
      }
    }
  };

  const alignmentClass = {
    start: 'justify-start',
    center: 'justify-center',
    end: 'justify-end',
  }[align];

  return (
    <Tabs
      value={currentTabId}
      onValueChange={handleTabChange}
      className={cn("w-full", className)}
    >
      <TabsList
        className={cn(
          alignmentClass,
          fullWidth && "w-full",
          tabListClassName
        )}
      >
        {tabs.map((tab) => {
          const Icon = tab.icon;

          const triggerContent = (
            <>
              {Icon && <Icon className="h-4 w-4" />}
              <span>{tab.label}</span>
              {tab.badge !== undefined && (
                <Badge
                  variant={tab.badgeVariant ?? 'secondary'}
                  className="ml-1.5 h-5 px-1.5 text-xs"
                >
                  {tab.badge}
                </Badge>
              )}
            </>
          );

          // For routing mode, wrap in Link
          if (useRouting && tab.href && !tab.disabled) {
            return (
              <TabsTrigger
                key={tab.id}
                value={tab.id}
                disabled={tab.disabled}
                asChild
              >
                <Link to={basePath + tab.href} className="flex items-center gap-1.5">
                  {triggerContent}
                </Link>
              </TabsTrigger>
            );
          }

          return (
            <TabsTrigger
              key={tab.id}
              value={tab.id}
              disabled={tab.disabled}
              className="flex items-center gap-1.5"
            >
              {triggerContent}
            </TabsTrigger>
          );
        })}
      </TabsList>

      {/* Render tab content if enabled */}
      {renderContent && tabs.map((tab) => (
        <TabsContent key={tab.id} value={tab.id}>
          {tab.content}
        </TabsContent>
      ))}
    </Tabs>
  );
}

/**
 * useTabNavigation - Hook for managing tab state with URL sync
 *
 * @param tabs - Array of tab items
 * @param options - Configuration options
 * @returns Tab state and handlers
 *
 * @example
 * ```tsx
 * const { activeTab, setActiveTab, getTabProps } = useTabNavigation(tabs, {
 *   defaultTab: 'overview',
 *   syncWithUrl: true,
 *   urlParam: 'tab',
 * });
 * ```
 */
export function useTabNavigation(
  tabs: TabItem[],
  options: {
    defaultTab?: string;
    syncWithUrl?: boolean;
    urlParam?: string;
  } = {}
) {
  const { defaultTab, syncWithUrl = false, urlParam = 'tab' } = options;
  const location = useLocation();
  const navigate = useNavigate();

  // Get initial tab from URL or default
  const getInitialTab = (): string => {
    if (syncWithUrl) {
      const params = new URLSearchParams(location.search);
      const urlTab = params.get(urlParam);
      if (urlTab && tabs.find(t => t.id === urlTab)) {
        return urlTab;
      }
    }
    return defaultTab ?? tabs[0]?.id ?? '';
  };

  const [activeTab, setActiveTabState] = React.useState(getInitialTab);

  // Update URL when tab changes
  const setActiveTab = React.useCallback((tabId: string) => {
    setActiveTabState(tabId);

    if (syncWithUrl) {
      const params = new URLSearchParams(location.search);
      params.set(urlParam, tabId);
      navigate({ search: params.toString() }, { replace: true });
    }
  }, [syncWithUrl, urlParam, location.search, navigate]);

  // Sync with URL changes
  React.useEffect(() => {
    if (syncWithUrl) {
      const params = new URLSearchParams(location.search);
      const urlTab = params.get(urlParam);
      if (urlTab && urlTab !== activeTab && tabs.find(t => t.id === urlTab)) {
        setActiveTabState(urlTab);
      }
    }
  }, [location.search, syncWithUrl, urlParam, activeTab, tabs]);

  const getTabProps = (tabId: string) => ({
    isActive: activeTab === tabId,
    onClick: () => setActiveTab(tabId),
  });

  return {
    activeTab,
    setActiveTab,
    getTabProps,
    tabs,
  };
}
