/**
 * System domain barrel export
 *
 * Centralized exports for system-level components (nodes, workers, processes).
 * Import pattern: import { Nodes, WorkersTab } from '@/components/system';
 *
 * NOTE: Components are currently re-exported from their original locations.
 * This enables cleaner imports while maintaining backward compatibility.
 */

// Re-export from current locations (files not moved yet)
export { Nodes } from '@/components/Nodes';
export { WorkersTab } from '@/components/WorkersTab';
export { ProcessDebugger } from '@/components/ProcessDebugger';
