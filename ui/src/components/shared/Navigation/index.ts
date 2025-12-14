/**
 * Navigation Components
 *
 * Shared navigation components for consistent page layout and user navigation
 * throughout the AdapterOS UI.
 *
 * @module shared/Navigation
 */

// Breadcrumb navigation trail
export { Breadcrumbs, useBreadcrumbsFromRoute } from './Breadcrumbs';
export type { BreadcrumbsProps, BreadcrumbItemConfig } from './Breadcrumbs';

// Page-level action buttons
export { ActionBar, ActionGroup } from './ActionBar';
export type { ActionBarProps, ActionConfig } from './ActionBar';
