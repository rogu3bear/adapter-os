/**
 * Component prop types barrel export
 *
 * Centralized export for all component prop type definitions.
 * Import component props using: import type { ComponentNameProps } from '@/types/components';
 */

// Adapter component types
export type {
  StackAdapter,
  AdapterImportWizardProps,
  AdapterStackComposerProps,
  AdapterLifecycleManagerProps,
  AdapterMemoryMonitorProps,
  SortableAdapterItemProps,
  AdapterStateVisualizationProps,
  DomainAdapterDomain,
  DomainAdapter,
  DomainAdapterManagerProps,
  AdapterLoadingStatusProps,
  AdapterLoadingProgressProps,
  MissingPinnedAdaptersBannerProps,
} from './adapters';

// Training component types
export type {
  WizardState,
  DatasetSummary,
  TrainingWizardProps,
  TrainingMonitorProps,
  MetricsComparisonProps,
  ChartDataPoint,
  PublishAdapterDialogProps,
  PreprocessingOptions,
  TokenizationSettings,
  DatasetConfigData,
  DatasetConfigProps,
  DatasetSelectorProps,
  TrainingMetricsDisplayProps,
  DatasetPreviewProps,
  DatasetStatsProps,
  QuickTrainConfirmModalProps,
  TrainingComparisonProps,
  DatasetVersionPickerProps,
  DatasetSplitConfigProps,
} from './training';

// Chat component types
export type {
  EvidenceItem,
  ThroughputStats,
  ChatMessage,
  ChatInterfaceProps,
  ChatMessageProps,
  ChatSearchBarProps,
  ChatArchivePanelProps,
  SessionCardProps,
  EvidenceDrawerProps,
  EvidenceDrawerTab,
  RulebookTabProps,
  CalculationTabProps,
  TraceTabProps,
  EvidenceDrawerTriggerProps,
  ReplayButtonProps,
  ChatErrorDisplayProps,
  ChatLoadingOverlayProps,
  ChatTimeoutWarningProps,
  ChatSessionActionsProps,
  ChatTagsManagerProps,
  ChatCategoriesManagerProps,
  PreChatAdapterPromptProps,
  MissingPinnedAdaptersWarningProps,
  SimplifiedChatWidgetProps,
  RouterActivitySidebarProps,
  ChatInitialLoadStateProps,
} from './chat';

// Monitoring and dashboard component types
export type {
  MetricData,
  MetricsChartProps,
  ResourceMetrics,
  NodeInfo,
  ResourceMonitorProps,
  DashboardProps,
  DashboardLayoutProps,
  MetricCardData,
  MetricsCardProps,
  AlertData,
  AlertListProps,
  DashboardWidgetFrameProps,
  DashboardSettingsProps,
  RoleDashboardProps,
  SREDashboardProps,
  OperatorDashboardProps,
  ComplianceDashboardProps,
  ITAdminDashboardProps,
  SystemHealthStatus,
  HealthStatusProps,
  RealtimeMetricsProps,
  TelemetryViewerProps,
} from './monitoring';

// Re-export common types for backward compatibility
export * from './common';
