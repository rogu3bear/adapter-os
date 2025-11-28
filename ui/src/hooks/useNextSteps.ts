/**
 * useNextSteps Hook
 *
 * Provides dynamic, prioritized recommendations based on current system state.
 *
 * Priority Levels:
 * - Critical: System errors, policy violations, memory pressure
 * - High: No adapters, adapters not loaded, failing training jobs
 * - Medium: Training jobs in progress, completed jobs pending review
 * - Low: Optimization suggestions, documentation links
 *
 * Each action includes: id, title, description, priority, icon, action callback
 */

import { useMemo } from 'react';
import { useNavigate } from 'react-router-dom';
import type { SystemStateResponse, MemoryPressureLevel } from '@/api/system-state-types';
import type { Adapter } from '@/api/adapter-types';
import type { PolicyCheck } from '@/api/policyTypes';
import {
  AlertTriangle,
  Shield,
  Zap,
  CheckCircle,
  Activity,
  TrendingUp,
  BookOpen,
  PlayCircle,
  Eye,
  Upload,
  Settings,
  Database,
  LucideIcon
} from 'lucide-react';

export type ActionPriority = 'critical' | 'high' | 'medium' | 'low';

export interface NextStepAction {
  id: string;
  title: string;
  description: string;
  priority: ActionPriority;
  icon: LucideIcon;
  action: () => void;
  route?: string;
  metadata?: Record<string, unknown>;
}

export interface UseNextStepsInput {
  /** System state from useSystemState hook */
  systemState?: SystemStateResponse | null;
  /** List of all adapters */
  adapters?: Adapter[];
  /** Training jobs (if available) */
  trainingJobs?: Array<{
    id: string;
    status: string;
    adapter_name?: string;
    started_at?: string;
    completed_at?: string;
    error_message?: string;
  }>;
  /** Policy violations (if available) */
  policyViolations?: PolicyCheck[];
  /** System errors (if available) */
  systemErrors?: Array<{
    message: string;
    severity: 'error' | 'warning';
    source?: string;
  }>;
}

export interface UseNextStepsReturn {
  /** Prioritized list of recommended actions */
  actions: NextStepAction[];
  /** Count by priority */
  counts: {
    critical: number;
    high: number;
    medium: number;
    low: number;
    total: number;
  };
  /** Whether there are critical actions */
  hasCritical: boolean;
}

/**
 * Hook for generating dynamic next step recommendations
 *
 * @example
 * ```tsx
 * const { data: systemState } = useSystemState();
 * const { actions, hasCritical } = useNextSteps({
 *   systemState,
 *   adapters,
 *   trainingJobs,
 *   policyViolations
 * });
 *
 * return (
 *   <div>
 *     {hasCritical && <Alert>Critical issues detected</Alert>}
 *     {actions.map(action => (
 *       <ActionCard key={action.id} action={action} />
 *     ))}
 *   </div>
 * );
 * ```
 */
