/**
 * Policies domain barrel export
 *
 * Centralized exports for policy management components.
 * Import pattern: import { Policies, PolicyEditor } from '@/components/policies';
 *
 * NOTE: Components are currently re-exported from their original locations.
 * This enables cleaner imports while maintaining backward compatibility.
 */

// Re-export from current locations (files not moved yet)
export { Policies } from '../Policies';
export { PolicyEditor } from '../PolicyEditor';
export { default as PolicyPreflightDialog } from '../PolicyPreflightDialog';
export { PolicyViolationAlert } from '../PolicyViolationAlert';
