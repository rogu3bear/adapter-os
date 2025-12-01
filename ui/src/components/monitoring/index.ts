/**
 * Monitoring domain barrel export
 *
 * Centralized exports for all monitoring and metrics components.
 * Import pattern: import { MonitoringDashboard, TrainingMonitor } from '@/components/monitoring';
 *
 * NOTE: Components are currently re-exported from their original locations.
 * This enables cleaner imports while maintaining backward compatibility.
 */

// Re-export from current locations (files not moved yet)
export { AdapterMemoryMonitor } from '@/components/AdapterMemoryMonitor';
export { MonitoringDashboard } from '@/components/MonitoringDashboard';
export { MonitoringPage } from '@/components/MonitoringPage';
export { ResourceMonitor } from '@/components/ResourceMonitor';
export { TrainingJobMonitor } from '@/components/TrainingJobMonitor';
export { TrainingMonitor } from '@/components/TrainingMonitor';
export { Telemetry } from '@/components/Telemetry';
