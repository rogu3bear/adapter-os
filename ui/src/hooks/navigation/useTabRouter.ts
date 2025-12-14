import { useCallback, useMemo } from 'react';
import { useNavigate, useLocation, useParams } from 'react-router-dom';

/**
 * Configuration for a single tab within a tabbed interface.
 */
export interface TabConfig<T extends string> {
  /** Unique identifier for the tab */
  id: T;
  /** Display label for the tab */
  label: string;
  /** Path pattern with :params (e.g., '/training/jobs/:jobId') */
  path: string;
  /** Optional parameter name - tab only shown when this param exists */
  requiresParam?: string;
}

/**
 * Options for configuring the tab router.
 */
export interface UseTabRouterOptions<T extends string> {
  /** Array of tab configurations */
  tabs: TabConfig<T>[];
  /** Base path for the tab group (used as fallback) */
  basePath: string;
  /** Default tab to show when no match found */
  defaultTab: T;
}

/**
 * Generic tab router hook for unified tab navigation.
 * Converts hash-based routing to path-based routing for deep linking.
 *
 * @example
 * ```tsx
 * const { activeTab, setActiveTab, availableTabs, getTabPath } = useTabRouter({
 *   tabs: [
 *     { id: 'overview', label: 'Overview', path: '/training' },
 *     { id: 'jobs', label: 'Jobs', path: '/training/jobs/:jobId', requiresParam: 'jobId' }
 *   ],
 *   basePath: '/training',
 *   defaultTab: 'overview'
 * });
 * ```
 */
export function useTabRouter<T extends string>(options: UseTabRouterOptions<T>) {
  const { tabs, basePath, defaultTab } = options;
  const navigate = useNavigate();
  const location = useLocation();
  const params = useParams();

  /**
   * Resolve current active tab from pathname.
   * Matches against tab path patterns to determine which tab is active.
   */
  const activeTab = useMemo(() => {
    for (const tab of tabs) {
      const pattern = tab.path
        .replace(/:[^/]+/g, '[^/]+') // Replace :param with regex
        .replace(/\*/g, '.*'); // Handle wildcards
      const regex = new RegExp(`^${pattern}$`);

      if (regex.test(location.pathname)) {
        return tab.id;
      }
    }
    return defaultTab;
  }, [location.pathname, tabs, defaultTab]);

  /**
   * Get available tabs based on current params.
   * Filters out tabs that require params that aren't present.
   */
  const availableTabs = useMemo(() => {
    return tabs.filter(tab => {
      if (!tab.requiresParam) return true;
      return !!params[tab.requiresParam];
    });
  }, [tabs, params]);

  /**
   * Navigate to a specific tab.
   * Resolves the tab path with current params and navigates to it.
   */
  const setActiveTab = useCallback(
    (tabId: T) => {
      const tab = tabs.find(t => t.id === tabId);
      if (!tab) return;

      // Resolve path with current params
      let path = tab.path;
      for (const [key, value] of Object.entries(params)) {
        if (value) {
          path = path.replace(`:${key}`, value);
        }
      }

      navigate(path);
    },
    [tabs, params, navigate]
  );

  /**
   * Get the resolved path for a tab (for Link components).
   * Substitutes current params into the tab's path pattern.
   */
  const getTabPath = useCallback(
    (tabId: T): string => {
      const tab = tabs.find(t => t.id === tabId);
      if (!tab) return basePath;

      let path = tab.path;
      for (const [key, value] of Object.entries(params)) {
        if (value) {
          path = path.replace(`:${key}`, value);
        }
      }

      return path;
    },
    [tabs, params, basePath]
  );

  return {
    /** Currently active tab ID */
    activeTab,
    /** Function to navigate to a tab */
    setActiveTab,
    /** Tabs available based on current params */
    availableTabs,
    /** Get resolved path for a tab (for Link components) */
    getTabPath,
  };
}

/**
 * Pre-configured tab router for Adapter pages.
 * Handles adapter listing and detail views with sub-tabs.
 */
