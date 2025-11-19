/**
 * LayoutProvider - Deprecated wrapper for backwards compatibility
 *
 * This file now re-exports hooks from the new provider structure:
 * - CoreProviders: Theme, Auth, Resize
 * - FeatureProviders: Tenant
 *
 * For new code, import directly from:
 * - @/providers/CoreProviders (useTheme, useAuth, useResize, RequireAuth)
 * - @/providers/FeatureProviders (useTenant)
 */

// Re-export from new provider structure for backwards compatibility
export {
  useTheme,
  useAuth,
  useResize,
  RequireAuth,
} from '@/providers/CoreProviders';

export {
  useTenant,
} from '@/providers/FeatureProviders';

// Deprecated: LayoutProvider component - use AppProviders instead
// Kept for backwards compatibility in tests
import { CoreProviders } from '@/providers/CoreProviders';
import { FeatureProviders } from '@/providers/FeatureProviders';

/**
 * @deprecated Use AppProviders instead. This is kept for backwards compatibility.
 */
export function LayoutProvider({ children }: { children: React.ReactNode }) {
  return (
    <CoreProviders>
      <FeatureProviders>
        {children}
      </FeatureProviders>
    </CoreProviders>
  );
}
