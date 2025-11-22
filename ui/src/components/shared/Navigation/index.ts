/**
 * Navigation Components
 *
 * Shared navigation components for consistent page layout and user navigation
 * throughout the AdapterOS UI.
 *
 * @module shared/Navigation
 */

// Page header with breadcrumbs and actions
export { PageHeader, PageHeaderSkeleton } from './PageHeader';
export type { PageHeaderProps } from './PageHeader';

// Breadcrumb navigation trail
export { Breadcrumbs, useBreadcrumbsFromRoute } from './Breadcrumbs';
export type { BreadcrumbsProps, BreadcrumbItemConfig } from './Breadcrumbs';

// Tab-based sub-navigation
export { TabNavigation, useTabNavigation } from './TabNavigation';
export type { TabNavigationProps, TabItem } from './TabNavigation';

// Page-level action buttons
export { ActionBar, ActionGroup } from './ActionBar';
export type { ActionBarProps, ActionConfig } from './ActionBar';

// Back navigation button
export { BackButton, BackLink, useBackNavigation } from './BackButton';
export type { BackButtonProps } from './BackButton';
