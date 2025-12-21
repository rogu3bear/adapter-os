import { createElement, lazy } from 'react';
import { lazyRouteableNamed } from './route-types';
import { useLocation, useParams } from 'react-router-dom';
import LegacyRedirectNotice from '@/components/LegacyRedirectNotice';
import { lazyWithRetry } from '@/utils/lazyWithRetry';
import { buildTelemetryEventStreamLink, buildTelemetryTraceLink, buildTelemetryViewerLink } from '@/utils/navLinks';
import type { UserRole } from '@/api/types';
import type { LucideIcon } from 'lucide-react';
import { UiMode } from './ui-mode';
import {
  LayoutDashboard,
  Compass,
  Upload,
  Zap,
  FlaskConical,
  GitCompare,
  TrendingUp,
  Box,
  Activity,
  Route,
  Play,
  Eye,
  RotateCcw,
  Shield,
  FileText,
  FileOutput,
  Settings,
  BarChart3,
  Building,
  Users,
  Grid3x3,
  Server,
  Cpu,
  MemoryStick,
  Database,
  Layers,
  Plug,
  CheckCircle,
  FileCode,
  GitBranch,
  PlusCircle,
  MessageSquare,
  Crown,
  Network,
  Bug,
  Map,
  Briefcase,
  Package,
  Bell,
  Download,
  Filter,
} from 'lucide-react';

// Lazy-loaded page components for code splitting
const OwnerHomePage = lazy(() => import('@/pages/OwnerHome'));
const DashboardPage = lazy(() => import('@/pages/DashboardPage'));
const TenantsPage = lazy(() => import('@/pages/TenantsPage'));
const TenantDetailRoutePage = lazy(() => import('@/pages/Admin/TenantDetailPage'));
const StackDetailRoutePage = lazy(() => import('@/pages/Admin/StackDetailModal'));
const AdaptersPage = lazy(() => import('@/pages/AdaptersPage'));
const AdapterDetailPage = lazy(() => import('@/pages/Adapters/AdapterDetailPage'));
const AdapterRegisterPage = lazy(() => import('@/pages/Adapters/AdapterRegisterPage'));
const AdaptersShellPage = lazyWithRetry(() => import('@/pages/Adapters/AdaptersShell'));
const PoliciesPage = lazy(() => import('@/pages/PoliciesPage'));
const MetricsPage = lazy(() => import('@/pages/MetricsPage'));
const InferencePage = lazy(() => import('@/pages/InferencePage'));
const ChatPage = lazy(() => import('@/pages/ChatPage'));
const AuditPage = lazy(() => import('@/pages/AuditPage'));
const RepositoriesShellPage = lazy(() => import('@/pages/Repositories/RepositoriesShell'));
const CompliancePage = lazyRouteableNamed(() => import('@/pages/Security/ComplianceTab'), 'ComplianceTab');
const EvidencePage = lazy(() => import('@/pages/EvidencePage'));
const BaseModelsPage = lazy(() => import('@/pages/BaseModelsPage'));
const WorkflowPage = lazy(() => import('@/pages/WorkflowPage'));
const TrainingPage = lazy(() => import('@/pages/Training/TrainingPage'));
const TrainingJobsPage = lazy(() => import('@/pages/Training/TrainingJobsPage'));
const TrainingJobDetailPage = lazy(() => import('@/pages/Training/TrainingJobDetail'));
const TrainingDatasetsPage = lazyRouteableNamed(() => import('@/pages/Training/DatasetsTab'), 'DatasetsTab');
const DatasetDetailPage = lazy(() => import('@/pages/Training/DatasetDetailPage'));
const DatasetChatPage = lazy(() => import('@/pages/Training/DatasetChatPage'));
const ResultChatPage = lazy(() => import('@/pages/Training/ResultChatPage'));
const TrainingTemplatesPage = lazyRouteableNamed(() => import('@/pages/Training/TemplatesTab'), 'TemplatesTab');
const TrainingShellPage = lazy(() => import('@/pages/Training/TrainingShell'));
const CreateAdapterPage = lazy(() => import('@/pages/CreateAdapterPage'));
const TestingPage = lazy(() => import('@/pages/TestingPage'));
const GoldenPage = lazy(() => import('@/pages/GoldenPage'));
const PromotionPage = lazy(() => import('@/pages/PromotionPage'));
const RoutingPage = lazy(() => import('@/pages/RoutingPage'));
const ReplayShellPage = lazy(() => import('@/pages/Replay/ReplayShell'));
const AdminPage = lazy(() => import('@/pages/AdminPage'));
const AdminStacksPage = lazyRouteableNamed(() => import('@/pages/Admin/AdapterStacksTab'), 'AdapterStacksTab');
const AdminPluginsPage = lazy(() => import('@/pages/Admin/PluginsPage'));
const AdminSettingsPage = lazy(() => import('@/pages/Admin/SettingsPage'));
const TrainerPage = lazy(() => import('@/pages/TrainerPage'));
const PersonasPage = lazy(() => import('@/pages/PersonasPage'));
const ManagementPage = lazy(() => import('@/pages/ManagementPage'));
const SystemOverviewPage = lazy(() => import('@/pages/System/SystemOverviewPage'));
const SystemNodesPage = lazy(() => import('@/pages/System/NodesTab'));
const NodeDetailRoutePage = lazy(() => import('@/pages/System/NodeDetailModal'));
const SystemWorkersPage = lazy(() => import('@/pages/System/WorkersTab'));
const SystemMemoryPage = lazy(() => import('@/pages/System/MemoryTab'));
const SystemMetricsPage = lazy(() => import('@/pages/System/MetricsTab'));
const PilotStatusPage = lazy(() => import('@/pages/System/PilotStatusPage'));
const GuidedFlowPage = lazy(() => import('@/pages/GuidedFlowPage'));
const DocumentLibraryPage = lazy(() => import('@/pages/DocumentLibrary'));
const DocumentChatPage = lazy(() => import('@/pages/DocumentLibrary/DocumentChatPage'));
const RouterConfigPage = lazy(() => import('@/pages/RouterConfigPage'));
const FederationPage = lazy(() => import('@/pages/FederationPage'));
const DevErrorsPage = lazy(() => import('@/pages/DevErrorsPage'));
const DevContractsPage = lazy(() => import('@/pages/Dev/ContractsPage'));
const RoutesDebugPage = lazy(() => import('@/pages/Dev/RoutesDebugPage'));
const TelemetryPage = lazy(() => import('@/pages/TelemetryPage'));

const redirectTo = (to: string, label?: string) => () => createElement(LegacyRedirectNotice, { to, label });

const redirectTelemetry = (tab: 'events' | 'traces' | 'viewer', includeTraceId = false) =>
  () => createElement(TelemetryRedirect, { tab, includeTraceId });

const redirectChatSession = () => () => createElement(ChatSessionRedirect);

