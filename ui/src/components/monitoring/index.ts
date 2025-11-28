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
export { AdapterMemoryMonitor } from '../AdapterMemoryMonitor';
export { MonitoringDashboard } from '../MonitoringDashboard';
export { MonitoringPage } from '../MonitoringPage';
export { ResourceMonitor } from '../ResourceMonitor';
export { TrainingJobMonitor } from '../TrainingJobMonitor';
export { TrainingMonitor } from '../TrainingMonitor';
export { Telemetry } from '../Telemetry';