export function useAdapterTabRouter() {
  return useTabRouter({
    basePath: '/adapters',
    defaultTab: 'list',
    tabs: [
      { id: 'list', label: 'All Adapters', path: '/adapters' },
      { id: 'register', label: 'Register', path: '/adapters/new' },
      { id: 'overview', label: 'Overview', path: '/adapters/:adapterId', requiresParam: 'adapterId' },
      {
        id: 'activations',
        label: 'Activations',
        path: '/adapters/:adapterId/activations',
        requiresParam: 'adapterId',
      },
      { id: 'usage', label: 'Usage', path: '/adapters/:adapterId/usage', requiresParam: 'adapterId' },
      { id: 'lineage', label: 'Lineage', path: '/adapters/:adapterId/lineage', requiresParam: 'adapterId' },
      { id: 'manifest', label: 'Manifest', path: '/adapters/:adapterId/manifest', requiresParam: 'adapterId' },
      { id: 'policies', label: 'Policies', path: '/adapters/:adapterId/policies', requiresParam: 'adapterId' },
    ],
  });
}

/**
 * Pre-configured tab router for Training pages.
 * Handles jobs, datasets, templates, and related sub-views.
 */
export function useTrainingTabRouter() {
  return useTabRouter({
    basePath: '/training',
    defaultTab: 'overview',
    tabs: [
      { id: 'overview', label: 'Overview', path: '/training' },
      { id: 'jobs', label: 'Jobs', path: '/training/jobs' },
      { id: 'job-detail', label: 'Job Details', path: '/training/jobs/:jobId', requiresParam: 'jobId' },
      {
        id: 'job-chat',
        label: 'Result Chat',
        path: '/training/jobs/:jobId/chat',
        requiresParam: 'jobId',
      },
      { id: 'datasets', label: 'Datasets', path: '/training/datasets' },
      {
        id: 'dataset-detail',
        label: 'Dataset Details',
        path: '/training/datasets/:datasetId',
        requiresParam: 'datasetId',
      },
      {
        id: 'dataset-chat',
        label: 'Dataset Chat',
        path: '/training/datasets/:datasetId/chat',
        requiresParam: 'datasetId',
      },
      { id: 'templates', label: 'Templates', path: '/training/templates' },
      { id: 'artifacts', label: 'Artifacts', path: '/training/artifacts' },
      { id: 'settings', label: 'Settings', path: '/training/settings' },
    ],
  });
}

/**
 * Pre-configured tab router for Telemetry pages.
 * Handles event streams, viewer, alerts, and filters.
 */
export function useTelemetryTabRouter() {
  return useTabRouter({
    basePath: '/telemetry',
    defaultTab: 'event-stream',
    tabs: [
      { id: 'event-stream', label: 'Event Stream', path: '/telemetry' },
      { id: 'viewer', label: 'Viewer', path: '/telemetry/viewer' },
      { id: 'viewer-trace', label: 'Trace', path: '/telemetry/viewer/:traceId', requiresParam: 'traceId' },
      { id: 'alerts', label: 'Alerts', path: '/telemetry/alerts' },
      { id: 'exports', label: 'Exports', path: '/telemetry/exports' },
      { id: 'filters', label: 'Filters', path: '/telemetry/filters' },
    ],
  });
}

/**
 * Pre-configured tab router for Replay pages.
 * Handles run history, decision traces, and evidence.
 */
export function useReplayTabRouter() {
  return useTabRouter({
    basePath: '/replay',
    defaultTab: 'runs',
    tabs: [
      { id: 'runs', label: 'Runs', path: '/replay' },
      { id: 'decision-trace', label: 'Decision Trace', path: '/replay/decision-trace' },
      { id: 'evidence', label: 'Evidence', path: '/replay/evidence' },
      { id: 'compare', label: 'Compare', path: '/replay/compare' },
      { id: 'export', label: 'Export', path: '/replay/export' },
    ],
  });
}

/**
 * Pre-configured tab router for Repository pages.
 * Handles repo listing and detail views.
 */
export function useRepositoryTabRouter() {
  return useTabRouter({
    basePath: '/repos',
    defaultTab: 'list',
    tabs: [
      { id: 'list', label: 'All Repositories', path: '/repos' },
      { id: 'detail', label: 'Repository Detail', path: '/repos/:repoId', requiresParam: 'repoId' },
      {
        id: 'version',
        label: 'Version Detail',
        path: '/repos/:repoId/versions/:versionId',
        requiresParam: 'versionId',
      },
    ],
  });
}
