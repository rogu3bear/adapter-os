/**
 * Routes Manifest - Enhanced route metadata for analysis and navigation
 *
 * This manifest adds semantic metadata to routes for:
 * - Debugging route sprawl
 * - Identifying duplicates and orphans
 * - Grouping navigation by section
 * - Understanding route hierarchy (hub/detail/tool)
 * - Tracking lifecycle status (active/deprecated/draft)
 * - Determining sidebar reachability
 *
 * Usage:
 * - View at /_dev/routes in development mode
 * - Import getRouteManifest() for programmatic access
 */

import { routes, type RouteConfig } from './routes';
import { UiMode } from './ui-mode';

/** Route lifecycle status */
export type RouteStatus =
  | 'active'      // Production-ready, in use
  | 'deprecated'; // Still routable, but scheduled for removal

/** Route type classification */
export type RouteType =
  | 'hub'      // Main section entry point with tabs/sub-navigation
  | 'detail'   // Detail view for a specific resource (e.g., /adapters/:id)
  | 'tool'     // Utility/action page (e.g., create, register)
  | 'landing'  // Landing/home pages
  | 'future';  // Placeholder/partially implemented

/** Semantic section for grouping */
export type RouteSection =
  | 'Core'        // Home, dashboard, onboarding
  | 'Adapters'    // Adapter management
  | 'Training'    // Training jobs, datasets, templates
  | 'Inference'   // Chat, inference, documents, RAG
  | 'System'      // Nodes, workers, memory, base models
  | 'Monitor'     // Metrics, observability, telemetry
  | 'Security'    // Policies, audit, compliance
  | 'Admin'       // Tenants, users, stacks, settings
  | 'Labs';       // Experimental/dev features

/** Reachability classification */
export type Reachability =
  | 'primary'    // In sidebar, directly navigable
  | 'nested'     // Reachable via parent route / tabs
  | 'hidden'     // Only via direct URL or internal link
  | 'orphan';    // Not reachable at all (broken)

/** Enhanced route entry with analysis metadata */
export interface RouteManifestEntry {
  path: string;
  navTitle: string | null;
  navGroup: string | null;
  section: RouteSection;
  type: RouteType;
  status: RouteStatus;
  isHub: boolean;
  inSidebar: boolean;              // Has navTitle + navGroup
  reachability: Reachability;
  tabs: string[];                  // For hub pages: list of tab paths
  expectedTabs: string[];          // For hub pages: what tabs SHOULD exist
  missingTabs: string[];           // expectedTabs - actual tabs
  extraTabs: string[];             // actual tabs - expectedTabs
  parentPath: string | null;       // Parent route for breadcrumbs
  componentFile: string;           // TSX file path (relative to src/)
  minRole: string | null;          // Minimum role required
  modes: UiMode[];                 // UI mode visibility
  issues: string[];                // Detected problems
  notes: string;                   // Implementation notes
}

/**
 * Route status overrides - mark specific routes as deprecated
 */
const STATUS_OVERRIDES: Record<string, RouteStatus> = {
  // Example: '/old-page': 'deprecated',
  '/owner': 'deprecated',                   // Owner home replaced by admin hub
  '/management': 'deprecated',              // Legacy management shell
  '/workflow': 'deprecated',                // Legacy onboarding flow, redirects to training
  '/personas': 'deprecated',                // Legacy personas tour
  '/flow/lora': 'deprecated',               // Guided flow superseded by training shell
  '/trainer': 'deprecated',                 // Quick trainer folded into training hub
  '/create-adapter': 'deprecated',          // Legacy adapter creation flow
  '/promotion': 'deprecated',               // Promotion merged into adapters/activation flows
  '/monitoring': 'deprecated',              // Folded into metrics
  '/reports': 'deprecated',                 // Folded into metrics
  '/code-intelligence': 'deprecated',       // Redirect to telemetry viewer
  '/metrics/advanced': 'deprecated',        // Consolidated into main metrics
  '/help': 'deprecated',                    // Help surface moved into dashboard
  '/admin/tenants': 'deprecated',           // Workspace hub canonical
  '/admin/tenants/:tenantId': 'deprecated', // Workspace hub canonical
  '/telemetry/traces': 'deprecated',        // Legacy trace deep links
  '/telemetry/traces/:traceId': 'deprecated', // Legacy trace deep links
  '/chat/sessions/:sessionId': 'deprecated',  // Legacy chat session deep link
  '/security': 'deprecated',                // Guardrails canonical at /security/policies
};

