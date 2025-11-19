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

import DashboardPage from '@/pages/DashboardPage';
import TenantsPage from '@/pages/TenantsPage';
import AdaptersPage from '@/pages/AdaptersPage';
import { AdapterDetail } from '@/components/AdapterDetail';
import PoliciesPage from '@/pages/PoliciesPage';
import MetricsPage from '@/pages/MetricsPage';
import TelemetryPage from '@/pages/TelemetryPage';
import ObservabilityPage from '@/pages/ObservabilityPage';
import InferencePage from '@/pages/InferencePage';
import AuditPage from '@/pages/AuditPage';
import BaseModelsPage from '@/pages/BaseModelsPage';
import WorkflowPage from '@/pages/WorkflowPage';
import TrainingPage from '@/pages/TrainingPage';
import TestingPage from '@/pages/TestingPage';
import GoldenPage from '@/pages/GoldenPage';
import PromotionPage from '@/pages/PromotionPage';
import RoutingPage from '@/pages/RoutingPage';
import ReplayPage from '@/pages/ReplayPage';
import AdminPage from '@/pages/AdminPage';
import ReportsPage from '@/pages/ReportsPage';
import TrainerPage from '@/pages/TrainerPage';
import PersonasPage from '@/pages/PersonasPage';
import ManagementPage from '@/pages/ManagementPage';

export interface RouteConfig {
  path: string;
  component: React.ComponentType;
  requiresAuth?: boolean;
  requiredRoles?: UserRole[];
  navGroup?: string;
  navTitle?: string;
  navIcon?: LucideIcon;
  navOrder?: number;
  disabled?: boolean;
  external?: boolean;
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
  },
  {
    path: '/management',
    component: ManagementPage,
    requiresAuth: true,
    navGroup: 'Home',
    navTitle: 'Management Panel',
    navIcon: Grid3x3,
    navOrder: 2,
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
  },
  {
    path: '/trainer',
    component: TrainerPage,
    requiresAuth: true,
    navGroup: 'ML Pipeline',
    navTitle: 'Single-File Trainer',
    navIcon: Upload,
    navOrder: 1,
  },
  {
    path: '/training',
    component: TrainingPage,
    requiresAuth: true,
    navGroup: 'ML Pipeline',
    navTitle: 'Training Jobs',
    navIcon: Zap,
    navOrder: 2,
  },
  {
    path: '/testing',
    component: TestingPage,
    requiresAuth: true,
    navGroup: 'ML Pipeline',
    navTitle: 'Testing',
    navIcon: FlaskConical,
    navOrder: 3,
  },
  {
    path: '/golden',
    component: GoldenPage,
    requiresAuth: true,
    navGroup: 'ML Pipeline',
    navTitle: 'Golden Runs',
    navIcon: GitCompare,
    navOrder: 4,
  },
  {
    path: '/promotion',
    component: PromotionPage,
    requiresAuth: true,
    navGroup: 'ML Pipeline',
    navTitle: 'Promotion',
    navIcon: TrendingUp,
    navOrder: 5,
  },
  {
    path: '/adapters',
    component: AdaptersPage,
    requiresAuth: true,
    navGroup: 'ML Pipeline',
    navTitle: 'Adapters',
    navIcon: Box,
    navOrder: 6,
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
  },
  {
    path: '/monitoring',
    component: ObservabilityPage,
    requiresAuth: true,
    navGroup: 'Monitoring',
    navTitle: 'System Health',
    navIcon: Activity,
    navOrder: 2,
  },
  {
    path: '/routing',
    component: RoutingPage,
    requiresAuth: true,
    navGroup: 'Monitoring',
    navTitle: 'Routing Inspector',
    navIcon: Route,
    navOrder: 3,
  },
  {
    path: '/inference',
    component: InferencePage,
    requiresAuth: true,
    navGroup: 'Operations',
    navTitle: 'Inference',
    navIcon: Play,
    navOrder: 1,
  },
  {
    path: '/telemetry',
    component: TelemetryPage,
    requiresAuth: true,
    navGroup: 'Operations',
    navTitle: 'Telemetry',
    navIcon: Eye,
    navOrder: 2,
  },
  {
    path: '/replay',
    component: ReplayPage,
    requiresAuth: true,
    navGroup: 'Operations',
    navTitle: 'Replay',
    navIcon: RotateCcw,
    navOrder: 3,
  },
  {
    path: '/policies',
    component: PoliciesPage,
    requiresAuth: true,
    navGroup: 'Compliance',
    navTitle: 'Policies',
    navIcon: Shield,
    navOrder: 1,
  },
  {
    path: '/audit',
    component: AuditPage,
    requiresAuth: true,
    navGroup: 'Compliance',
    navTitle: 'Audit',
    navIcon: FileText,
    navOrder: 2,
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
  },
  {
    path: '/reports',
    component: ReportsPage,
    requiresAuth: true,
    navGroup: 'Administration',
    navTitle: 'Reports',
    navIcon: BarChart3,
    navOrder: 2,
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
  },
  {
    path: '/base-models',
    component: BaseModelsPage,
    requiresAuth: true,
  },
  {
    path: '/observability',
    component: ObservabilityPage,
    requiresAuth: true,
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