function ChatSessionRedirect() {
  const { sessionId } = useParams();
  const location = useLocation();
  const searchParams = new URLSearchParams(location.search);

  if (sessionId) {
    searchParams.set('session', sessionId);
  }

  const target = `/chat${searchParams.toString() ? `?${searchParams.toString()}` : ''}`;
  return createElement(LegacyRedirectNotice, { to: target, label: 'Chat' });
}

function TelemetryRedirect({
  tab,
  includeTraceId = false,
}: {
  tab: 'events' | 'traces' | 'viewer';
  includeTraceId?: boolean;
}) {
  const { traceId } = useParams();
  const location = useLocation();
  const searchParams = new URLSearchParams(location.search);

  const sourceType = (searchParams.get('source_type') ?? searchParams.get('sourceType') ?? '').trim() || undefined;
  const target = (() => {
    switch (tab) {
      case 'events':
        return buildTelemetryEventStreamLink({ sourceType });
      case 'viewer':
      case 'traces':
        if (includeTraceId && traceId) {
          return buildTelemetryTraceLink(traceId, { sourceType });
        }
        return buildTelemetryViewerLink({ sourceType });
      default:
        return buildTelemetryEventStreamLink({ sourceType });
    }
  })();

  return createElement(LegacyRedirectNotice, { to: target, label: 'Telemetry' });
}

export type RouteCluster = 'Build' | 'Run' | 'Observe' | 'Verify';

/**
 * A routeable component type - a component that can be rendered without props.
 * RouteGuard renders components as `<Component />` with no props, so all route
 * components must have no required props.
 *
 * DO NOT use `as any` or `as React.ComponentType<any>` to bypass this constraint.
 * If you have a component with required props (e.g., a modal), create a `*RoutePage`
 * wrapper that reads params from the URL and fetches necessary data.
 */
type RouteComponent = React.LazyExoticComponent<React.ComponentType<object>> | React.ComponentType<object>;

export interface RouteConfig {
  path: string;
  /**
   * The component to render for this route.
   *
   * IMPORTANT: This component must be callable with NO PROPS (`<Component />`).
   * Modal components or components with required props (like `open`, `onClose`,
   * `tenant`, etc.) should NEVER be used directly here.
   *
   * Instead, create a route-safe wrapper (e.g., `TenantDetailRoutePage`) that:
   * 1. Reads params from URL via `useParams()`
   * 2. Fetches required data via hooks
   * 3. Renders the modal/component with proper props
   *
   * @see TenantDetailRoutePage for an example of this pattern.
   */
  component: RouteComponent;
  requiresAuth?: boolean;
  /**
   * Controls which roles can ACCESS this route (enforced access control).
   *
   * Blocks both navigation AND direct URL access. Users without the required role
   * will be redirected to an unauthorized page or fallback route.
   *
   * When to use:
   * - For routes that should be completely inaccessible to certain roles
   * - When you need to enforce security boundaries (e.g., admin-only settings)
   *
   * @example
   * // Admin-only route - operators cannot access even with direct URL
   * {
   *   path: '/admin/settings',
   *   component: AdminSettingsPage,
   *   requiredRoles: ['admin'],
   *   roleVisibility: ['admin'],
   * }
   */
  requiredRoles?: UserRole[];
  requiredPermissions?: string[];
  navGroup?: string;
  navTitle?: string;
  navIcon?: LucideIcon;
  navOrder?: number;
  disabled?: boolean;
  external?: boolean;
  skeletonVariant?: 'default' | 'dashboard' | 'table' | 'form';
  breadcrumb?: string;
  parentPath?: string;
  cluster: RouteCluster;
  /**
   * Controls which roles can SEE this route in navigation (UI visibility).
   *
   * Does NOT block direct URL access. Only affects whether the route appears in
   * sidebars, menus, and breadcrumbs. Users can still access via direct URL.
   *
   * When to use:
   * - To declutter navigation for roles that shouldn't see certain features
   * - When you want to hide routes from UI but allow URL access (e.g., shared links)
   * - For progressive disclosure based on user role
   *
   * Best practice: For sensitive routes, set BOTH roleVisibility AND requiredRoles
   * to the same value for consistent visibility and access control.
   *
   * @example
   * // Route hidden from viewers but accessible via URL (e.g., shared links)
   * {
   *   path: '/inference',
   *   component: InferencePage,
   *   roleVisibility: ['admin', 'operator'],
   *   // No requiredRoles - viewers can access via direct URL
   * }
   *
   * @example
   * // Admin-only route - consistent visibility and access
   * {
   *   path: '/admin/settings',
   *   component: AdminSettingsPage,
   *   requiredRoles: ['admin'],
   *   roleVisibility: ['admin'], // Both set for consistency
   * }
   */
  roleVisibility?: UserRole[];
  modes?: UiMode[];
}

/**
 * LEGACY REDIRECTS - Candidates for removal
 * ==========================================
 * The following routes are legacy redirects that should be removed after
 * confirming no external links depend on them:
 *
 * | Path              | Redirects To    | Notes                          |
 * |-------------------|-----------------|--------------------------------|
 * | /owner            | /admin          | Legacy owner home              |
 * | /management       | /dashboard      | Legacy management panel        |
 * | /workflow         | /training       | Legacy onboarding              |
 * | /personas         | /dashboard      | Legacy personas tour           |
 * | /flow/lora        | /training       | Legacy guided setup            |
 * | /trainer          | /training       | Legacy quick trainer           |
 * | /create-adapter   | /adapters#register | Legacy adapter creation     |
 * | /promotion        | /adapters       | Legacy promotion flow          |
 * | /monitoring       | /metrics        | Merged into metrics            |
 * | /reports          | /metrics        | Merged into metrics            |
 * | /code-intelligence| /telemetry      | Moved to telemetry viewer      |
 * | /metrics/advanced | /metrics        | Consolidated into main metrics |
 * | /help             | /dashboard      | Legacy help center             |
 *
 * Audit date: 2025-12-19
 * TODO: Track usage analytics before removing these routes
 */