/**
 * Map navGroup to semantic section
 */
function inferSection(route: RouteConfig): RouteSection {
  const group = route.navGroup?.toLowerCase() ?? '';
  const path = route.path.toLowerCase();

  // Explicit mappings
  if (group === 'home') return 'Core';
  if (group === 'build' && path.includes('training')) return 'Training';
  if (group === 'build' && path.includes('adapter')) return 'Adapters';
  if (group === 'build') return 'Adapters';  // testing, golden, promotion
  if (group === 'run') return 'Inference';
  if (group === 'monitor') return 'Monitor';
  if (group === 'system') return 'System';
  if (group === 'secure') return 'Security';
  if (group === 'administration') return 'Admin';
  if (group === 'resources') return 'Core';
  if (group === 'dev tools') return 'Labs';

  // Path-based fallbacks
  if (path.includes('/training')) return 'Training';
  if (path.includes('/adapter')) return 'Adapters';
  if (path.includes('/system')) return 'System';
  if (path.includes('/admin')) return 'Admin';
  if (path.includes('/security')) return 'Security';
  if (path.includes('/dev/') || path.includes('/_dev/')) return 'Labs';

  return 'Core';
}

/**
 * Infer route type from path and structure
 */
function inferType(route: RouteConfig, allRoutes: RouteConfig[]): RouteType {
  const path = route.path;

  // Detail pages have params
  if (path.includes(':')) return 'detail';

  // Tool pages are create/register/new actions
  if (path.includes('/new') || path.includes('/create') || path.includes('/register')) {
    return 'tool';
  }

  // Landing pages
  if (path === '/owner' || path === '/dashboard' || path === '/personas') {
    return 'landing';
  }

  // Hub pages have child routes
  const hasChildren = allRoutes.some(r =>
    r.path !== path &&
    r.path.startsWith(path + '/') &&
    !r.path.includes(':')
  );
  if (hasChildren) return 'hub';

  // Check for known hub paths
  const hubPaths = [
    '/adapters', '/training', '/system', '/admin',
    '/security/policies', '/monitoring', '/documents'
  ];
  if (hubPaths.includes(path)) return 'hub';

  return 'tool';
}

/**
 * Find actual tabs for hub pages
 */
function findActualTabs(route: RouteConfig, allRoutes: RouteConfig[]): string[] {
  return allRoutes
    .filter(r =>
      r.path !== route.path &&
      r.path.startsWith(route.path + '/') &&
      r.parentPath === route.path
    )
    .map(r => r.path);
}

/**
 * Determine reachability of a route
 */
function determineReachability(
  route: RouteConfig,
  allRoutes: RouteConfig[],
  inSidebar: boolean
): Reachability {
  // In sidebar = primary
  if (inSidebar) return 'primary';

  // Has parentPath = nested
  if (route.parentPath) return 'nested';

  // Detail pages are typically nested
  if (route.path.includes(':')) return 'nested';

  // Check if any route links to this as a child
  const hasParent = allRoutes.some(r =>
    r.path !== route.path &&
    route.path.startsWith(r.path + '/') &&
    !route.path.includes(':')
  );
  if (hasParent) return 'nested';

  // No way to reach it
  return 'orphan';
}

/**
 * Extract component file path from route config
 * This inspects the component's import to determine the file
 */
