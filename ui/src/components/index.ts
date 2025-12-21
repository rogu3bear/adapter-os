// Main component exports

// Dashboard
export { default as Dashboard } from './dashboard/index';
export { default as DashboardLayout } from './dashboard/DashboardLayout';
export { DashboardProvider, useDashboard } from './dashboard/DashboardProvider';

// Dashboard roles
export { default as AdminDashboard } from './dashboard/roles/AdminDashboard';
export { default as OperatorDashboard } from './dashboard/roles/OperatorDashboard';
export { default as SREDashboard } from './dashboard/roles/SREDashboard';
export { default as ComplianceDashboard } from './dashboard/roles/ComplianceDashboard';
export { default as ViewerDashboard } from './dashboard/roles/ViewerDashboard';

// Dashboard config
export { roleConfigs } from './dashboard/config/roleConfigs';

// Policy
export { default as PolicyPreflightDialog } from './PolicyPreflightDialog';

// Documents
export { PDFViewer } from './documents/PDFViewer';
export { default as PDFViewerEmbedded } from './documents/PDFViewerEmbedded';
export type { PDFViewerEmbeddedRef } from './documents/PDFViewerEmbedded';
export { default as DocumentChatLayout } from './documents/DocumentChatLayout';

// Chat
export { EvidenceSources } from './chat/EvidenceSources';
export { EvidenceItem } from './chat/EvidenceItem';
export { ProofBadge } from './chat/ProofBadge';
export type { ChatMessage } from './chat/ChatMessage';
export { ChatMessageComponent } from './chat/ChatMessage';
export { RouterActivitySidebar } from './chat/RouterActivitySidebar';
export { RouterDetailsModal } from './chat/RouterDetailsModal';
export { RouterIndicator } from './chat/RouterIndicator';
export { ChatErrorBoundary } from './chat/ChatErrorBoundary';
export { default as RouterTechnicalView } from './chat/RouterTechnicalView';
export { default as RouterSummaryView } from './chat/RouterSummaryView';
export type { RouterDecisionSummary } from './chat/RouterSummaryView';

// Legacy dashboard (will be deprecated)
export { Dashboard as LegacyDashboard } from './Dashboard';
