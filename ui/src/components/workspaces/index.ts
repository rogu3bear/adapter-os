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
export { WorkspaceCard } from '../WorkspaceCard';
export { WorkspaceMembers } from '../WorkspaceMembers';
export { WorkspaceResources } from '../WorkspaceResources';
export { WorkspaceSelector } from '../WorkspaceSelector';