function inferComponentFile(route: RouteConfig): string {
  const path = route.path;

  // Map paths to component files based on route reality (shells + redirects)
  const pathToFile: Record<string, string> = {
    '/owner': 'components/LegacyRedirectNotice.tsx',
    '/dashboard': 'pages/DashboardPage.tsx',
    '/workspaces': 'pages/WorkspacesPage.tsx',
    '/management': 'components/LegacyRedirectNotice.tsx',
    '/workflow': 'components/LegacyRedirectNotice.tsx',
    '/personas': 'components/LegacyRedirectNotice.tsx',
    '/flow/lora': 'components/LegacyRedirectNotice.tsx',
    '/repos': 'pages/Repositories/RepositoriesShell.tsx',
    '/repos/:repoId': 'pages/Repositories/RepositoriesShell.tsx',
    '/repos/:repoId/versions/:versionId': 'pages/Repositories/RepositoriesShell.tsx',
    '/trainer': 'components/LegacyRedirectNotice.tsx',
    '/create-adapter': 'components/LegacyRedirectNotice.tsx',
    '/training': 'pages/Training/TrainingShell.tsx',
    '/training/jobs': 'pages/Training/TrainingShell.tsx',
    '/training/jobs/:jobId': 'pages/Training/TrainingShell.tsx',
    '/training/jobs/:jobId/chat': 'pages/Training/ResultChatPage.tsx',
    '/training/datasets': 'pages/Training/TrainingShell.tsx',
    '/training/datasets/:datasetId': 'pages/Training/TrainingShell.tsx',
    '/training/datasets/:datasetId/chat': 'pages/Training/DatasetChatPage.tsx',
    '/training/templates': 'pages/Training/TrainingShell.tsx',
    '/training/artifacts': 'pages/Training/TrainingShell.tsx',
    '/training/settings': 'pages/Training/TrainingShell.tsx',
    '/testing': 'pages/TestingPage.tsx',
    '/golden': 'pages/GoldenPage.tsx',
    '/promotion': 'components/LegacyRedirectNotice.tsx',
    '/adapters': 'pages/Adapters/AdaptersShell.tsx',
    '/adapters/new': 'pages/Adapters/AdaptersShell.tsx',
    '/adapters/:adapterId': 'pages/Adapters/AdaptersShell.tsx',
    '/adapters/:adapterId/activations': 'pages/Adapters/AdaptersShell.tsx',
    '/adapters/:adapterId/usage': 'pages/Adapters/AdaptersShell.tsx',
    '/adapters/:adapterId/lineage': 'pages/Adapters/AdaptersShell.tsx',
    '/adapters/:adapterId/manifest': 'pages/Adapters/AdaptersShell.tsx',
    '/adapters/:adapterId/policies': 'pages/Adapters/AdaptersShell.tsx',
    '/metrics': 'pages/MetricsPage.tsx',
    '/monitoring': 'components/LegacyRedirectNotice.tsx',
    '/routing': 'pages/RoutingPage.tsx',
    '/system': 'pages/System/SystemOverviewPage.tsx',
    '/system/nodes': 'pages/System/NodesTab.tsx',
    '/system/nodes/:nodeId': 'pages/System/NodeDetailModal.tsx',
    '/system/workers': 'pages/System/WorkersTab.tsx',
    '/system/memory': 'pages/System/MemoryTab.tsx',
    '/system/metrics': 'pages/System/MetricsTab.tsx',
    '/system/pilot-status': 'pages/System/PilotStatusPage.tsx',
    '/inference': 'pages/InferencePage.tsx',
    '/chat': 'pages/ChatPage.tsx',
    '/chat/sessions/:sessionId': 'components/LegacyRedirectNotice.tsx',
    '/documents': 'pages/DocumentLibrary/index.tsx',
    '/documents/:documentId/chat': 'pages/DocumentLibrary/DocumentChatPage.tsx',
    '/telemetry': 'pages/TelemetryPage.tsx',
    '/telemetry/viewer': 'pages/TelemetryPage.tsx',
    '/telemetry/viewer/:traceId': 'pages/TelemetryPage.tsx',
    '/telemetry/traces': 'components/LegacyRedirectNotice.tsx',
    '/telemetry/traces/:traceId': 'components/LegacyRedirectNotice.tsx',
    '/telemetry/alerts': 'pages/TelemetryPage.tsx',
    '/telemetry/exports': 'pages/TelemetryPage.tsx',
    '/telemetry/filters': 'pages/TelemetryPage.tsx',
    '/replay': 'pages/Replay/ReplayShell.tsx',
    '/replay/:sessionId': 'pages/Replay/ReplayShell.tsx',
    '/replay/decision-trace': 'pages/Replay/ReplayShell.tsx',
    '/replay/:sessionId/decision-trace': 'pages/Replay/ReplayShell.tsx',
    '/replay/evidence': 'pages/Replay/ReplayShell.tsx',
    '/replay/:sessionId/evidence': 'pages/Replay/ReplayShell.tsx',
    '/replay/compare': 'pages/Replay/ReplayShell.tsx',
    '/replay/:sessionId/compare': 'pages/Replay/ReplayShell.tsx',
    '/replay/export': 'pages/Replay/ReplayShell.tsx',
    '/replay/:sessionId/export': 'pages/Replay/ReplayShell.tsx',
    '/security': 'components/LegacyRedirectNotice.tsx',
    '/security/policies': 'pages/PoliciesPage.tsx',
    '/security/audit': 'pages/AuditPage.tsx',
    '/security/compliance': 'pages/Security/ComplianceTab.tsx',
    '/security/evidence': 'pages/EvidencePage.tsx',
    '/admin': 'pages/Admin/AdminPage.tsx',
    '/admin/tenants': 'components/LegacyRedirectNotice.tsx',
    '/admin/tenants/:tenantId': 'components/LegacyRedirectNotice.tsx',
    '/admin/stacks': 'pages/Admin/AdapterStacksTab.tsx',
    '/admin/stacks/:stackId': 'pages/Admin/StackDetailModal.tsx',
    '/admin/plugins': 'pages/Admin/PluginsPage.tsx',
    '/admin/settings': 'pages/Admin/SettingsPage.tsx',
    '/reports': 'components/LegacyRedirectNotice.tsx',
    '/base-models': 'pages/BaseModelsPage.tsx',
    '/code-intelligence': 'components/LegacyRedirectNotice.tsx',
    '/metrics/advanced': 'components/LegacyRedirectNotice.tsx',
    '/help': 'components/LegacyRedirectNotice.tsx',
    '/router-config': 'pages/RouterConfigPage.tsx',
    '/federation': 'pages/FederationPage.tsx',
    '/dev/api-errors': 'pages/DevErrorsPage.tsx',
    '/dev/contracts': 'pages/Dev/ContractsPage.tsx',
    '/_dev/routes': 'pages/Dev/RoutesDebugPage.tsx',
  };

  // Exact match
  if (pathToFile[path]) return pathToFile[path];

  // Dynamic routes - infer from path structure
  if (path.startsWith('/admin/tenants')) return 'components/LegacyRedirectNotice.tsx';
  if (path.startsWith('/repos')) return 'pages/Repositories/RepositoriesShell.tsx';
  if (path.startsWith('/adapters')) return 'pages/Adapters/AdaptersShell.tsx';
  if (path.startsWith('/training')) return 'pages/Training/TrainingShell.tsx';
  if (path.startsWith('/replay')) return 'pages/Replay/ReplayShell.tsx';
  if (path.startsWith('/telemetry')) return 'pages/TelemetryPage.tsx';
  if (path.startsWith('/documents/')) return 'pages/DocumentLibrary/DocumentChatPage.tsx';

  return 'unknown';
}

