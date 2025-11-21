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
} from 'lucide-react';

// Lazy-loaded page components for code splitting
const DashboardPage = lazy(() => import('@/pages/DashboardPage'));
const TenantsPage = lazy(() => import('@/pages/TenantsPage'));
const AdaptersPage = lazy(() => import('@/pages/AdaptersPage'));
const AdapterDetail = lazy(() => import('@/components/AdapterDetail').then(m => ({ default: m.AdapterDetail })));
const PoliciesPage = lazy(() => import('@/pages/PoliciesPage'));
const MetricsPage = lazy(() => import('@/pages/MetricsPage'));
const TelemetryPage = lazy(() => import('@/pages/TelemetryPage'));
const ObservabilityPage = lazy(() => import('@/pages/ObservabilityPage'));
const InferencePage = lazy(() => import('@/pages/InferencePage'));
const AuditPage = lazy(() => import('@/pages/AuditPage'));
const BaseModelsPage = lazy(() => import('@/pages/BaseModelsPage'));
const WorkflowPage = lazy(() => import('@/pages/WorkflowPage'));
const TrainingPage = lazy(() => import('@/pages/TrainingPage'));
const TestingPage = lazy(() => import('@/pages/TestingPage'));
const GoldenPage = lazy(() => import('@/pages/GoldenPage'));
const PromotionPage = lazy(() => import('@/pages/PromotionPage'));
const RoutingPage = lazy(() => import('@/pages/RoutingPage'));
const ReplayPage = lazy(() => import('@/pages/ReplayPage'));
const AdminPage = lazy(() => import('@/pages/AdminPage'));
const ReportsPage = lazy(() => import('@/pages/ReportsPage'));
const TrainerPage = lazy(() => import('@/pages/TrainerPage'));
const PersonasPage = lazy(() => import('@/pages/PersonasPage'));
const ManagementPage = lazy(() => import('@/pages/ManagementPage'));

export interface RouteConfig {
  path: string;
  component: React.LazyExoticComponent<React.ComponentType<unknown>> | React.ComponentType;
  requiresAuth?: boolean;
  requiredRoles?: UserRole[];
  navGroup?: string;
  navTitle?: string;
  navIcon?: LucideIcon;
  navOrder?: number;
  disabled?: boolean;
  external?: boolean;
  skeletonVariant?: 'default' | 'dashboard' | 'table' | 'form';
}