export function useNextSteps(input: UseNextStepsInput = {}): UseNextStepsReturn {
  const {
    systemState,
    adapters = [],
    trainingJobs = [],
    policyViolations = [],
    systemErrors = []
  } = input;

  const navigate = useNavigate();

  const actions = useMemo(() => {
    const recommendations: NextStepAction[] = [];

    // ===== PRIORITY 1: CRITICAL =====

    // Critical system errors
    const criticalErrors = systemErrors.filter(e => e.severity === 'error');
    if (criticalErrors.length > 0) {
      recommendations.push({
        id: 'fix-system-errors',
        title: 'Fix Critical System Errors',
        description: `${criticalErrors.length} critical error${criticalErrors.length > 1 ? 's' : ''} detected: ${criticalErrors[0].message}`,
        priority: 'critical',
        icon: AlertTriangle,
        action: () => navigate('/system'),
        route: '/system',
        metadata: { errorCount: criticalErrors.length }
      });
    }

    // Policy violations
    const failedPolicies = policyViolations.filter(p => p.status === 'failed');
    if (failedPolicies.length > 0) {
      recommendations.push({
        id: 'resolve-policy-violations',
        title: 'Resolve Policy Violations',
        description: `${failedPolicies.length} policy violation${failedPolicies.length > 1 ? 's' : ''}: ${failedPolicies[0].name}`,
        priority: 'critical',
        icon: Shield,
        action: () => navigate('/security/policies'),
        route: '/security/policies',
        metadata: { violationCount: failedPolicies.length }
      });
    }

    // Critical memory pressure
    if (systemState?.memory.pressure_level === 'critical') {
      recommendations.push({
        id: 'address-memory-pressure',
        title: 'Address Critical Memory Pressure',
        description: `Memory headroom at ${systemState.memory.headroom_percent.toFixed(1)}% (policy requires ≥15%)`,
        priority: 'critical',
        icon: AlertTriangle,
        action: () => navigate('/system/memory'),
        route: '/system/memory',
        metadata: {
          headroomPercent: systemState.memory.headroom_percent,
          pressureLevel: systemState.memory.pressure_level
        }
      });
    }

    // Unhealthy services
    const unhealthyServices = systemState?.node.services.filter(
      s => s.status === 'unhealthy' || s.status === 'degraded'
    ) || [];
    if (unhealthyServices.length > 0) {
      recommendations.push({
        id: 'fix-unhealthy-services',
        title: 'Fix Unhealthy Services',
        description: `${unhealthyServices.length} service${unhealthyServices.length > 1 ? 's' : ''} degraded: ${unhealthyServices[0].name}`,
        priority: 'critical',
        icon: AlertTriangle,
        action: () => navigate('/system'),
        route: '/system',
        metadata: { serviceCount: unhealthyServices.length }
      });
    }

    // ===== PRIORITY 2: HIGH =====

    // No adapters registered
    if (adapters.length === 0) {
      recommendations.push({
        id: 'register-first-adapter',
        title: 'Register Your First Adapter',
        description: 'Get started by registering an adapter for code intelligence',
        priority: 'high',
        icon: Upload,
        action: () => navigate('/adapters?action=register'),
        route: '/adapters',
        metadata: { isFirstAdapter: true }
      });
    }

    // Adapters exist but none loaded
    const loadedAdapters = adapters.filter(
      a => a.current_state && ['warm', 'hot', 'resident'].includes(a.current_state)
    );
    if (adapters.length > 0 && loadedAdapters.length === 0) {
      recommendations.push({
        id: 'load-adapter',
        title: 'Load Adapter for Inference',
        description: `You have ${adapters.length} registered adapter${adapters.length > 1 ? 's' : ''} but none loaded`,
        priority: 'high',
        icon: PlayCircle,
        action: () => navigate('/adapters'),
        route: '/adapters',
        metadata: { adapterCount: adapters.length }
      });
    }

    // High memory pressure (not critical)
    if (systemState?.memory.pressure_level === 'high') {
      recommendations.push({
        id: 'optimize-memory-usage',
        title: 'Optimize Memory Usage',
        description: `Memory headroom at ${systemState.memory.headroom_percent.toFixed(1)}% - consider unloading adapters`,
        priority: 'high',
        icon: TrendingUp,
        action: () => navigate('/system/memory'),
        route: '/system/memory',
        metadata: {
          headroomPercent: systemState.memory.headroom_percent,
          pressureLevel: systemState.memory.pressure_level
        }
      });
    }

    // Failed training jobs
    const failedJobs = trainingJobs.filter(j => j.status === 'failed' || j.status === 'error');
    if (failedJobs.length > 0) {
      recommendations.push({
        id: 'review-failed-training',
        title: 'Review Failed Training Jobs',
        description: `${failedJobs.length} training job${failedJobs.length > 1 ? 's' : ''} failed - check logs for details`,
        priority: 'high',
        icon: AlertTriangle,
        action: () => navigate('/training'),
        route: '/training',
        metadata: { failedJobCount: failedJobs.length }
      });
    }

    // ===== PRIORITY 3: MEDIUM =====

    // Training jobs in progress
    const runningJobs = trainingJobs.filter(
      j => j.status === 'running' || j.status === 'pending' || j.status === 'preparing'
    );
    if (runningJobs.length > 0) {
      recommendations.push({
        id: 'monitor-training-jobs',
        title: 'Monitor Training Progress',
        description: `${runningJobs.length} training job${runningJobs.length > 1 ? 's' : ''} in progress`,
        priority: 'medium',
        icon: Activity,
        action: () => navigate('/training'),
        route: '/training',
        metadata: { runningJobCount: runningJobs.length }
      });
    }

    // Completed training jobs
    const completedJobs = trainingJobs.filter(j => j.status === 'completed' || j.status === 'success');
    if (completedJobs.length > 0) {
      recommendations.push({
        id: 'review-completed-training',
        title: 'Review Completed Training',
        description: `${completedJobs.length} training job${completedJobs.length > 1 ? 's' : ''} completed - review results`,
        priority: 'medium',
        icon: CheckCircle,
        action: () => navigate('/training'),
        route: '/training',
        metadata: { completedJobCount: completedJobs.length }
      });
    }

    // Medium memory pressure
    if (systemState?.memory.pressure_level === 'medium') {
      recommendations.push({
        id: 'monitor-memory',
        title: 'Monitor Memory Usage',
        description: `Memory headroom at ${systemState.memory.headroom_percent.toFixed(1)}% - within safe range`,
        priority: 'medium',
        icon: Activity,
        action: () => navigate('/system/memory'),
        route: '/system/memory',
        metadata: {
          headroomPercent: systemState.memory.headroom_percent,
          pressureLevel: systemState.memory.pressure_level
        }
      });
    }

    // Policy warnings
    const policyWarnings = policyViolations.filter(p => p.status === 'warning');
    if (policyWarnings.length > 0) {
      recommendations.push({
        id: 'review-policy-warnings',
        title: 'Review Policy Warnings',
        description: `${policyWarnings.length} policy warning${policyWarnings.length > 1 ? 's' : ''} detected`,
        priority: 'medium',
        icon: Shield,
        action: () => navigate('/security/policies'),
        route: '/security/policies',
        metadata: { warningCount: policyWarnings.length }
      });
    }

    // ===== PRIORITY 4: LOW =====

    // Start new training (if adapters exist and no jobs running)
    if (adapters.length > 0 && runningJobs.length === 0 && recommendations.length < 10) {
      recommendations.push({
        id: 'start-new-training',
        title: 'Start New Training Job',
        description: 'Train adapters on your codebase for improved accuracy',
        priority: 'low',
        icon: Zap,
        action: () => navigate('/training?action=start'),
        route: '/training',
        metadata: { suggestedAction: 'training' }
      });
    }

    // Review performance metrics
    if (loadedAdapters.length > 0 && recommendations.length < 10) {
      recommendations.push({
        id: 'review-metrics',
        title: 'Review Performance Metrics',
        description: 'Check inference latency and adapter activation rates',
        priority: 'low',
        icon: Activity,
        action: () => navigate('/monitoring'),
        route: '/monitoring',
        metadata: { loadedAdapterCount: loadedAdapters.length }
      });
    }

    // Explore documentation (if no critical/high items)
    if (recommendations.filter(r => r.priority === 'critical' || r.priority === 'high').length === 0) {
      recommendations.push({
        id: 'explore-docs',
        title: 'Explore Documentation',
        description: 'Learn about adapter lifecycle, policies, and best practices',
        priority: 'low',
        icon: BookOpen,
        action: () => window.open('/docs', '_blank'),
        metadata: { suggestedAction: 'documentation' }
      });
    }

    // Configure settings (if system is healthy)
    if (
      recommendations.filter(r => r.priority === 'critical' || r.priority === 'high').length === 0 &&
      adapters.length > 0
    ) {
      recommendations.push({
        id: 'configure-settings',
        title: 'Configure System Settings',
        description: 'Optimize memory policies, retention, and performance parameters',
        priority: 'low',
        icon: Settings,
        action: () => navigate('/settings'),
        route: '/settings',
        metadata: { suggestedAction: 'settings' }
      });
    }

    // Upload training data (if no datasets and adapters exist)
    if (adapters.length > 0 && trainingJobs.length === 0 && recommendations.length < 10) {
      recommendations.push({
        id: 'upload-training-data',
        title: 'Upload Training Data',
        description: 'Add documents and code samples to improve adapter performance',
        priority: 'low',
        icon: Database,
        action: () => navigate('/documents'),
        route: '/documents',
        metadata: { suggestedAction: 'upload-data' }
      });
    }

    // Sort by priority (critical -> high -> medium -> low)
    const priorityOrder: ActionPriority[] = ['critical', 'high', 'medium', 'low'];
    return recommendations.sort((a, b) => {
      return priorityOrder.indexOf(a.priority) - priorityOrder.indexOf(b.priority);
    });
  }, [systemState, adapters, trainingJobs, policyViolations, systemErrors, navigate]);

  // Calculate counts
  const counts = useMemo(() => {
    const critical = actions.filter(a => a.priority === 'critical').length;
    const high = actions.filter(a => a.priority === 'high').length;
    const medium = actions.filter(a => a.priority === 'medium').length;
    const low = actions.filter(a => a.priority === 'low').length;

    return {
      critical,
      high,
      medium,
      low,
      total: actions.length
    };
  }, [actions]);

  const hasCritical = counts.critical > 0;

  return {
    actions,
    counts,
    hasCritical
  };
}

/**
 * Get priority color class for styling
 */
export function getPriorityColor(priority: ActionPriority): string {
  switch (priority) {
    case 'critical':
      return 'border-l-red-600';
    case 'high':
      return 'border-l-orange-500';
    case 'medium':
      return 'border-l-yellow-500';
    case 'low':
      return 'border-l-blue-500';
    default:
      return 'border-l-gray-500';
  }
}

/**
 * Get priority badge color class
 */
export function getPriorityBadgeColor(priority: ActionPriority): string {
  switch (priority) {
    case 'critical':
      return 'bg-red-100 text-red-800 border-red-200';
    case 'high':
      return 'bg-orange-100 text-orange-800 border-orange-200';
    case 'medium':
      return 'bg-yellow-100 text-yellow-800 border-yellow-200';
    case 'low':
      return 'bg-blue-100 text-blue-800 border-blue-200';
    default:
      return 'bg-gray-100 text-gray-800 border-gray-200';
  }
}

export default useNextSteps;