/**
 * Detect issues with a route
 */
function detectIssues(
  route: RouteConfig,
  allRoutes: RouteConfig[],
  reachability: Reachability,
  status: RouteStatus
): string[] {
  const issues: string[] = [];

  // Orphan detection (improved)
  if (reachability === 'orphan') {
    issues.push('orphan: not reachable from nav or parent');
  }

  // Hidden but not deprecated
  if (reachability === 'hidden' && status === 'active') {
    issues.push('hidden: active route with no nav path');
  }

  // Deprecated routes still in sidebar
  if (status === 'deprecated' && route.navTitle && route.navGroup) {
    issues.push('deprecated: still in sidebar');
  }

  // Duplicate nav titles in same group
  if (route.navTitle && route.navGroup) {
    const dupes = allRoutes.filter(r =>
      r.path !== route.path &&
      r.navTitle === route.navTitle &&
      r.navGroup === route.navGroup
    );
    if (dupes.length > 0) {
      issues.push(`duplicate: same title in ${route.navGroup}`);
    }
  }

  // Similar paths (potential duplicates)
  const pathBase = route.path.replace(/Page$/, '').replace(/Tab$/, '');
  const similar = allRoutes.filter(r =>
    r.path !== route.path &&
    (r.path.replace(/Page$/, '').replace(/Tab$/, '') === pathBase ||
     r.path === route.path + 'Page')
  );
  if (similar.length > 0) {
    issues.push(`similar: ${similar.map(r => r.path).join(', ')}`);
  }

  // Missing parentPath for nested routes
  if (route.path.split('/').length > 2 && !route.parentPath && !route.path.includes(':')) {
    issues.push('missing: parentPath');
  }

  return issues;
}