export const routes: RouteConfig[] = [
  {
    path: '/owner',
    component: redirectTo('/admin', 'Admin'),
    requiresAuth: true,
    requiredRoles: ['admin'],
    skeletonVariant: 'dashboard',
    breadcrumb: 'Owner Home (Legacy)',
    cluster: 'Verify',
    roleVisibility: ['admin'],
    modes: [],
  },
  {
    path: '/dashboard',
    component: DashboardPage,
    requiresAuth: true,
    navGroup: 'Run',
    navTitle: 'Dashboard',
    navIcon: LayoutDashboard,
    navOrder: 0,
    skeletonVariant: 'dashboard',
    breadcrumb: 'Dashboard',
    cluster: 'Run',
    roleVisibility: ['admin', 'operator', 'sre', 'compliance', 'auditor', 'viewer'],
    modes: [UiMode.User],
  },
  {
    // LEGACY: management panel retained for compatibility; hidden from nav
    path: '/management',
    component: redirectTo('/dashboard', 'Dashboard'),
    requiresAuth: true,
    skeletonVariant: 'dashboard',
    breadcrumb: 'Management',
    cluster: 'Build',
    roleVisibility: ['admin'],
    modes: [],
  },
  {
    path: '/workflow',
    component: redirectTo('/training', 'Training'),
    requiresAuth: true,
    breadcrumb: 'Onboarding',
    cluster: 'Build',
    roleVisibility: ['admin'],
    modes: [],
  },
  {
    // LEGACY: personas tour retained; hidden from nav
    path: '/personas',
    component: redirectTo('/dashboard', 'Dashboard'),
    requiresAuth: false,
    skeletonVariant: 'default',
    breadcrumb: 'Personas',
    cluster: 'Build',
    roleVisibility: ['admin'],
    modes: [],
  },
  {
    // LEGACY: guided flow retained; hidden from nav
    path: '/flow/lora',
    component: redirectTo('/training', 'Training'),
    requiresAuth: true,
    skeletonVariant: 'default',
    breadcrumb: 'Guided Setup',
    cluster: 'Build',
    roleVisibility: ['admin'],
    modes: [],
  },
  {
    path: '/repos',
    component: RepositoriesShellPage,
    requiresAuth: true,
    requiredRoles: ['admin', 'operator'],
    navGroup: 'Build',
    navTitle: 'Repositories',
    navIcon: GitBranch,
    navOrder: 0,
    skeletonVariant: 'table',
    breadcrumb: 'Repositories',
    cluster: 'Build',
    roleVisibility: ['admin', 'operator'],
    modes: [UiMode.Builder],
  },
  {
    path: '/repos/:repoId',
    component: RepositoriesShellPage,
    requiresAuth: true,
    skeletonVariant: 'default',
    breadcrumb: 'Repository Detail',
    parentPath: '/repos',
    cluster: 'Build',
    roleVisibility: ['admin', 'operator'],
  },
  {
    path: '/repos/:repoId/versions/:versionId',
    component: RepositoriesShellPage,
    requiresAuth: true,
    skeletonVariant: 'default',
    breadcrumb: 'Version Detail',
    parentPath: '/repos/:repoId',
    cluster: 'Build',
    roleVisibility: ['admin', 'operator'],
  },
  {
    // LEGACY: quick training retained; hidden from nav
    path: '/trainer',
    component: redirectTo('/training', 'Training'),
    requiresAuth: true,
    skeletonVariant: 'form',
    breadcrumb: 'Trainer',
    cluster: 'Build',
    roleVisibility: ['admin', 'operator'],
    modes: [],
  },
  {
    path: '/create-adapter',
    component: redirectTo('/adapters#register', 'Adapters'),
    requiresAuth: true,
    requiredPermissions: ['adapter:register', 'training:start'],
    skeletonVariant: 'form',
    breadcrumb: 'Create Adapter',
    cluster: 'Build',
    roleVisibility: ['admin', 'operator'],
  },
  {
    path: '/training',
    component: TrainingShellPage,
    requiresAuth: true,
    requiredRoles: ['admin', 'operator'],
    navGroup: 'Build',
    navTitle: 'Training',
    navIcon: Zap,
    navOrder: 2,
    skeletonVariant: 'table',
    breadcrumb: 'Training',
    cluster: 'Build',
    roleVisibility: ['admin', 'operator'],
    modes: [UiMode.Builder],
  },
  {
    path: '/training/jobs',
    component: TrainingShellPage,
    requiresAuth: true,
    navTitle: 'Jobs',
    navIcon: Briefcase,
    skeletonVariant: 'table',
    breadcrumb: 'Jobs',
    parentPath: '/training',
    cluster: 'Build',
    roleVisibility: ['admin', 'operator'],
  },
  {
    path: '/training/jobs/:jobId',
    component: TrainingShellPage,
    requiresAuth: true,
    skeletonVariant: 'default',
    breadcrumb: 'Job Detail',
    parentPath: '/training/jobs',
    cluster: 'Build',
    roleVisibility: ['admin', 'operator'],
  },
  {
    path: '/training/jobs/:jobId/chat',
    component: ResultChatPage,
    requiresAuth: true,
    skeletonVariant: 'form',
    breadcrumb: 'Result Chat',
    parentPath: '/training/jobs/:jobId',
    cluster: 'Run',
    roleVisibility: ['admin', 'operator'],
  },
  {
    path: '/training/datasets',
    component: TrainingShellPage,
    requiresAuth: true,
    navTitle: 'Datasets',
    navIcon: Database,
    skeletonVariant: 'table',
    breadcrumb: 'Datasets',
    parentPath: '/training',
    cluster: 'Build',
    roleVisibility: ['admin', 'operator'],
  },
  {
    path: '/training/datasets/:datasetId',
    component: TrainingShellPage,
    requiresAuth: true,
    skeletonVariant: 'default',
    breadcrumb: 'Dataset Detail',
    parentPath: '/training/datasets',
    cluster: 'Build',
    roleVisibility: ['admin', 'operator'],
  },
  {
    path: '/training/datasets/:datasetId/chat',
    component: DatasetChatPage,
    requiresAuth: true,
    skeletonVariant: 'form',
    breadcrumb: 'Dataset Chat',
    parentPath: '/training/datasets/:datasetId',
    cluster: 'Run',
    roleVisibility: ['admin', 'operator'],
  },
  {
    path: '/training/templates',
    component: TrainingShellPage,
    requiresAuth: true,
    navTitle: 'Templates',
    navIcon: FileCode,
    skeletonVariant: 'table',
    breadcrumb: 'Templates',
    parentPath: '/training',
    cluster: 'Build',
    roleVisibility: ['admin', 'operator'],
  },
  {
    path: '/training/artifacts',
    component: TrainingShellPage,
    requiresAuth: true,
    navTitle: 'Artifacts',
    navIcon: Package,
    skeletonVariant: 'default',
    breadcrumb: 'Artifacts',
    parentPath: '/training',
    cluster: 'Build',
    roleVisibility: ['admin', 'operator'],
  },
  {
    path: '/training/settings',
    component: TrainingShellPage,
    requiresAuth: true,
    navTitle: 'Settings',
    navIcon: Settings,
    skeletonVariant: 'form',
    breadcrumb: 'Settings',
    parentPath: '/training',
    cluster: 'Build',
    roleVisibility: ['admin', 'operator'],
  },
  {
    path: '/testing',
    component: TestingPage,
    requiresAuth: true,
    navGroup: 'Verify',
    navTitle: 'Testing',
    navIcon: FlaskConical,
    navOrder: 3,
    skeletonVariant: 'default',
    breadcrumb: 'Testing',
    cluster: 'Verify',
    roleVisibility: ['admin', 'operator', 'sre'],
    modes: [UiMode.Builder],
  },
  {
    path: '/golden',
    component: GoldenPage,
    requiresAuth: true,
    navGroup: 'Verify',
    navTitle: 'Verified Runs',
    navIcon: GitCompare,
    navOrder: 4,
    skeletonVariant: 'table',
    breadcrumb: 'Verified Runs',
    cluster: 'Verify',
    roleVisibility: ['admin', 'operator', 'compliance', 'auditor'],
    modes: [UiMode.Builder],
  },
  {
    // LEGACY: promotion flow retained; hidden from nav
    path: '/promotion',
    component: redirectTo('/adapters', 'Adapters'),
    requiresAuth: true,
    skeletonVariant: 'default',
    breadcrumb: 'Promotion',
    cluster: 'Build',
    roleVisibility: ['admin', 'operator'],
    modes: [],
  },
  {
    path: '/adapters',
    component: AdaptersShellPage,
    requiresAuth: true,
    requiredRoles: ['admin', 'operator'],
    navGroup: 'Build',
    navTitle: 'Adapters',
    navIcon: Box,
    navOrder: 1,
    skeletonVariant: 'table',
    breadcrumb: 'Adapters',
    cluster: 'Build',
    roleVisibility: ['admin', 'operator'],
    modes: [UiMode.Builder],
  },
  {
    path: '/adapters/new',
    component: AdaptersShellPage,
    requiresAuth: true,
    requiredPermissions: ['adapter:register'],
    skeletonVariant: 'form',
    breadcrumb: 'Register New Adapter',
    parentPath: '/adapters',
    cluster: 'Build',
    roleVisibility: ['admin', 'operator'],
  },
  {
    path: '/adapters/:adapterId',
    component: AdaptersShellPage,
    requiresAuth: true,
    skeletonVariant: 'default',
    breadcrumb: 'Adapter Detail',
    parentPath: '/adapters',
    cluster: 'Build',
    roleVisibility: ['admin', 'operator'],
  },
  {
    path: '/adapters/:adapterId/activations',
    component: AdaptersShellPage,
    requiresAuth: true,
    skeletonVariant: 'table',
    breadcrumb: 'Activations',
    parentPath: '/adapters/:adapterId',
    cluster: 'Build',
    roleVisibility: ['admin', 'operator'],
  },
  {
    path: '/adapters/:adapterId/usage',
    component: AdaptersShellPage,
    requiresAuth: true,
    skeletonVariant: 'default',
    breadcrumb: 'Usage',
    parentPath: '/adapters/:adapterId',
    cluster: 'Build',
    roleVisibility: ['admin', 'operator'],
  },
  {
    path: '/adapters/:adapterId/lineage',
    component: AdaptersShellPage,
    requiresAuth: true,
    skeletonVariant: 'default',
    breadcrumb: 'Lineage',
    parentPath: '/adapters/:adapterId',
    cluster: 'Build',
    roleVisibility: ['admin', 'operator'],
  },
  {
    path: '/adapters/:adapterId/manifest',
    component: AdaptersShellPage,
    requiresAuth: true,
    skeletonVariant: 'default',
    breadcrumb: 'Manifest',
    parentPath: '/adapters/:adapterId',
    cluster: 'Build',
    roleVisibility: ['admin', 'operator'],
  },
  {
    path: '/adapters/:adapterId/policies',
    component: AdaptersShellPage,
    requiresAuth: true,
    skeletonVariant: 'default',
    breadcrumb: 'Policies',
    parentPath: '/adapters/:adapterId',
    cluster: 'Build',
    roleVisibility: ['admin', 'operator'],
  },
  {
    path: '/metrics',
    component: MetricsPage,
    requiresAuth: true,
    navGroup: 'Observe',
    navTitle: 'Metrics',
    navIcon: Activity,
    navOrder: 1,
    skeletonVariant: 'dashboard',
    breadcrumb: 'Metrics',
    cluster: 'Observe',
    roleVisibility: ['admin', 'operator', 'sre', 'viewer'],
    modes: [UiMode.User],
  },
  {
    path: '/monitoring',
    component: redirectTo('/metrics', 'Metrics'),
    requiresAuth: true,
    skeletonVariant: 'dashboard',
    breadcrumb: 'System Health',
    cluster: 'Observe',
    roleVisibility: ['admin', 'operator', 'sre', 'viewer'],
    modes: [],
  },
  {
    path: '/routing',
    component: RoutingPage,
    requiresAuth: true,
    navGroup: 'Observe',
    navTitle: 'Routing History',
    navIcon: Route,
    navOrder: 2,
    skeletonVariant: 'default',
    breadcrumb: 'Routing History',
    cluster: 'Observe',
    roleVisibility: ['admin', 'operator', 'sre'],
    modes: [UiMode.User],
  },
  {
    path: '/system',
    component: SystemOverviewPage,
    requiresAuth: true,
    navGroup: 'Observe',
    navTitle: 'System Overview',
    navIcon: Server,
    navOrder: 3,
    skeletonVariant: 'dashboard',
    breadcrumb: 'System',
    cluster: 'Observe',
    roleVisibility: ['admin', 'operator', 'sre'],
    modes: [UiMode.User],
  },
  {
    path: '/system/nodes',
    component: SystemNodesPage,
    requiresAuth: true,
    navGroup: 'Observe',
    navTitle: 'Nodes',
    navIcon: Cpu,
    navOrder: 2,
    skeletonVariant: 'table',
    breadcrumb: 'Nodes',
    parentPath: '/system',
    cluster: 'Observe',
    roleVisibility: ['admin', 'operator', 'sre'],
    modes: [UiMode.User],
  },
  {
    path: '/system/nodes/:nodeId',
    component: NodeDetailRoutePage,
    requiresAuth: true,
    skeletonVariant: 'default',
    breadcrumb: 'Node Detail',
    parentPath: '/system/nodes',
    cluster: 'Observe',
    roleVisibility: ['admin', 'operator', 'sre'],
  },
  {
    path: '/system/workers',
    component: SystemWorkersPage,
    requiresAuth: true,
    navGroup: 'Observe',
    navTitle: 'Workers',
    navIcon: Users,
    navOrder: 3,
    skeletonVariant: 'table',
    breadcrumb: 'Workers',
    parentPath: '/system',
    cluster: 'Observe',
    roleVisibility: ['admin', 'operator', 'sre'],
    modes: [UiMode.User],
  },
  {
    path: '/system/memory',
    component: SystemMemoryPage,
    requiresAuth: true,
    navGroup: 'Observe',
    navTitle: 'Memory',
    navIcon: MemoryStick,
    navOrder: 4,
    skeletonVariant: 'dashboard',
    breadcrumb: 'Memory',
    parentPath: '/system',
    cluster: 'Observe',
    roleVisibility: ['admin', 'operator', 'sre'],
    modes: [UiMode.User],
  },
  {
    path: '/system/metrics',
    component: SystemMetricsPage,
    requiresAuth: true,
    navGroup: 'Observe',
    navTitle: 'System Metrics',
    navIcon: BarChart3,
    navOrder: 5,
    skeletonVariant: 'dashboard',
    breadcrumb: 'Metrics',
    parentPath: '/system',
    cluster: 'Observe',
    roleVisibility: ['admin', 'operator', 'sre'],
    modes: [UiMode.User],
  },
  {
    path: '/system/pilot-status',
    component: PilotStatusPage,
    requiresAuth: true,
    navGroup: 'Observe',
    navTitle: 'Pilot Status',
    navIcon: CheckCircle,
    navOrder: 6,
    skeletonVariant: 'dashboard',
    breadcrumb: 'Pilot Status',
    parentPath: '/system',
    cluster: 'Observe',
    roleVisibility: ['admin', 'operator', 'sre'],
    modes: [UiMode.User],
  },
  {
    path: '/inference',
    component: InferencePage,
    requiresAuth: true,
    requiredPermissions: ['inference:execute'],
    navGroup: 'Run',
    navTitle: 'Inference',
    navIcon: Play,
    navOrder: 1,
    skeletonVariant: 'form',
    breadcrumb: 'Inference',
    cluster: 'Run',
    roleVisibility: ['admin', 'operator', 'sre'],
    modes: [UiMode.User],
  },
  {
    path: '/chat',
    component: ChatPage,
    requiresAuth: true,
    requiredRoles: ['admin', 'operator'],
    navGroup: 'Run',
    navTitle: 'Chat',
    navIcon: MessageSquare,
    navOrder: 2,
    skeletonVariant: 'form',
    breadcrumb: 'Chat',
    cluster: 'Run',
    roleVisibility: ['admin', 'operator'],
    modes: [UiMode.User],
  },
  {
    path: '/chat/sessions/:sessionId',
    component: redirectChatSession(),
    requiresAuth: true,
    skeletonVariant: 'form',
    breadcrumb: 'Chat Session',
    parentPath: '/chat',
    cluster: 'Run',
    roleVisibility: ['admin', 'operator'],
    modes: [UiMode.User],
  },
  {
    path: '/documents',
    component: DocumentLibraryPage,
    requiresAuth: true,
    requiredRoles: ['admin', 'operator'],
    navGroup: 'Run',
    navTitle: 'Documents',
    navIcon: FileText,
    navOrder: 3,
    skeletonVariant: 'table',
    breadcrumb: 'Documents',
    cluster: 'Run',
    roleVisibility: ['admin', 'operator'],
    modes: [UiMode.User],
  },
  {
    path: '/documents/:documentId/chat',
    component: DocumentChatPage,
    requiresAuth: true,
    skeletonVariant: 'form',
    breadcrumb: 'Document Chat',
    parentPath: '/documents',
    cluster: 'Run',
    roleVisibility: ['admin', 'operator'],
  },
  {
    path: '/telemetry',
    component: TelemetryPage,
    requiresAuth: true,
    navGroup: 'Observe',
    navTitle: 'Event History',
    navIcon: Eye,
    navOrder: 4,
    skeletonVariant: 'table',
    breadcrumb: 'Event History',
    cluster: 'Observe',
    roleVisibility: ['admin', 'operator', 'sre', 'compliance', 'auditor'],
    modes: [UiMode.Audit],
  },
  {
    path: '/telemetry/viewer',
    component: TelemetryPage,
    requiresAuth: true,
    navTitle: 'Viewer',
    navIcon: Eye,
    skeletonVariant: 'table',
    breadcrumb: 'Telemetry Viewer',
    parentPath: '/telemetry',
    cluster: 'Observe',
    roleVisibility: ['admin', 'operator', 'sre', 'compliance', 'auditor'],
    modes: [UiMode.Audit],
  },
  {
    path: '/telemetry/viewer/:traceId',
    component: TelemetryPage,
    requiresAuth: true,
    skeletonVariant: 'table',
    breadcrumb: 'Telemetry Viewer',
    parentPath: '/telemetry',
    cluster: 'Observe',
    roleVisibility: ['admin', 'operator', 'sre', 'compliance', 'auditor'],
    modes: [UiMode.Audit],
  },
  {
    path: '/telemetry/traces',
    component: redirectTelemetry('traces'),
    requiresAuth: true,
    skeletonVariant: 'table',
    breadcrumb: 'Trace Viewer',
    parentPath: '/telemetry',
    cluster: 'Observe',
    roleVisibility: ['admin', 'operator', 'sre', 'compliance', 'auditor'],
    modes: [UiMode.Audit],
  },
  {
    path: '/telemetry/traces/:traceId',
    component: redirectTelemetry('traces', true),
    requiresAuth: true,
    skeletonVariant: 'table',
    breadcrumb: 'Trace Viewer',
    parentPath: '/telemetry',
    cluster: 'Observe',
    roleVisibility: ['admin', 'operator', 'sre', 'compliance', 'auditor'],
    modes: [UiMode.Audit],
  },
  {
    path: '/telemetry/alerts',
    component: TelemetryPage,
    requiresAuth: true,
    navTitle: 'Alerts',
    navIcon: Bell,
    skeletonVariant: 'table',
    breadcrumb: 'Alerts',
    parentPath: '/telemetry',
    cluster: 'Observe',
    roleVisibility: ['admin', 'operator', 'sre', 'compliance', 'auditor'],
    modes: [UiMode.Audit],
  },
  {
    path: '/telemetry/exports',
    component: TelemetryPage,
    requiresAuth: true,
    navTitle: 'Exports',
    navIcon: Download,
    skeletonVariant: 'default',
    breadcrumb: 'Exports',
    parentPath: '/telemetry',
    cluster: 'Observe',
    roleVisibility: ['admin', 'operator', 'sre', 'compliance', 'auditor'],
    modes: [UiMode.Audit],
  },
  {
    path: '/telemetry/filters',
    component: TelemetryPage,
    requiresAuth: true,
    navTitle: 'Filters',
    navIcon: Filter,
    skeletonVariant: 'form',
    breadcrumb: 'Filters',
    parentPath: '/telemetry',
    cluster: 'Observe',
    roleVisibility: ['admin', 'operator', 'sre', 'compliance', 'auditor'],
    modes: [UiMode.Audit],
  },
  {
    path: '/replay',
    component: ReplayShellPage,
    requiresAuth: true,
    navGroup: 'Verify',
    navTitle: 'Run History',
    navIcon: RotateCcw,
    navOrder: 5,
    skeletonVariant: 'default',
    breadcrumb: 'Run History',
    cluster: 'Verify',
    roleVisibility: ['admin', 'operator', 'sre', 'compliance', 'auditor'],
    modes: [UiMode.Audit],
  },
  {
    path: '/replay/:sessionId',
    component: ReplayShellPage,
    requiresAuth: true,
    skeletonVariant: 'default',
    breadcrumb: 'Session Detail',
    parentPath: '/replay',
    cluster: 'Verify',
    roleVisibility: ['admin', 'operator', 'sre', 'compliance', 'auditor'],
    modes: [UiMode.Audit],
  },
  {
    path: '/replay/decision-trace',
    component: ReplayShellPage,
    requiresAuth: true,
    skeletonVariant: 'default',
    breadcrumb: 'Decision Trace',
    parentPath: '/replay',
    cluster: 'Verify',
    roleVisibility: ['admin', 'operator', 'sre', 'compliance', 'auditor'],
    modes: [UiMode.Audit],
  },
  {
    path: '/replay/:sessionId/decision-trace',
    component: ReplayShellPage,
    requiresAuth: true,
    skeletonVariant: 'default',
    breadcrumb: 'Decision Trace',
    parentPath: '/replay/:sessionId',
    cluster: 'Verify',
    roleVisibility: ['admin', 'operator', 'sre', 'compliance', 'auditor'],
    modes: [UiMode.Audit],
  },
  {
    path: '/replay/evidence',
    component: ReplayShellPage,
    requiresAuth: true,
    skeletonVariant: 'default',
    breadcrumb: 'Evidence',
    parentPath: '/replay',
    cluster: 'Verify',
    roleVisibility: ['admin', 'operator', 'sre', 'compliance', 'auditor'],
    modes: [UiMode.Audit],
  },
  {
    path: '/replay/:sessionId/evidence',
    component: ReplayShellPage,
    requiresAuth: true,
    skeletonVariant: 'default',
    breadcrumb: 'Evidence',
    parentPath: '/replay/:sessionId',
    cluster: 'Verify',
    roleVisibility: ['admin', 'operator', 'sre', 'compliance', 'auditor'],
    modes: [UiMode.Audit],
  },
  {
    path: '/replay/compare',
    component: ReplayShellPage,
    requiresAuth: true,
    skeletonVariant: 'default',
    breadcrumb: 'Compare',
    parentPath: '/replay',
    cluster: 'Verify',
    roleVisibility: ['admin', 'operator', 'sre', 'compliance', 'auditor'],
    modes: [UiMode.Audit],
  },
  {
    path: '/replay/:sessionId/compare',
    component: ReplayShellPage,
    requiresAuth: true,
    skeletonVariant: 'default',
    breadcrumb: 'Compare',
    parentPath: '/replay/:sessionId',
    cluster: 'Verify',
    roleVisibility: ['admin', 'operator', 'sre', 'compliance', 'auditor'],
    modes: [UiMode.Audit],
  },
  {
    path: '/replay/export',
    component: ReplayShellPage,
    requiresAuth: true,
    skeletonVariant: 'default',
    breadcrumb: 'Export',
    parentPath: '/replay',
    cluster: 'Verify',
    roleVisibility: ['admin', 'operator', 'sre', 'compliance', 'auditor'],
    modes: [UiMode.Audit],
  },
  {
    path: '/replay/:sessionId/export',
    component: ReplayShellPage,
    requiresAuth: true,
    skeletonVariant: 'default',
    breadcrumb: 'Export',
    parentPath: '/replay/:sessionId',
    cluster: 'Verify',
    roleVisibility: ['admin', 'operator', 'sre', 'compliance', 'auditor'],
    modes: [UiMode.Audit],
  },
  {
    path: '/security',
    component: redirectTo('/security/policies', 'Guardrails'),
    requiresAuth: true,
    navGroup: 'Verify',
    navTitle: 'Security',
    navIcon: Shield,
    navOrder: 6,
    skeletonVariant: 'table',
    breadcrumb: 'Security',
    cluster: 'Verify',
    roleVisibility: ['admin', 'compliance', 'auditor'],
    modes: [UiMode.Audit],
  },
  {
    path: '/security/policies',
    component: PoliciesPage,
    requiresAuth: true,
    navGroup: 'Verify',
    navTitle: 'Guardrails',
    navIcon: Shield,
    navOrder: 0,
    skeletonVariant: 'table',
    breadcrumb: 'Guardrails',
    parentPath: '/security',
    cluster: 'Verify',
    roleVisibility: ['admin', 'compliance', 'auditor'],
    modes: [UiMode.Audit],
  },
  {
    path: '/security/audit',
    component: AuditPage,
    requiresAuth: true,
    requiredPermissions: ['audit:view'],
    navGroup: 'Verify',
    navTitle: 'Audit Logs',
    navIcon: FileText,
    navOrder: 1,
    skeletonVariant: 'table',
    breadcrumb: 'Audit',
    parentPath: '/security',
    cluster: 'Verify',
    roleVisibility: ['admin', 'compliance', 'auditor'],
    modes: [UiMode.Audit],
  },
  {
    path: '/security/compliance',
    component: CompliancePage,
    requiresAuth: true,
    requiredPermissions: ['audit:view'],
    navGroup: 'Verify',
    navTitle: 'Compliance',
    navIcon: CheckCircle,
    navOrder: 2,
    skeletonVariant: 'table',
    breadcrumb: 'Compliance',
    parentPath: '/security',
    cluster: 'Verify',
    roleVisibility: ['admin', 'compliance', 'auditor'],
    modes: [UiMode.Audit],
  },
  {
    path: '/security/evidence',
    component: EvidencePage,
    requiresAuth: true,
    navGroup: 'Verify',
    navTitle: 'Evidence',
    navIcon: FileOutput,
    navOrder: 3,
    skeletonVariant: 'table',
    breadcrumb: 'Evidence',
    parentPath: '/security',
    cluster: 'Verify',
    roleVisibility: ['admin', 'compliance', 'auditor', 'operator'],
    modes: [UiMode.Audit],
  },
  {
    path: '/admin',
    component: AdminPage,
    requiresAuth: true,
    requiredRoles: ['admin'],
    navGroup: 'Build',
    navTitle: 'Admin',
    navIcon: Settings,
    navOrder: 5,
    skeletonVariant: 'form',
    breadcrumb: 'Admin',
    cluster: 'Build',
    roleVisibility: ['admin'],
    modes: [UiMode.Builder],
  },
  {
    path: '/admin/tenants',
    component: TenantsPage,
    requiresAuth: true,
    requiredRoles: ['admin'],
    navGroup: 'Build',
    navTitle: 'Organizations',
    navIcon: Building,
    navOrder: 2,
    skeletonVariant: 'table',
    breadcrumb: 'Organizations',
    parentPath: '/admin',
    cluster: 'Build',
    roleVisibility: ['admin'],
    modes: [UiMode.Builder],
  },
  {
    path: '/admin/tenants/:tenantId',
    component: TenantDetailRoutePage,
    requiresAuth: true,
    requiredRoles: ['admin'],
    skeletonVariant: 'default',
    breadcrumb: 'Organization Detail',
    parentPath: '/admin/tenants',
    cluster: 'Build',
    roleVisibility: ['admin'],
  },
  {
    path: '/admin/stacks',
    component: AdminStacksPage,
    requiresAuth: true,
    requiredRoles: ['admin'],
    navGroup: 'Build',
    navTitle: 'Adapter Stacks',
    navIcon: Layers,
    navOrder: 3,
    skeletonVariant: 'table',
    breadcrumb: 'Stacks',
    parentPath: '/admin',
    cluster: 'Build',
    roleVisibility: ['admin'],
    modes: [UiMode.Builder],
  },
  {
    path: '/admin/stacks/:stackId',
    component: StackDetailRoutePage,
    requiresAuth: true,
    requiredRoles: ['admin'],
    skeletonVariant: 'default',
    breadcrumb: 'Stack Detail',
    parentPath: '/admin/stacks',
    cluster: 'Build',
    roleVisibility: ['admin'],
  },
  {
    path: '/admin/plugins',
    component: AdminPluginsPage,
    requiresAuth: true,
    requiredRoles: ['admin'],
    navGroup: 'Build',
    navTitle: 'Plugins',
    navIcon: Plug,
    navOrder: 4,
    skeletonVariant: 'table',
    breadcrumb: 'Plugins',
    parentPath: '/admin',
    cluster: 'Build',
    roleVisibility: ['admin'],
    modes: [UiMode.Builder],
  },
  {
    path: '/admin/settings',
    component: AdminSettingsPage,
    requiresAuth: true,
    requiredRoles: ['admin'],
    navGroup: 'Build',
    navTitle: 'Settings',
    navIcon: Settings,
    navOrder: 5,
    skeletonVariant: 'form',
    breadcrumb: 'Settings',
    parentPath: '/admin',
    cluster: 'Build',
    roleVisibility: ['admin'],
    modes: [UiMode.Builder],
  },
  {
    path: '/reports',
    component: redirectTo('/metrics', 'Metrics'),
    requiresAuth: true,
    skeletonVariant: 'dashboard',
    breadcrumb: 'Reports',
    cluster: 'Observe',
    roleVisibility: ['admin', 'sre', 'compliance', 'auditor'],
    modes: [],
  },
  {
    path: '/base-models',
    component: BaseModelsPage,
    requiresAuth: true,
    navGroup: 'Build',
    navTitle: 'Base Models',
    navIcon: Database,
    navOrder: 4,
    skeletonVariant: 'table',
    breadcrumb: 'Base Models',
    cluster: 'Build',
    roleVisibility: ['admin'],
    modes: [UiMode.Builder],
  },
  {
    path: '/code-intelligence',
    component: redirectTo(buildTelemetryViewerLink({ sourceType: 'code_intelligence' }), 'Telemetry'),
    requiresAuth: true,
    skeletonVariant: 'table',
    breadcrumb: 'Code Intelligence',
    cluster: 'Run',
    roleVisibility: ['admin'],
    modes: [],
  },
  {
    path: '/metrics/advanced',
    component: redirectTo('/metrics', 'Metrics'),
    requiresAuth: true,
    skeletonVariant: 'dashboard',
    breadcrumb: 'Advanced Metrics',
    cluster: 'Observe',
    roleVisibility: ['admin', 'operator', 'sre'],
    modes: [],
  },
  {
    path: '/help',
    component: redirectTo('/dashboard', 'Dashboard'),
    requiresAuth: false,
    skeletonVariant: 'default',
    breadcrumb: 'Help Center',
    cluster: 'Observe',
    modes: [],
  },
  {
    path: '/router-config',
    component: RouterConfigPage,
    requiresAuth: true,
    requiredRoles: ['admin', 'operator', 'sre'],
    navGroup: 'Build',
    navTitle: 'Router Config',
    navIcon: Network,
    navOrder: 3,
    skeletonVariant: 'form',
    breadcrumb: 'Router Config',
    cluster: 'Build',
    roleVisibility: ['admin', 'operator', 'sre'],
    modes: [UiMode.Builder],
  },
  {
    // IA-EXTRA: federation route is tooling/advanced and not part of core flows
    path: '/federation',
    component: FederationPage,
    requiresAuth: true,
    requiredRoles: ['admin'],
    navGroup: 'Build',
    navTitle: 'Federation',
    navIcon: Network,
    navOrder: 6,
    skeletonVariant: 'table',
    breadcrumb: 'Federation',
    cluster: 'Build',
    roleVisibility: ['admin'],
    modes: [UiMode.Builder],
  },
  // Dev-only routes
  ...(import.meta.env.DEV
    ? [
        {
          // IA-EXTRA: dev-only route, excluded from production IA
          path: '/dev/api-errors',
          component: DevErrorsPage,
          requiresAuth: false,
          skeletonVariant: 'default' as const,
          breadcrumb: 'API Error Inspector',
          cluster: 'Verify' as const,
          roleVisibility: ['admin'] as UserRole[],
        },
        {
          // Dev-only contract viewer with live JSON payloads
          path: '/dev/contracts',
          component: DevContractsPage,
          requiresAuth: true,
          skeletonVariant: 'default' as const,
          breadcrumb: 'Contract Samples',
          cluster: 'Verify' as const,
          roleVisibility: ['admin'] as UserRole[],
        },
        {
          // IA-EXTRA: dev-only route, excluded from production IA
          path: '/_dev/routes',
          component: RoutesDebugPage,
          requiresAuth: false,
          skeletonVariant: 'table' as const,
          breadcrumb: 'Routes Manifest',
          cluster: 'Verify' as const,
          roleVisibility: ['admin'] as UserRole[],
        },
      ]
    : []),
];

