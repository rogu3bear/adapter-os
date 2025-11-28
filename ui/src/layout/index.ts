/**
 * Layout barrel export
 *
 * Provides centralized exports for all layout components.
 * Import pattern: import { FeatureLayout, RootLayout } from '@/layout';
 */

// Main layouts
export { default as FeatureLayout } from './FeatureLayout';
export { default as RootLayout } from './RootLayout';

// Layout provider and hooks (deprecated - use @/providers instead)
export {
  LayoutProvider,
  useTheme,
  useAuth,
  useResize,
  RequireAuth,
  useTenant,
} from './LayoutProvider';

// PageWrapper - Unified page wrapper component
export { PageWrapper, default as PageWrapperDefault } from './PageWrapper';