/**
 * Generate notes about a route
 */
function generateNotes(route: RouteConfig, status: RouteStatus): string {
  const notes: string[] = [];

  if (status !== 'active') {
    notes.push(status);
  }
  if (route.requiredRoles?.includes('admin')) {
    notes.push('admin-only');
  }
  if (route.requiredPermissions?.length) {
    notes.push(`perms: ${route.requiredPermissions.join(', ')}`);
  }
  if (route.disabled) {
    notes.push('disabled');
  }

  return notes.join('; ');
}

/**
 * Hub page definitions with expected tabs
 * This is the source of truth for what tabs each hub SHOULD have
 */
export const HUB_DEFINITIONS: Record<string, {
  section: RouteSection;
  expectedTabs: string[];
  description: string;
}> = {
  '/adapters': {
    section: 'Adapters',
    expectedTabs: ['/adapters/new'],
    description: 'Adapter registry and management',
  },
  '/training': {
    section: 'Training',
    expectedTabs: ['/training/jobs', '/training/datasets', '/training/templates', '/training/artifacts', '/training/settings'],
    description: 'Training pipeline hub',
  },
  '/system': {
    section: 'System',
    expectedTabs: ['/system/nodes', '/system/workers', '/system/memory', '/system/metrics', '/system/pilot-status'],
    description: 'System infrastructure overview',
  },
  '/telemetry': {
    section: 'Monitor',
    expectedTabs: ['/telemetry/viewer', '/telemetry/alerts', '/telemetry/exports', '/telemetry/filters'],
    description: 'Telemetry hub (path-based tabs)',
  },
  '/admin': {
    section: 'Admin',
    expectedTabs: ['/admin/stacks', '/admin/plugins', '/admin/settings'],
    description: 'Administration and configuration',
  },
  '/documents': {
    section: 'Inference',
    expectedTabs: [],
    description: 'Document library and RAG',
  },
  '/security/policies': {
    section: 'Security',
    expectedTabs: [],
    description: 'Policy management',
  },
};

/**
 * Primary spine - MVP spine plus essential ops views
 */
export const PRIMARY_SPINE = [
  '/dashboard',
  '/workspaces',
  '/base-models',
  '/documents',
  '/training',
  '/chat',
  '/metrics',
  '/routing',
  '/system',
  '/telemetry',
  '/replay',
  '/security/policies',
] as const;

/**
 * Legacy Routes - track redirects for cleanup and observability
 */
