import { lazy } from 'react';
import type { UserRole } from '@/api/types';
import type { LucideIcon } from 'lucide-react';
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
  HelpCircle,
  Network,
  Globe,
  Bug,
  Map,
} from 'lucide-react';

// Lazy-loaded page components for code splitting
const OwnerHomePage = lazy(() => import('@/pages/OwnerHome'));
const DashboardPage = lazy(() => import('@/pages/DashboardPage'));
const TenantsPage = lazy(() => import('@/pages/TenantsPage'));
const TenantDetailPage = lazy(() => import('@/pages/Admin/TenantDetailPage').then(m => ({ default: m.TenantDetailPage })));
const AdaptersPage = lazy(() => import('@/pages/AdaptersPage'));
const AdapterDetailPage = lazy(() => import('@/pages/Adapters/AdapterDetailPage'));
const AdapterRegisterPage = lazy(() => import('@/pages/Adapters/AdapterRegisterPage'));
const AdapterActivationsPage = lazy(() => import('@/pages/Adapters/AdapterActivations'));
const AdapterLineagePage = lazy(() => import('@/pages/Adapters/AdapterLineage'));
const AdapterManifestPage = lazy(() => import('@/pages/Adapters/AdapterManifest'));
const AdaptersShellPage = lazy(() => import('@/pages/Adapters/AdaptersShell'));
const PoliciesPage = lazy(() => import('@/pages/PoliciesPage'));
const MetricsPage = lazy(() => import('@/pages/MetricsPage'));
const TelemetryPage = lazy(() => import('@/pages/TelemetryPage'));
const TelemetryViewerPage = lazy(() => import('@/pages/TelemetryViewerPage'));
const ObservabilityPage = lazy(() => import('@/pages/ObservabilityPage'));
const InferencePage = lazy(() => import('@/pages/InferencePage'));
const ChatPage = lazy(() => import('@/pages/ChatPage'));
const AuditPage = lazy(() => import('@/pages/AuditPage'));
const CompliancePage = lazy(() => import('@/pages/Security/ComplianceTab').then(m => ({ default: m.ComplianceTab })));
const BaseModelsPage = lazy(() => import('@/pages/BaseModelsPage'));
const WorkflowPage = lazy(() => import('@/pages/WorkflowPage'));
const TrainingPage = lazy(() => import('@/pages/Training/TrainingPage'));
const TrainingJobsPage = lazy(() => import('@/pages/Training/TrainingJobsPage'));
const TrainingJobDetailPage = lazy(() => import('@/pages/Training/TrainingJobDetail'));
const TrainingDatasetsPage = lazy(() => import('@/pages/Training/DatasetsTab').then(m => ({ default: m.DatasetsTab })));
const DatasetDetailPage = lazy(() => import('@/pages/Training/DatasetDetailPage'));
const TrainingTemplatesPage = lazy(() => import('@/pages/Training/TemplatesTab').then(m => ({ default: m.TemplatesTab })));
const TrainingShellPage = lazy(() => import('@/pages/Training/TrainingShell'));
const CreateAdapterPage = lazy(() => import('@/pages/CreateAdapterPage'));
const TestingPage = lazy(() => import('@/pages/TestingPage'));
const GoldenPage = lazy(() => import('@/pages/GoldenPage'));
const PromotionPage = lazy(() => import('@/pages/PromotionPage'));
const RoutingPage = lazy(() => import('@/pages/RoutingPage'));
const ReplayPage = lazy(() => import('@/pages/ReplayPage'));
const ReplayShellPage = lazy(() => import('@/pages/Replay/ReplayShell'));
const AdminPage = lazy(() => import('@/pages/AdminPage'));
const AdminStacksPage = lazy(() => import('@/pages/Admin/AdapterStacksTab').then(m => ({ default: m.AdapterStacksTab })));
const AdminPluginsPage = lazy(() => import('@/pages/Admin/PluginsPage'));
const AdminSettingsPage = lazy(() => import('@/pages/Admin/SettingsPage'));
const ReportsPage = lazy(() => import('@/pages/ReportsPage'));
const TrainerPage = lazy(() => import('@/pages/TrainerPage'));
const PersonasPage = lazy(() => import('@/pages/PersonasPage'));
const ManagementPage = lazy(() => import('@/pages/ManagementPage'));
const SystemOverviewPage = lazy(() => import('@/pages/System/SystemOverviewPage'));
const SystemNodesPage = lazy(() => import('@/pages/System/NodesTab'));
const SystemWorkersPage = lazy(() => import('@/pages/System/WorkersTab'));
const SystemMemoryPage = lazy(() => import('@/pages/System/MemoryTab'));
const SystemMetricsPage = lazy(() => import('@/pages/System/MetricsTab'));
const CodeIntelligencePage = lazy(() => import('@/pages/CodeIntelligencePage'));
const AdvancedMetricsPage = lazy(() => import('@/pages/AdvancedMetricsPage'));
const GuidedFlowPage = lazy(() => import('@/pages/GuidedFlowPage'));
const DocumentLibraryPage = lazy(() => import('@/pages/DocumentLibrary'));
const DocumentChatPage = lazy(() => import('@/pages/DocumentLibrary/DocumentChatPage'));
const HelpCenterPage = lazy(() => import('@/pages/HelpCenterPage'));
const RouterConfigPage = lazy(() => import('@/pages/RouterConfigPage'));
const FederationPage = lazy(() => import('@/pages/FederationPage'));
const DevErrorsPage = lazy(() => import('@/pages/DevErrorsPage'));
const RoutesDebugPage = lazy(() => import('@/pages/Dev/RoutesDebugPage'));
const TelemetryShellPage = lazy(() => import('@/pages/Telemetry/TelemetryShell'));

