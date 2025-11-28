/**
 * Wizards domain barrel export
 *
 * Centralized exports for all wizard components.
 * Import pattern: import { TrainingWizard, WorkflowWizard } from '@/components/wizards';
 *
 * NOTE: Components are currently re-exported from their original locations.
 * This enables cleaner imports while maintaining backward compatibility.
 */

// Re-export from current locations (files not moved yet)
export { AdapterImportWizard } from '../AdapterImportWizard';
export { CursorSetupWizard } from '../CursorSetupWizard';
export { ModelImportWizard } from '../ModelImportWizard';
export { TenantImportWizard } from '../TenantImportWizard';
export { TrainingWizard } from '../TrainingWizard';
export { WorkflowWizard } from '../WorkflowWizard';