// Helper to get route by path
export function getRouteByPath(path: string): RouteConfig | undefined {
  return routes.find(route => route.path === path);
}

// Helper to match route with params (e.g., /adapters/:adapterId matches /adapters/123)
export function matchRoute(pathname: string): RouteConfig | undefined {
  return routes.find(route => {
    const routeParts = route.path.split('/');
    const pathParts = pathname.split('/');

    if (routeParts.length !== pathParts.length) {
      return false;
    }

    return routeParts.every((part, i) => {
      if (part.startsWith(':')) {
        return true; // param segment matches any value
      }
      return part === pathParts[i];
    });
  });
}

// Cluster helpers
export function getClusterForPath(pathname: string): RouteCluster | undefined {
  return matchRoute(pathname)?.cluster;
}

export function formatClusterPrefixedLabel(
  pathname: string,
  label: string,
  delimiter: ' / ' | ': ' = ' / ',
): string {
  const cluster = getClusterForPath(pathname);
  return cluster ? `${cluster}${delimiter}${label}` : label;
}

/**
 * Helper to match a route config by parameterized path.
 * Supports matching parameterized paths like '/adapters/:adapterId'.
 *
 * @param path - Path to match (can be parameterized)
 * @param pathname - Actual pathname with param values
 * @returns RouteConfig if matched, undefined otherwise
 */
