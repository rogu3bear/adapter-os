/**
 * Workspaces domain barrel export
 *
 * Centralized exports for workspace management components.
 * Import pattern: import { WorkspaceCard, WorkspaceSelector } from '@/components/workspaces';
 *
 * NOTE: Components are currently re-exported from their original locations.
 * This enables cleaner imports while maintaining backward compatibility.
 */

// Re-export from current locations (files not moved yet)
export { WorkspaceCard } from '@/components/WorkspaceCard';
export { WorkspaceMembers } from '@/components/WorkspaceMembers';
export { WorkspaceResources } from '@/components/WorkspaceResources';
export { WorkspaceSelector } from '@/components/WorkspaceSelector';
