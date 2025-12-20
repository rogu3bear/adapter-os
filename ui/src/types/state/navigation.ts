/**
 * Navigation State Types
 *
 * State management for navigation, routing, and breadcrumbs.
 *
 * Citations:
 * - ui/src/components/BreadcrumbNavigation.tsx - Breadcrumb component
 * - ui/src/config/breadcrumbs.ts - Breadcrumb configuration
 */

/**
 * Breadcrumb item
 */
export interface BreadcrumbItem {
  /** Display label */
  label: string;
  /** Route path */
  href?: string;
  /** Whether item is current page */
  isCurrent?: boolean;
  /** Icon component name */
  icon?: string;
  /** Additional metadata */
  metadata?: Record<string, unknown>;
}

/**
 * Breadcrumb state
 */
export interface BreadcrumbState {
  /** Breadcrumb items */
  items: BreadcrumbItem[];
  /** Current page label */
  currentLabel: string;
  /** Whether breadcrumbs are loading */
  isLoading?: boolean;
}

/**
 * Navigation tab
 */
export interface NavigationTab {
  /** Tab ID */
  id: string;
  /** Display label */
  label: string;
  /** Route path */
  path: string;
  /** Icon component name */
  icon?: string;
  /** Badge count */
  badge?: number;
  /** Whether tab is disabled */
  disabled?: boolean;
  /** Whether tab is hidden */
  hidden?: boolean;
}

/**
 * Tab navigation state
 */
export interface TabNavigationState {
  /** Available tabs */
  tabs: NavigationTab[];
  /** Active tab ID */
  activeTabId: string;
  /** Whether tabs are sticky */
  isSticky?: boolean;
}

/**
 * Tab navigation actions
 */
export interface TabNavigationActions {
  /** Set active tab */
  setActiveTab: (tabId: string) => void;
  /** Go to next tab */
  nextTab: () => void;
  /** Go to previous tab */
  previousTab: () => void;
}

/**
 * Navigation history state
 */
export interface NavigationHistoryState {
  /** Previous route */
  previousRoute: string | null;
  /** Can go back */
  canGoBack: boolean;
  /** Can go forward */
  canGoForward: boolean;
  /** History stack depth */
  depth: number;
}

/**
 * Navigation history actions
 */
export interface NavigationHistoryActions {
  /** Go back */
  goBack: () => void;
  /** Go forward */
  goForward: () => void;
  /** Push new route to history */
  push: (route: string) => void;
  /** Replace current route */
  replace: (route: string) => void;
}

/**
 * Sidebar state
 */
export interface SidebarState {
  /** Whether sidebar is open */
  open: boolean;
  /** Whether sidebar is collapsed */
  collapsed: boolean;
  /** Active section ID */
  activeSection?: string;
  /** Sidebar width */
  width?: number;
}

/**
 * Sidebar actions
 */
export interface SidebarActions {
  /** Toggle sidebar open/closed */
  toggleSidebar: () => void;
  /** Toggle sidebar collapsed/expanded */
  toggleCollapsed: () => void;
  /** Set active section */
  setActiveSection: (sectionId: string) => void;
  /** Open sidebar */
  openSidebar: () => void;
  /** Close sidebar */
  closeSidebar: () => void;
}

/**
 * Complete sidebar state with actions
 */
export interface SidebarStateWithActions extends SidebarState, SidebarActions {}

/**
 * Route parameter state
 */
export interface RouteParamState {
  /** Route parameters */
  params: Record<string, string>;
  /** Query parameters */
  query: Record<string, string | string[]>;
  /** Hash fragment */
  hash?: string;
}

/**
 * Navigation context
 */
export interface NavigationContext {
  /** Current route path */
  pathname: string;
  /** Route parameters */
  params: RouteParamState;
  /** Navigation history */
  history: NavigationHistoryState;
  /** Breadcrumbs */
  breadcrumbs: BreadcrumbState;
  /** Active sidebar */
  sidebar?: SidebarState;
}