export const LEGACY_ROUTES: Array<{ from: string; to: string; note?: string }> = [
  { from: '/owner', to: '/admin', note: 'owner home replaced by admin hub' },
  { from: '/management', to: '/dashboard', note: 'legacy management panel' },
  { from: '/workflow', to: '/training', note: 'training hub canonical' },
  { from: '/personas', to: '/dashboard', note: 'legacy personas tour' },
  { from: '/flow/lora', to: '/training', note: 'guided flow merged into training' },
  { from: '/trainer', to: '/training', note: 'legacy trainer folded into training hub' },
  { from: '/create-adapter', to: '/adapters#register', note: 'adapter registration lives in adapters shell' },
  { from: '/promotion', to: '/adapters', note: 'promotion flows live with adapters' },
  { from: '/monitoring', to: '/metrics', note: 'metrics/monitoring consolidation' },
  { from: '/reports', to: '/metrics', note: 'reports merged into metrics' },
  { from: '/code-intelligence', to: '/telemetry/viewer?source_type=code_intelligence', note: 'code intel rolls into telemetry' },
  { from: '/metrics/advanced', to: '/metrics', note: 'advanced metrics merged into main view' },
  { from: '/help', to: '/dashboard', note: 'help now within dashboard' },
  { from: '/admin/tenants', to: '/workspaces', note: 'workspace hub canonical' },
  { from: '/admin/tenants/:tenantId', to: '/workspaces', note: 'tenant details live in workspaces' },
  { from: '/telemetry/traces', to: '/telemetry/viewer', note: 'telemetry trace viewer consolidated' },
  { from: '/telemetry/traces/:traceId', to: '/telemetry/viewer/:traceId', note: 'trace deep links preserved' },
  { from: '/chat/sessions/:sessionId', to: '/chat?session=:sessionId', note: 'chat sessions now use query param' },
  { from: '/security', to: '/security/policies', note: 'guardrails canonical route' },
] as const;

/**
 * Build the complete route manifest
 */
export function buildRouteManifest(): RouteManifestEntry[] {
  return routes.map(route => {
    const section = inferSection(route);
    const type = inferType(route, routes);
    const status = STATUS_OVERRIDES[route.path] ?? 'active';
    const inSidebar = !!(route.navTitle && route.navGroup);
    const reachability = determineReachability(route, routes, inSidebar);
    const hubDef = HUB_DEFINITIONS[route.path];

    const actualTabs = findActualTabs(route, routes);
    const expectedTabs = hubDef?.expectedTabs ?? [];

    // Compare expected vs actual tabs
    const missingTabs = expectedTabs.filter(t => !actualTabs.includes(t) && !t.includes(':'));
    const extraTabs = actualTabs.filter(t => !expectedTabs.includes(t) && !t.includes(':'));

    const minRole = route.requiredRoles?.[0] ?? null;

    // Add hub-specific issues
    const issues = detectIssues(route, routes, reachability, status);
    if (hubDef && missingTabs.length > 0) {
      issues.push(`hub: missing tabs (${missingTabs.join(', ')})`);
    }
    if (hubDef && extraTabs.length > 0) {
      issues.push(`hub: unexpected tabs (${extraTabs.join(', ')})`);
    }

    return {
      path: route.path,
      navTitle: route.navTitle ?? null,
      navGroup: route.navGroup ?? null,
      section: hubDef?.section ?? section,
      type: hubDef ? 'hub' : type,
      status,
      isHub: !!hubDef || type === 'hub',
      inSidebar,
      reachability,
      tabs: actualTabs,
      expectedTabs,
      missingTabs,
      extraTabs,
      parentPath: route.parentPath ?? null,
      componentFile: inferComponentFile(route),
      minRole,
      modes: route.modes ?? [],
      issues,
      notes: generateNotes(route, status),
    };
  });
}

/**
 * Get cached manifest (memoized)
 */
let cachedManifest: RouteManifestEntry[] | null = null;

export function getRouteManifest(): RouteManifestEntry[] {
  if (!cachedManifest) {
    cachedManifest = buildRouteManifest();
  }
  return cachedManifest;
}

/** Clear cache (for testing) */
export function clearManifestCache(): void {
  cachedManifest = null;
}

/**
 * Analysis helpers
 */
export function getOrphanRoutes(): RouteManifestEntry[] {
  return getRouteManifest().filter(r => r.reachability === 'orphan');
}

