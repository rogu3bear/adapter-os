/**
 * Persona stages components barrel export
 *
 * Provides role-specific dashboard components for different user personas.
 * Import pattern: import { MLEngineerTrainingSetup } from '@/components/persona-stages';
 */

// App Developer persona
export { default as AppDevAPIDocs } from './AppDevAPIDocs';
export { default as AppDevPerformancePanel } from './AppDevPerformancePanel';
export { default as AppDevSDKManager } from './AppDevSDKManager';
export { default as AppDevTestConsole } from './AppDevTestConsole';

// Data Scientist persona
export { default as DataScientistCollaborationHub } from './DataScientistCollaborationHub';
export { default as DataScientistDatasetManager } from './DataScientistDatasetManager';
export { default as DataScientistEvaluationUI } from './DataScientistEvaluationUI';
export { default as DataScientistExperimentTracker } from './DataScientistExperimentTracker';

// DevOps persona
export { default as DevOpsCIDCPanel } from './DevOpsCIDCPanel';
export { default as DevOpsMonitoringDashboard } from './DevOpsMonitoringDashboard';
export { default as DevOpsResourceDashboard } from './DevOpsResourceDashboard';
export { default as DevOpsServerConfig } from './DevOpsServerConfig';

// ML Engineer persona
export { default as MLEngineerInferenceTest } from './MLEngineerInferenceTest';
export { default as MLEngineerRegistryBrowser } from './MLEngineerRegistryBrowser';
export { default as MLEngineerTrainingMetrics } from './MLEngineerTrainingMetrics';
export { default as MLEngineerTrainingSetup } from './MLEngineerTrainingSetup';

// Product Manager persona
export { default as ProductManagerConfigPortal } from './ProductManagerConfigPortal';
export { default as ProductManagerFeedbackHub } from './ProductManagerFeedbackHub';
export { default as ProductManagerPerformanceOverview } from './ProductManagerPerformanceOverview';
export { default as ProductManagerUsageAnalytics } from './ProductManagerUsageAnalytics';

// Security persona
export { default as SecurityAuditTrail } from './SecurityAuditTrail';
export { default as SecurityIsolationTester } from './SecurityIsolationTester';
export { default as SecurityPolicyEditor } from './SecurityPolicyEditor';
export { default as SecurityThreatDashboard } from './SecurityThreatDashboard';