function matchRouteConfig(path: string, pathname: string): RouteConfig | undefined {
  // First try exact match
  const exactMatch = routes.find(route => route.path === path);
  if (exactMatch) {
    return exactMatch;
  }

  // Then try pattern matching for parameterized paths
  return routes.find(route => {
    const routeParts = route.path.split('/');
    const pathParts = path.split('/');

    if (routeParts.length !== pathParts.length) {
      return false;
    }

    return routeParts.every((part, i) => {
      if (part.startsWith(':')) {
        return true; // param segment matches any value
      }
      return part === pathParts[i];
    });
  });
}

/**
 * Resolves a parameterized path with actual parameter values.
 *
 * @param parameterizedPath - Path with :param placeholders
 * @param params - Record of param names to values
 * @returns Resolved path with params replaced
 */
function resolvePathWithParams(
  parameterizedPath: string,
  params: Record<string, string>
): string {
  let resolvedPath = parameterizedPath;

  Object.entries(params).forEach(([paramName, paramValue]) => {
    const paramPattern = `:${paramName}`;
    resolvedPath = resolvedPath.replace(paramPattern, paramValue);
  });

  return resolvedPath;
}

/**
 * Extracts parameter values from pathname based on route pattern.
 *
 * @param pathname - Actual URL path
 * @param routePattern - Route pattern with :params
 * @returns Record of param names to values
 */
