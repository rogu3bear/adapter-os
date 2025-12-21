/**
 * Workbench components barrel export
 *
 * The Workbench is the unified chat hub with three-column layout:
 * - Left rail: Sessions, Datasets, Stacks tabs
 * - Center: ChatInterface
 * - Right rail: Evidence/Trace (collapsible)
 */

// Layout
export { WorkbenchLayout } from './WorkbenchLayout';
export { WorkbenchTopBar } from './WorkbenchTopBar';

// Left rail
export { LeftRail } from './left-rail/LeftRail';
export { LeftRailTabs } from './left-rail/LeftRailTabs';
export { SessionsTab } from './left-rail/SessionsTab';
export { DatasetsTab } from './left-rail/DatasetsTab';
export { StacksTab } from './left-rail/StacksTab';

// Right rail
export { RightRail, RightRailToggle } from './right-rail/RightRail';
export { RightRailHeader } from './right-rail/RightRailHeader';

// Controls
export { DetachAllButton } from './controls/DetachAllButton';
export { ResetDefaultButton } from './controls/ResetDefaultButton';
export { ActiveDatasetChip } from './controls/ActiveDatasetChip';
export { ActiveStackChip } from './controls/ActiveStackChip';
export { UndoSnackbar } from './controls/UndoSnackbar';
