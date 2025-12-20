/**
 * Auth Mock Appliers
 *
 * Functions that apply vi.mock() for auth-related modules.
 *
 * IMPORTANT: vi.mock() calls must be hoisted to module scope.
 * Use these factories with vi.mock() at the top level of your test file.
 */

import { vi } from 'vitest';
import {
  createUseAuthMock,
  type UseAuthMockOptions,
  type UseAuthMockReturn,
} from '@/test/mocks/hooks/auth';

// Re-export types for convenience
export type { UseAuthMockOptions, UseAuthMockReturn } from '@/test/mocks/hooks/auth';

/**
 * Apply a basic CoreProviders auth mock.
 *
 * Note: Call this at module scope in your test file (not inside describe/it),
 * so the mocked module is registered before the components under test import it.
 */
export function mockUseAuth(options: UseAuthMockOptions = {}): UseAuthMockReturn {
  const authMock = createUseAuthMock(options);

  vi.mock('@/providers/CoreProviders', () => ({
    useAuth: () => authMock,
    useResize: () => ({
      getLayout: vi.fn(() => null),
      setLayout: vi.fn(),
    }),
    TENANT_SELECTION_REQUIRED_KEY: 'aos-tenant-selection-required',
  }));

  return authMock;
}

/**
 * Create a hoisted auth mock that can be used with vi.mock()
 *
 * USAGE: Call vi.hoisted() at module scope, then vi.mock() using the factory.
 *
 * @example
 * ```typescript
 * // At top of test file
 * import { createHoistedAuthMock } from '@/test/mocks';
 *
 * const { authMock, authMockFactory } = createHoistedAuthMock({ user: { role: 'viewer' } });
 *
 * vi.mock('@/providers/CoreProviders', authMockFactory);
 *
 * // In tests
 * expect(authMock.logout).toHaveBeenCalled();
 * ```
 */
export function createHoistedAuthMock(options: UseAuthMockOptions = {}) {
  const authMock = createUseAuthMock(options);

  const authMockFactory = () => ({
    useAuth: () => authMock,
    useResize: () => ({
      getLayout: vi.fn(() => null),
      setLayout: vi.fn(),
    }),
    TENANT_SELECTION_REQUIRED_KEY: 'aos-tenant-selection-required',
  });

  return { authMock, authMockFactory };
}

/**
 * Create auth mock with mutable state for per-test overrides
 *
 * @example
 * ```typescript
 * // At top of test file
 * const authState = createMutableAuthState();
 *
 * vi.mock('@/providers/CoreProviders', () => ({
 *   useAuth: () => authState.current,
 *   useResize: () => ({ getLayout: vi.fn(() => null), setLayout: vi.fn() }),
 *   TENANT_SELECTION_REQUIRED_KEY: 'aos-tenant-selection-required',
 * }));
 *
 * // In beforeEach or tests
 * authState.update({ user: { role: 'viewer' } });
 * authState.reset();
 * ```
 */
export function createMutableAuthState(initialOptions: UseAuthMockOptions = {}) {
  let current = createUseAuthMock(initialOptions);

  return {
    get current() {
      return current;
    },
    update(options: UseAuthMockOptions) {
      current = createUseAuthMock(options);
    },
    reset() {
      current = createUseAuthMock(initialOptions);
    },
  };
}
