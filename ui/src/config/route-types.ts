/**
 * Route Type Enforcement
 *
 * This module provides compile-time enforcement that route components can be
 * rendered without props. RouteGuard renders components as `<Component />`,
 * so any component with required props will crash at runtime.
 *
 * ## The Problem
 * Modal components like `TenantDetailPage` require props:
 * ```ts
 * interface TenantDetailPageProps {
 *   tenant: Tenant;      // required
 *   open: boolean;       // required
 *   onClose: () => void; // required
 * }
 * ```
 * If routed directly, RouteGuard calls `<TenantDetailPage />` with no props → crash.
 *
 * ## The Solution
 * 1. Create a wrapper component (`TenantDetailRoutePage`) that:
 *    - Reads params from URL via `useParams()`
 *    - Fetches data via hooks
 *    - Renders the modal with proper props
 * 2. Use the types below to enforce this at compile time
 *
 * @example
 * // This will compile - wrapper has no required props
 * const routes = [
 *   defineRoute({ path: '/tenant/:id', component: TenantDetailRoutePage }),
 * ];
 *
 * // This will NOT compile - modal has required props
 * const routes = [
 *   defineRoute({ path: '/tenant/:id', component: TenantDetailPage }),
 *   // Error: Type 'TenantDetailPage' does not satisfy 'HasNoRequiredProps'
 * ];
 */

import { lazy, type ComponentType, type ComponentProps, type LazyExoticComponent } from 'react';

/**
 * Type-level check: does `{}` extend `T`?
 * If yes, T has no required properties (all props are optional or none exist).
 * If no, T has at least one required property.
 */
type HasNoRequiredProps<T> = {} extends T ? true : false;

/**
 * Extracts props from a component type, handling both regular and lazy components.
 */
type ExtractComponentProps<C> = C extends LazyExoticComponent<infer Inner>
  ? Inner extends ComponentType<infer P>
    ? P
    : never
  : C extends ComponentType<infer P>
    ? P
    : never;

/**
 * Compile-time assertion that a component can be rendered without props.
 *
 * Usage in route definitions:
 * ```ts
 * component: TenantDetailRoutePage satisfies RouteableComponent
 * ```
 *
 * If the component has required props, TypeScript will error.
 */
export type RouteableComponent = ComponentType<{}> | ComponentType<void> | ComponentType<Record<string, never>>;

/**
 * Type guard that produces a clear error message when a component has required props.
 */
export type AssertRouteable<C extends ComponentType<Record<string, unknown>>> =
  HasNoRequiredProps<ComponentProps<C>> extends true
    ? C
    : { __error: `Component has required props and cannot be used as a route. Create a *RoutePage wrapper that reads params from URL and fetches data.`; __props: ComponentProps<C> };

/**
 * Helper to define a route with compile-time component validation.
 *
 * This function enforces that the component can be rendered without props.
 * If a component with required props is passed, TypeScript will error.
 *
 * @example
 * // Works - DashboardPage has no required props
 * defineRoute({
 *   path: '/dashboard',
 *   component: DashboardPage,
 *   cluster: 'Run',
 * });
 *
 * // Error - TenantDetailPage has required props { tenant, open, onClose }
 * defineRoute({
 *   path: '/tenant/:id',
 *   component: TenantDetailPage, // Type error here
 *   cluster: 'Build',
 * });
 */
export function defineRoute<
  C extends ComponentType<P>,
  P extends Record<string, unknown> | object = object,
>(
  config: {
    path: string;
    component: HasNoRequiredProps<ComponentProps<C>> extends true ? C : never;
    [key: string]: unknown;
  }
): typeof config {
  return config;
}

/**
 * Lazily load a named export as a route component.
 * Cleaner than: lazy(() => import('./Foo').then(m => ({ default: m.Bar })))
 *
 * @example
 * const ComplianceTab = lazyRouteableNamed(
 *   () => import('@/pages/Security/ComplianceTab'),
 *   'ComplianceTab'
 * );
 */
export function lazyRouteableNamed<
  M extends Record<string, ComponentType<Record<string, unknown>>>,
  K extends keyof M,
>(
  factory: () => Promise<M>,
  exportName: K
): LazyExoticComponent<M[K]> {
  return lazy(() => factory().then(m => ({ default: m[exportName] })));
}

/**
 * Runtime validation for development mode.
 * Checks if a component appears to have required props based on displayName patterns.
 *
 * This is a heuristic - it catches common patterns but isn't foolproof.
 * The real enforcement is at compile time via types.
 */
export function validateRouteableAtRuntime(
  component: ComponentType<Record<string, unknown>>,
  path: string
): void {
  if (process.env.NODE_ENV !== 'development') {
    return;
  }

  const name = component.displayName || component.name || 'Unknown';

  // Heuristic: Components ending in common modal/dialog patterns are suspicious
  const suspiciousPatterns = [
    /Modal$/,
    /Dialog$/,
    /Drawer$/,
    /Sheet$/,
    /Popup$/,
    /Overlay$/,
  ];

  // Don't flag *RoutePage wrappers - those are the solution
  if (name.endsWith('RoutePage')) {
    return;
  }

  for (const pattern of suspiciousPatterns) {
    if (pattern.test(name)) {
      // Using console.warn is intentional here for dev-time debugging
      // eslint-disable-next-line no-console
      console.warn(
        `[RouteGuard] Suspicious route component "${name}" for path "${path}". ` +
        `Components matching "${pattern}" often have required props (open, onClose, etc.) ` +
        `and may crash when rendered without them. Consider using a *RoutePage wrapper.`
      );
      break;
    }
  }
}

/**
 * List of known component names that should NEVER be routed directly.
 * These are modal/dialog components with required props.
 *
 * Add to this list when you encounter components that have been
 * incorrectly routed.
 */
export const BLOCKED_ROUTE_COMPONENTS = [
  'TenantDetailPage', // Requires: tenant, open, onClose
  // Add more as discovered
] as const;

type BlockedComponentName = typeof BLOCKED_ROUTE_COMPONENTS[number];

/**
 * Checks if a component is in the blocked list.
 * Used for runtime validation in development.
 */
export function isBlockedRouteComponent(component: ComponentType<Record<string, unknown>>): boolean {
  const name = component.displayName || component.name;
  return name ? BLOCKED_ROUTE_COMPONENTS.includes(name as BlockedComponentName) : false;
}