export const routes: RouteConfig[] = [
  {
    path: '/dashboard',
    component: DashboardPage,
    requiresAuth: true,
    navGroup: 'Home',
    navTitle: 'Dashboard',
    navIcon: LayoutDashboard,
    navOrder: 1,
    skeletonVariant: 'dashboard',
  },
  {
    path: '/management',
    component: ManagementPage,
    requiresAuth: true,
    navGroup: 'Home',
    navTitle: 'Management Panel',
    navIcon: Grid3x3,
    navOrder: 2,
    skeletonVariant: 'dashboard',
  },
  {
    path: '/workflow',
    component: WorkflowPage,
    requiresAuth: true,
    navGroup: 'Home',
    navTitle: 'Getting Started',
    navIcon: Compass,
    navOrder: 3,
  },
  {
    path: '/personas',
    component: PersonasPage,
    requiresAuth: false,
    navGroup: 'Home',
    navTitle: 'Persona Demo',
    navIcon: Users,
    navOrder: 4,
    skeletonVariant: 'default',
  },
  {
    path: '/trainer',
    component: TrainerPage,
    requiresAuth: true,
    navGroup: 'ML Pipeline',
    navTitle: 'Single-File Trainer',
    navIcon: Upload,
    navOrder: 1,
    skeletonVariant: 'form',
  },
  {
    path: '/training',
    component: TrainingPage,
    requiresAuth: true,
    navGroup: 'ML Pipeline',
    navTitle: 'Training Jobs',
    navIcon: Zap,
    navOrder: 2,
    skeletonVariant: 'table',
  },
  {
    path: '/testing',
    component: TestingPage,
    requiresAuth: true,
    navGroup: 'ML Pipeline',
    navTitle: 'Testing',
    navIcon: FlaskConical,
    navOrder: 3,
    skeletonVariant: 'default',
  },
  {
    path: '/golden',
    component: GoldenPage,
    requiresAuth: true,
    navGroup: 'ML Pipeline',
    navTitle: 'Golden Runs',
    navIcon: GitCompare,
    navOrder: 4,
    skeletonVariant: 'table',
  },
  {
    path: '/promotion',
    component: PromotionPage,
    requiresAuth: true,
    navGroup: 'ML Pipeline',
    navTitle: 'Promotion',
    navIcon: TrendingUp,
    navOrder: 5,
    skeletonVariant: 'default',
  },
  {
    path: '/adapters',
    component: AdaptersPage,
    requiresAuth: true,
    navGroup: 'ML Pipeline',
    navTitle: 'Adapters',
    navIcon: Box,
    navOrder: 6,
    skeletonVariant: 'table',
  },
  {
    path: '/adapters/:adapterId',
    component: AdapterDetail,
    requiresAuth: true,
  },
  {
    path: '/metrics',
    component: MetricsPage,
    requiresAuth: true,
    navGroup: 'Monitoring',
    navTitle: 'Metrics',
    navIcon: Activity,
    navOrder: 1,
    skeletonVariant: 'dashboard',
  },
  {
    path: '/monitoring',
    component: ObservabilityPage,
    requiresAuth: true,
    navGroup: 'Monitoring',
    navTitle: 'System Health',
    navIcon: Activity,
    navOrder: 2,
    skeletonVariant: 'dashboard',
  },
  {
    path: '/routing',
    component: RoutingPage,
    requiresAuth: true,
    navGroup: 'Monitoring',
    navTitle: 'Routing Inspector',
    navIcon: Route,
    navOrder: 3,
    skeletonVariant: 'default',
  },
  {
    path: '/inference',
    component: InferencePage,
    requiresAuth: true,
    navGroup: 'Operations',
    navTitle: 'Inference',
    navIcon: Play,
    navOrder: 1,
    skeletonVariant: 'form',
  },
  {
    path: '/telemetry',
    component: TelemetryPage,
    requiresAuth: true,
    navGroup: 'Operations',
    navTitle: 'Telemetry',
    navIcon: Eye,
    navOrder: 2,
    skeletonVariant: 'table',
  },
  {
    path: '/replay',
    component: ReplayPage,
    requiresAuth: true,
    navGroup: 'Operations',
    navTitle: 'Replay',
    navIcon: RotateCcw,
    navOrder: 3,
    skeletonVariant: 'default',
  },
  {
    path: '/policies',
    component: PoliciesPage,
    requiresAuth: true,
    navGroup: 'Compliance',
    navTitle: 'Policies',
    navIcon: Shield,
    navOrder: 1,
    skeletonVariant: 'table',
  },
  {
    path: '/audit',
    component: AuditPage,
    requiresAuth: true,
    navGroup: 'Compliance',
    navTitle: 'Audit',
    navIcon: FileText,
    navOrder: 2,
    skeletonVariant: 'table',
  },
  {
    path: '/admin',
    component: AdminPage,
    requiresAuth: true,
    requiredRoles: ['admin'],
    navGroup: 'Administration',
    navTitle: 'IT Admin',
    navIcon: Settings,
    navOrder: 1,
    skeletonVariant: 'form',
  },
  {
    path: '/reports',
    component: ReportsPage,
    requiresAuth: true,
    navGroup: 'Administration',
    navTitle: 'Reports',
    navIcon: BarChart3,
    navOrder: 2,
    skeletonVariant: 'dashboard',
  },
  {
    path: '/tenants',
    component: TenantsPage,
    requiresAuth: true,
    requiredRoles: ['admin'],
    navGroup: 'Administration',
    navTitle: 'Tenants',
    navIcon: Building,
    navOrder: 3,
    skeletonVariant: 'table',
  },
  {
    path: '/base-models',
    component: BaseModelsPage,
    requiresAuth: true,
    skeletonVariant: 'table',
  },
];

// Helper to get route by path
export function getRouteByPath(path: string): RouteConfig | undefined {
  return routes.find(route => route.path === path);
}

// Helper to check if user has access to route
export function canAccessRoute(route: RouteConfig, userRole?: UserRole): boolean {
  if (!route.requiredRoles || route.requiredRoles.length === 0) {
    return true;
  }
  return userRole ? route.requiredRoles.includes(userRole) : false;
}