function extractParamsFromPath(
  pathname: string,
  routePattern: string
): Record<string, string> {
  const params: Record<string, string> = {};

  const patternParts = routePattern.split('/');
  const pathParts = pathname.split('/');

  if (patternParts.length !== pathParts.length) {
    return params;
  }

  patternParts.forEach((part, index) => {
    if (part.startsWith(':')) {
      const paramName = part.slice(1);
      params[paramName] = pathParts[index];
    }
  });

  return params;
}

/**
 * Helper to get breadcrumb trail for a route with resolved parameter paths.
 *
 * @param pathname - Current pathname (e.g., '/adapters/abc-123/lineage')
 * @param params - Optional route parameters (from useParams hook)
 * @returns Array of breadcrumb items with resolved paths and labels
 *
 * Example:
 *   getBreadcrumbs('/adapters/abc-123/lineage', { adapterId: 'abc-123' })
 *   Returns: [
 *     { path: '/adapters', label: 'Adapters' },
 *     { path: '/adapters/abc-123', label: 'Adapter Detail' },
 *     { path: '/adapters/abc-123/lineage', label: 'Lineage' }
 *   ]
 */
export function getBreadcrumbs(
  pathname: string,
  params?: Record<string, string>
): Array<{ path: string; label: string }> {
  const breadcrumbs: Array<{ path: string; label: string }> = [];
  const currentRoute = matchRoute(pathname);

  if (!currentRoute) {
    return breadcrumbs;
  }

  // Extract params from pathname if not provided
  const routeParams = params || extractParamsFromPath(pathname, currentRoute.path);

  // Build breadcrumb chain by following parentPath
  let route: RouteConfig | undefined = currentRoute;
  const chain: RouteConfig[] = [];

  while (route) {
    chain.unshift(route);

    // Match parent route, supporting parameterized paths
    if (route.parentPath) {
      // Resolve parent path with current params before matching
      const resolvedParentPath = resolvePathWithParams(route.parentPath, routeParams);
      route = matchRouteConfig(route.parentPath, resolvedParentPath);
    } else {
      route = undefined;
    }
  }

  // Convert to breadcrumb format with resolved paths
  chain.forEach(r => {
    if (r.breadcrumb) {
      breadcrumbs.push({
        path: resolvePathWithParams(r.path, routeParams),
        label: r.breadcrumb,
      });
    }
  });

  return breadcrumbs;
}