export function getHiddenRoutes(): RouteManifestEntry[] {
  return getRouteManifest().filter(r => r.reachability === 'hidden');
}

export function getDeprecatedRoutes(): RouteManifestEntry[] {
  return getRouteManifest().filter(r => r.status === 'deprecated');
}

export function getDuplicateRoutes(): RouteManifestEntry[] {
  return getRouteManifest().filter(r =>
    r.issues.some(i => i.includes('duplicate') || i.includes('similar'))
  );
}

export function getHubRoutes(): RouteManifestEntry[] {
  return getRouteManifest().filter(r => r.isHub);
}

export function getHubsWithIssues(): RouteManifestEntry[] {
  return getRouteManifest().filter(r =>
    r.isHub && r.issues.some(i => i.startsWith('hub:'))
  );
}

export function getSidebarRoutes(): RouteManifestEntry[] {
  return getRouteManifest().filter(r => r.inSidebar);
}

export function getRoutesBySection(section: RouteSection): RouteManifestEntry[] {
  return getRouteManifest().filter(r => r.section === section);
}

export function getRoutesByType(type: RouteType): RouteManifestEntry[] {
  return getRouteManifest().filter(r => r.type === type);
}

export function getRoutesByReachability(reachability: Reachability): RouteManifestEntry[] {
  return getRouteManifest().filter(r => r.reachability === reachability);
}

export function getRoutesByStatus(status: RouteStatus): RouteManifestEntry[] {
  return getRouteManifest().filter(r => r.status === status);
}

/**
 * Summary statistics
 */
export interface ManifestStats {
  total: number;
  bySection: Record<RouteSection, number>;
  byType: Record<RouteType, number>;
  byStatus: Record<RouteStatus, number>;
  byReachability: Record<Reachability, number>;
  hubs: number;
  hubsWithIssues: number;
  inSidebar: number;
  orphans: number;
  hidden: number;
  duplicates: number;
  withIssues: number;
}

export function getManifestStats(): ManifestStats {
  const manifest = getRouteManifest();

  const bySection: Record<RouteSection, number> = {
    Core: 0, Adapters: 0, Training: 0, Inference: 0,
    System: 0, Monitor: 0, Security: 0, Admin: 0, Labs: 0,
  };

  const byType: Record<RouteType, number> = {
    hub: 0, detail: 0, tool: 0, landing: 0, future: 0,
  };

  const byStatus: Record<RouteStatus, number> = {
    active: 0, deprecated: 0,
  };

  const byReachability: Record<Reachability, number> = {
    primary: 0, nested: 0, hidden: 0, orphan: 0,
  };

  let hubs = 0;
  let hubsWithIssues = 0;
  let inSidebar = 0;
  let orphans = 0;
  let hidden = 0;
  let duplicates = 0;
  let withIssues = 0;

  for (const entry of manifest) {
    bySection[entry.section]++;
    byType[entry.type]++;
    byStatus[entry.status]++;
    byReachability[entry.reachability]++;

    if (entry.isHub) hubs++;
    if (entry.isHub && entry.issues.some(i => i.startsWith('hub:'))) hubsWithIssues++;
    if (entry.inSidebar) inSidebar++;
    if (entry.reachability === 'orphan') orphans++;
    if (entry.reachability === 'hidden') hidden++;
    if (entry.issues.some(i => i.includes('duplicate') || i.includes('similar'))) duplicates++;
    if (entry.issues.length > 0) withIssues++;
  }

  return {
    total: manifest.length,
    bySection,
    byType,
    byStatus,
    byReachability,
    hubs,
    hubsWithIssues,
    inSidebar,
    orphans,
    hidden,
    duplicates,
    withIssues,
  };
}

/**
 * Product flow definitions for validation
 */
