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
export { AdapterImportWizard } from '@/components/AdapterImportWizard';
export { CursorSetupWizard } from '@/components/CursorSetupWizard';
export { ModelImportWizard } from '@/components/ModelImportWizard';
export { TenantImportWizard } from '@/components/TenantImportWizard';
export { TrainingWizard } from '@/components/TrainingWizard';
export { WorkflowWizard } from '@/components/WorkflowWizard';