/**
 * Checks if a user has access to a route based on role and permissions.
 *
 * This function enforces BOTH visibility and access control:
 * 1. roleVisibility: Controls navigation UI visibility (sidebar, breadcrumbs)
 * 2. requiredRoles: Controls actual route access (blocks unauthorized access)
 * 3. requiredPermissions: Additional fine-grained access control
 *
 * Access flow:
 * - Developer role bypasses all restrictions (superuser)
 * - roleVisibility is checked first (if user can't see it, they can't access it)
 * - requiredRoles enforces hard access control (redirects unauthorized users)
 * - requiredPermissions provides additional granular control
 *
 * @param route - The route configuration to check
 * @param userRole - The user's role (e.g., 'admin', 'operator', 'viewer')
 * @param userPermissions - Optional array of user permissions for fine-grained access
 * @returns true if user can access the route, false otherwise
 *
 * @example
 * // Admin-only route
 * const adminRoute = {
 *   path: '/admin',
 *   component: AdminPage,
 *   requiredRoles: ['admin'],
 *   roleVisibility: ['admin'],
 * };
 * canAccessRoute(adminRoute, 'operator'); // false - blocked by both visibility and access
 * canAccessRoute(adminRoute, 'admin');    // true
 *
 * @example
 * // Visible to operators but accessible via URL by viewers (e.g., shared inference links)
 * const inferenceRoute = {
 *   path: '/inference',
 *   component: InferencePage,
 *   roleVisibility: ['admin', 'operator'],
 *   // No requiredRoles - viewers can access via direct URL
 * };
 * canAccessRoute(inferenceRoute, 'viewer'); // false - hidden from viewer navigation
 * // But viewer could still access via direct URL if they know the path
 */
