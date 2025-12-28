import React from 'react';
import { useAuth } from '@/providers/CoreProviders';
import AdminDashboard from './roles/AdminDashboard';
import OperatorDashboard from './roles/OperatorDashboard';
import SREDashboard from './roles/SREDashboard';
import ComplianceDashboard from './roles/ComplianceDashboard';
import ViewerDashboard from './roles/ViewerDashboard';

// Re-export role-based dashboards
export { default as AdminDashboard } from './roles/AdminDashboard';
export { default as OperatorDashboard } from './roles/OperatorDashboard';
export { default as SREDashboard } from './roles/SREDashboard';
export { default as ComplianceDashboard } from './roles/ComplianceDashboard';
export { default as ViewerDashboard } from './roles/ViewerDashboard';

// Re-export layout and provider
export { default as DashboardLayout } from './DashboardLayout';
export { DashboardProvider, useDashboard } from './DashboardProvider';

// Re-export config
export { roleConfigs } from './config/roleConfigs';

// Re-export dashboard sub-components
export { DashboardOverviewTab, type DashboardOverviewTabProps } from './DashboardOverviewTab';
export { DashboardKpiCards, type DashboardKpiCardsProps } from './DashboardKpiCards';
export { DashboardSystemResources, type DashboardSystemResourcesProps } from './DashboardSystemResources';
export { DashboardWorkflowSection, type DashboardWorkflowSectionProps } from './DashboardWorkflowSection';
export { DashboardDatasetCard, type DashboardDatasetCardProps } from './DashboardDatasetCard';
export { DashboardTrainingCard, type DashboardTrainingCardProps } from './DashboardTrainingCard';
export { DashboardTrainingWizardCard } from './DashboardTrainingWizardCard';
export { DashboardAdaptersCard, type DashboardAdaptersCardProps } from './DashboardAdaptersCard';
export { DashboardChatCard, type DashboardChatCardProps } from './DashboardChatCard';
export { DashboardHealthDialog, type DashboardHealthDialogProps } from './DashboardHealthDialog';

export default function Dashboard() {
  const { user } = useAuth();
  const role = (user?.role || 'viewer').toLowerCase();

  switch (role) {
    case 'developer':
    case 'admin':
      return <AdminDashboard />;
    case 'operator':
      return <OperatorDashboard />;
    case 'sre':
      return <SREDashboard />;
    case 'compliance':
      return <ComplianceDashboard />;
    default:
      return <ViewerDashboard />;
  }
}