export const PRODUCT_FLOWS = {
  'training-to-chat': {
    name: 'File upload → dataset → training → adapter → chat',
    steps: [
      { page: '/documents', action: 'Upload file' },
      { page: '/training/datasets', action: 'Create dataset from documents' },
      { page: '/training', action: 'Start training job' },
      { page: '/training/jobs/:jobId', action: 'Monitor progress' },
      { page: '/adapters/:adapterId', action: 'View trained adapter' },
      { page: '/chat', action: 'Use adapter in chat' },
    ],
  },
  'register-test-promote': {
    name: 'Register adapter → test → promote',
    steps: [
      { page: '/adapters/new', action: 'Register .aos file' },
      { page: '/adapters/:adapterId', action: 'View adapter details' },
      { page: '/testing', action: 'Run test suite' },
      { page: '/golden', action: 'Verify golden runs' },
      { page: '/promotion', action: 'Promote to production' },
    ],
  },
  'system-health-check': {
    name: 'System health → inference → routing',
    steps: [
      { page: '/system', action: 'Check system overview' },
      { page: '/monitoring', action: 'Verify health metrics' },
      { page: '/inference', action: 'Run inference playground' },
      { page: '/routing', action: 'Inspect routing decisions' },
    ],
  },
} as const;

/**
 * Validate a product flow against the manifest
 */
export function validateFlow(flowKey: keyof typeof PRODUCT_FLOWS): {
  valid: boolean;
  issues: Array<{ step: number; page: string; issue: string }>;
} {
  const flow = PRODUCT_FLOWS[flowKey];
  const manifest = getRouteManifest();
  const issues: Array<{ step: number; page: string; issue: string }> = [];

  flow.steps.forEach((step, index) => {
    // Check if route exists (accounting for params)
    const pathPattern = step.page.replace(/:[^/]+/g, '[^/]+');
    const regex = new RegExp(`^${pathPattern}$`);
    const matchingRoutes = manifest.filter(r => regex.test(r.path));

    if (matchingRoutes.length === 0) {
      issues.push({ step: index + 1, page: step.page, issue: 'route not found' });
    } else if (matchingRoutes.length > 1) {
      issues.push({
        step: index + 1,
        page: step.page,
        issue: `ambiguous: ${matchingRoutes.map(r => r.path).join(', ')}`,
      });
    } else {
      // Check route status
      const route = matchingRoutes[0];
      if (route.status === 'deprecated') {
        issues.push({ step: index + 1, page: step.page, issue: 'deprecated route' });
      }
      if (route.reachability === 'orphan') {
        issues.push({ step: index + 1, page: step.page, issue: 'orphan (unreachable)' });
      }
    }
  });

  return { valid: issues.length === 0, issues };
}

/**
 * Generate a summary report for console output
 */
export function generateSummaryReport(): string {
  const stats = getManifestStats();
  const manifest = getRouteManifest();

  const lines: string[] = [
    '=== ROUTE MANIFEST SUMMARY ===',
    '',
    `Total routes: ${stats.total}`,
    `  In sidebar: ${stats.inSidebar}`,
    `  Hubs: ${stats.hubs} (${stats.hubsWithIssues} with issues)`,
    `  With issues: ${stats.withIssues} (${Math.round(stats.withIssues / stats.total * 100)}%)`,
    '',
    '--- By Status ---',
    `  Active: ${stats.byStatus.active}`,
    `  Deprecated: ${stats.byStatus.deprecated}`,
    '',
    '--- By Reachability ---',
    `  Primary (sidebar): ${stats.byReachability.primary}`,
    `  Nested: ${stats.byReachability.nested}`,
    `  Hidden: ${stats.byReachability.hidden}`,
    `  Orphan: ${stats.byReachability.orphan}`,
    '',
    '--- By Section ---',
    ...Object.entries(stats.bySection)
      .filter(([, v]) => v > 0)
      .sort((a, b) => b[1] - a[1])
      .map(([k, v]) => `  ${k}: ${v}`),
    '',
    '--- Issues ---',
    ...manifest
      .filter(r => r.issues.length > 0)
      .map(r => `  ${r.path}: ${r.issues.join(', ')}`),
  ];

  return lines.join('\n');
}