export type RouteCluster = 'Build' | 'Run' | 'Observe' | 'Verify';

export interface RouteConfig {
  path: string;
  component: React.LazyExoticComponent<React.ComponentType<unknown>> | React.ComponentType;
  requiresAuth?: boolean;
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
  roleVisibility?: UserRole[];
}

export const routes: RouteConfig[] = [
  {
    path: '/owner',
    component: OwnerHomePage,
    requiresAuth: true,
    requiredRoles: ['admin'],
    navGroup: 'Verify',
    navTitle: 'Owner Home',
    navIcon: Crown,
    navOrder: 0,
    skeletonVariant: 'dashboard',
    breadcrumb: 'Owner Home',
    cluster: 'Verify',
    roleVisibility: ['admin'],
  },
  {
    path: '/dashboard',
    component: DashboardPage,
    requiresAuth: true,
    navGroup: 'Run',
    navTitle: 'Dashboard',
    navIcon: LayoutDashboard,
    navOrder: 1,
    skeletonVariant: 'dashboard',
    breadcrumb: 'Dashboard',
    cluster: 'Run',
    roleVisibility: ['admin', 'operator', 'sre', 'compliance', 'auditor', 'viewer'],
  },
  {
    path: '/management',
    component: ManagementPage,
    requiresAuth: true,
    navGroup: 'Build',
    navTitle: 'Management Panel',
    navIcon: Grid3x3,
    navOrder: 2,
    skeletonVariant: 'dashboard',
    breadcrumb: 'Management',
    cluster: 'Build',
    roleVisibility: ['admin'],
  },
  {
    path: '/workflow',
    component: WorkflowPage,
    requiresAuth: true,
    navGroup: 'Build',
    navTitle: 'Getting Started',
    navIcon: Compass,
    navOrder: 3,
    breadcrumb: 'Getting Started',
    cluster: 'Build',
    roleVisibility: ['admin'],
  },
  {
    path: '/personas',
    component: PersonasPage,
    requiresAuth: false,
    navGroup: 'Build',
    navTitle: 'Product Tour',
    navIcon: Users,
    navOrder: 4,
    skeletonVariant: 'default',
    breadcrumb: 'Personas',
    cluster: 'Build',
    roleVisibility: ['admin'],
  },
  {
    path: '/flow/lora',
    component: GuidedFlowPage,
    requiresAuth: true,
    navGroup: 'Build',
    navTitle: 'Guided Setup',
    navIcon: GitBranch,
    navOrder: 5,
    skeletonVariant: 'default',
    breadcrumb: 'Guided Setup',
    cluster: 'Build',
    roleVisibility: ['admin'],
  },
  {
    path: '/trainer',
    component: TrainerPage,
    requiresAuth: true,
    navGroup: 'Build',
    navTitle: 'Quick Training',
    navIcon: Upload,
    navOrder: 1,
    skeletonVariant: 'form',
    breadcrumb: 'Trainer',
    cluster: 'Build',
    roleVisibility: ['admin', 'operator'],
  },
  {
    path: '/create-adapter',
    component: CreateAdapterPage,
    requiresAuth: true,
    requiredPermissions: ['adapter.register', 'training.start'],
    navGroup: 'Build',
    navTitle: 'Create Adapter',
    navIcon: PlusCircle,
    navOrder: 0,
    skeletonVariant: 'form',
    breadcrumb: 'Create Adapter',
    cluster: 'Build',
    roleVisibility: ['admin', 'operator'],
  },
  {
    path: '/training',
    component: TrainingShellPage,
    requiresAuth: true,
    navGroup: 'Build',
    navTitle: 'Training',
    navIcon: Zap,
    navOrder: 2,
    skeletonVariant: 'table',
    breadcrumb: 'Training',
    cluster: 'Build',
    roleVisibility: ['admin', 'operator'],
  },
  {
    path: '/training/jobs',
    component: TrainingShellPage,
    requiresAuth: true,
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
    path: '/training/datasets',
    component: TrainingShellPage,
    requiresAuth: true,
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
    path: '/training/templates',
    component: TrainingShellPage,
    requiresAuth: true,
    skeletonVariant: 'table',
    breadcrumb: 'Templates',
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
  },
  {
    path: '/promotion',
    component: PromotionPage,
    requiresAuth: true,
    navGroup: 'Build',
    navTitle: 'Promotion',
    navIcon: TrendingUp,
    navOrder: 5,
    skeletonVariant: 'default',
    breadcrumb: 'Promotion',
    cluster: 'Build',
    roleVisibility: ['admin', 'operator'],
  },
  {
    path: '/adapters',
    component: AdaptersShellPage,
    requiresAuth: true,
    navGroup: 'Build',
    navTitle: 'Adapters',
    navIcon: Box,
    navOrder: 6,
    skeletonVariant: 'table',
    breadcrumb: 'Adapters',
    cluster: 'Build',
    roleVisibility: ['admin', 'operator'],
  },
  {
    path: '/adapters/new',
    component: AdaptersShellPage,
    requiresAuth: true,
    requiredPermissions: ['adapter.register'],
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
  },
  {
    path: '/monitoring',
    component: ObservabilityPage,
    requiresAuth: true,
    navGroup: 'Observe',
    navTitle: 'System Health',
    navIcon: Activity,
    navOrder: 2,
    skeletonVariant: 'dashboard',
    breadcrumb: 'System Health',
    cluster: 'Observe',
    roleVisibility: ['admin', 'operator', 'sre', 'viewer'],
  },
  {
    path: '/routing',
    component: RoutingPage,
    requiresAuth: true,
    navGroup: 'Observe',
    navTitle: 'Selection History',
    navIcon: Route,
    navOrder: 3,
    skeletonVariant: 'default',
    breadcrumb: 'Selection History',
    cluster: 'Observe',
    roleVisibility: ['admin', 'operator', 'sre'],
  },
  {
    path: '/system',
    component: SystemOverviewPage,
    requiresAuth: true,
    navGroup: 'Observe',
    navTitle: 'System Overview',
    navIcon: Server,
    navOrder: 1,
    skeletonVariant: 'dashboard',
    breadcrumb: 'System',
    cluster: 'Observe',
    roleVisibility: ['admin', 'operator', 'sre'],
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
  },
  {
    path: '/inference',
    component: InferencePage,
    requiresAuth: true,
    navGroup: 'Run',
    navTitle: 'Inference',
    navIcon: Play,
    navOrder: 1,
    skeletonVariant: 'form',
    breadcrumb: 'Inference',
    cluster: 'Run',
    roleVisibility: ['admin', 'operator', 'sre', 'viewer'],
  },
  {
    path: '/chat',
    component: ChatPage,
    requiresAuth: true,
    navGroup: 'Run',
    navTitle: 'Chat',
    navIcon: MessageSquare,
    navOrder: 2,
    skeletonVariant: 'form',
    breadcrumb: 'Chat',
    cluster: 'Run',
    roleVisibility: ['admin', 'operator'],
  },
  {
    path: '/documents',
    component: DocumentLibraryPage,
    requiresAuth: true,
    navGroup: 'Run',
    navTitle: 'Documents',
    navIcon: FileText,
    navOrder: 3,
    skeletonVariant: 'table',
    breadcrumb: 'Documents',
    cluster: 'Run',
    roleVisibility: ['admin', 'operator'],
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
    component: TelemetryShellPage,
    requiresAuth: true,
    navGroup: 'Observe',
    navTitle: 'Event History',
    navIcon: Eye,
    navOrder: 4,
    skeletonVariant: 'table',
    breadcrumb: 'Event History',
    cluster: 'Observe',
    roleVisibility: ['admin', 'operator', 'sre', 'compliance', 'auditor'],
  },
  {
    path: '/telemetry/viewer',
    component: TelemetryShellPage,
    requiresAuth: true,
    skeletonVariant: 'table',
    breadcrumb: 'Telemetry Viewer',
    parentPath: '/telemetry',
    cluster: 'Observe',
    roleVisibility: ['admin', 'operator', 'sre', 'compliance', 'auditor'],
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
  },
  {
    path: '/security/policies',
    component: PoliciesPage,
    requiresAuth: true,
    navGroup: 'Verify',
    navTitle: 'Guardrails',
    navIcon: Shield,
    navOrder: 1,
    skeletonVariant: 'table',
    breadcrumb: 'Guardrails',
    cluster: 'Verify',
    roleVisibility: ['admin', 'compliance', 'auditor'],
  },
  {
    path: '/security/audit',
    component: AuditPage,
    requiresAuth: true,
    requiredPermissions: ['audit.view'],
    navGroup: 'Verify',
    navTitle: 'Audit Logs',
    navIcon: FileText,
    navOrder: 2,
    skeletonVariant: 'table',
    breadcrumb: 'Audit',
    cluster: 'Verify',
    roleVisibility: ['admin', 'compliance', 'auditor'],
  },
  {
    path: '/security/compliance',
    component: CompliancePage,
    requiresAuth: true,
    requiredPermissions: ['audit.view'],
    navGroup: 'Verify',
    navTitle: 'Compliance',
    navIcon: CheckCircle,
    navOrder: 3,
    skeletonVariant: 'table',
    breadcrumb: 'Compliance',
    cluster: 'Verify',
    roleVisibility: ['admin', 'compliance', 'auditor'],
  },
  {
    path: '/admin',
    component: AdminPage,
    requiresAuth: true,
    requiredRoles: ['admin'],
    navGroup: 'Build',
    navTitle: 'Admin',
    navIcon: Settings,
    navOrder: 1,
    skeletonVariant: 'form',
    breadcrumb: 'Admin',
    cluster: 'Build',
    roleVisibility: ['admin'],
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
  },
  {
    path: '/admin/tenants/:tenantId',
    component: TenantDetailPage,
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
  },
  {
    path: '/reports',
    component: ReportsPage,
    requiresAuth: true,
    navGroup: 'Observe',
    navTitle: 'Reports',
    navIcon: BarChart3,
    navOrder: 6,
    skeletonVariant: 'dashboard',
    breadcrumb: 'Reports',
    cluster: 'Observe',
    roleVisibility: ['admin', 'sre', 'compliance', 'auditor'],
  },
  {
    path: '/base-models',
    component: BaseModelsPage,
    requiresAuth: true,
    navGroup: 'Build',
    navTitle: 'Base Models',
    navIcon: Database,
    navOrder: 0,
    skeletonVariant: 'table',
    breadcrumb: 'Base Models',
    cluster: 'Build',
    roleVisibility: ['admin'],
  },
  {
    path: '/code-intelligence',
    component: CodeIntelligencePage,
    requiresAuth: true,
    navGroup: 'Run',
    navTitle: 'Code Intelligence',
    navIcon: FileCode,
    navOrder: 6,
    skeletonVariant: 'table',
    breadcrumb: 'Code Intelligence',
    cluster: 'Run',
    roleVisibility: ['admin'],
  },
  {
    path: '/metrics/advanced',
    component: AdvancedMetricsPage,
    requiresAuth: true,
    navGroup: 'Observe',
    navTitle: 'Advanced Metrics',
    navIcon: BarChart3,
    navOrder: 4,
    skeletonVariant: 'dashboard',
    breadcrumb: 'Advanced Metrics',
    cluster: 'Observe',
    roleVisibility: ['admin', 'operator', 'sre'],
  },
  {
    path: '/help',
    component: HelpCenterPage,
    requiresAuth: false,
    navGroup: 'Observe',
    navTitle: 'Help Center',
    navIcon: HelpCircle,
    navOrder: 1,
    skeletonVariant: 'default',
    breadcrumb: 'Help Center',
    cluster: 'Observe',
  },
  {
    path: '/router-config',
    component: RouterConfigPage,
    requiresAuth: true,
    requiredRoles: ['admin', 'operator', 'sre'],
    navGroup: 'Build',
    navTitle: 'Adapter Routing',
    navIcon: Network,
    navOrder: 6,
    skeletonVariant: 'form',
    breadcrumb: 'Router Configuration',
    cluster: 'Build',
    roleVisibility: ['admin', 'operator', 'sre'],
  },
  {
    // IA-EXTRA: federation route is tooling/advanced and not part of core flows
    path: '/federation',
    component: FederationPage,
    requiresAuth: true,
    requiredRoles: ['admin'],
    navGroup: 'Build', // IA-EXTRA: federation not in IA spec
    navTitle: 'Federation',
    navIcon: Globe,
    navOrder: 7,
    skeletonVariant: 'table',
    breadcrumb: 'Federation',
    cluster: 'Build',
    roleVisibility: ['admin'],
  },
  // Dev-only routes
  ...(import.meta.env.DEV
    ? [
        {
          // IA-EXTRA: dev-only route, excluded from production IA
          path: '/dev/errors',
          component: DevErrorsPage,
          requiresAuth: false,
          navGroup: 'Verify', // IA-EXTRA: dev-only
          navTitle: 'Error Inspector',
          navIcon: Bug,
          navOrder: 1,
          skeletonVariant: 'default' as const,
          breadcrumb: 'Error Inspector',
          cluster: 'Verify' as const,
          roleVisibility: ['admin'] as UserRole[],
        },
        {
          // IA-EXTRA: dev-only route, excluded from production IA
          path: '/_dev/routes',
          component: RoutesDebugPage,
          requiresAuth: false,
          navGroup: 'Verify', // IA-EXTRA: dev-only
          navTitle: 'Routes Manifest',
          navIcon: Map,
          navOrder: 2,
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

// Helper to get breadcrumb trail for a route
export function getBreadcrumbs(pathname: string): Array<{ path: string; label: string }> {
  const breadcrumbs: Array<{ path: string; label: string }> = [];
  const currentRoute = matchRoute(pathname);

  if (!currentRoute) {
    return breadcrumbs;
  }

  // Build breadcrumb chain by following parentPath
  let route: RouteConfig | undefined = currentRoute;
  const chain: RouteConfig[] = [];

  while (route) {
    chain.unshift(route);
    route = route.parentPath ? getRouteByPath(route.parentPath) : undefined;
  }

  // Convert to breadcrumb format
  chain.forEach(r => {
    if (r.breadcrumb) {
      breadcrumbs.push({
        path: r.path,
        label: r.breadcrumb,
      });
    }
  });

  return breadcrumbs;
}

// Helper to check if user has access to route
export function canAccessRoute(route: RouteConfig, userRole?: UserRole, userPermissions?: string[]): boolean {
  // Check role-based access (case-insensitive)
  if (route.requiredRoles && route.requiredRoles.length > 0) {
    if (!userRole || !route.requiredRoles.some(role => role.toLowerCase() === userRole.toLowerCase())) {
      return false;
    }
  }

  // Check permission-based access
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