export function canAccessRoute(route: RouteConfig, userRole?: UserRole, userPermissions?: string[]): boolean {
  // Developer role bypasses all route restrictions
  if (userRole?.toLowerCase() === 'developer') {
    return true;
  }

  // Check roleVisibility if defined (visibility check comes before access control)
  // This controls whether the route appears in navigation UI
  if (route.roleVisibility && route.roleVisibility.length > 0) {
    const hasVisibility = route.roleVisibility.some(
      r => r.toLowerCase() === userRole?.toLowerCase()
    );
    if (!hasVisibility) {
      return false; // Route is hidden from this role's navigation
    }
  }

  // Check role-based access (case-insensitive)
  // This enforces hard access control - users without required role are redirected
  if (route.requiredRoles && route.requiredRoles.length > 0) {
    if (!userRole || !route.requiredRoles.some(role => role.toLowerCase() === userRole.toLowerCase())) {
      return false; // Route is inaccessible to this role (even via direct URL)
    }
  }

  // Check permission-based access
  // Additional fine-grained control beyond role-based access
  if (route.requiredPermissions && route.requiredPermissions.length > 0) {
    if (!userPermissions || userPermissions.length === 0) {
      return false;
    }

    // User must have at least one of the required permissions
    const hasPermission = route.requiredPermissions.some(perm =>
      userPermissions.includes(perm)
    );

    if (!hasPermission) {
      return false;
    }
  }

  return true;
}
