/**
 * Shared Playwright Test Fixtures for AdapterOS UI
 *
 * Provides reusable test fixtures including:
 * - Authentication setup (dev bypass via ?dev=true URL param)
 * - API mocking helpers
 * - Page object factory
 * - Console error capture
 * - Screenshot on failure
 * - Common wait utilities
 */

import { test as base, expect, type Page, type ConsoleMessage, type TestInfo } from '@playwright/test';
import {
  setupApiMocks,
  type ApiMockOptions,
  type MockUserOptions,
  type UserRole,
} from './api-mocks';

// ============================================================================
// Types
// ============================================================================

export interface ConsoleCapture {
  /** Console error messages captured during test */
  consoleErrors: string[];
  /** Page errors (uncaught exceptions) captured during test */
  pageErrors: string[];
}

export interface AuthOptions {
  /** User role for the session */
  role?: UserRole;
  /** User email for the session */
  email?: string;
  /** User display name */
  displayName?: string;
  /** User ID */
  userId?: string;
  /** Tenant ID */
  tenantId?: string;
  /** Whether dev bypass is enabled */
  devBypass?: boolean;
}

export interface PageObjectContext {
  /** The Playwright page instance */
  page: Page;
  /** Console capture for assertions */
  consoleCapture: ConsoleCapture;
  /** Navigate to a route with optional dev bypass */
  goto: (path: string, options?: { devBypass?: boolean }) => Promise<void>;
  /** Wait for loading to complete (looks for loading markers) */
  waitForLoadingComplete: (label?: string) => Promise<void>;
  /** Wait for network idle */
  waitForNetworkIdle: () => Promise<void>;
  /** Assert no console errors occurred */
  assertNoConsoleErrors: () => void;
}

export interface TestFixtures {
  /** Authenticated page with console capture and API mocks */
  authenticatedPage: PageObjectContext;
  /** Console capture attached to the page */
  consoleCapture: ConsoleCapture;
  /** API mock options for customization */
  apiMockOptions: ApiMockOptions;
}

// ============================================================================
// Console Error Capture
// ============================================================================

/**
 * Attaches console and page error listeners to capture errors during tests.
 * Returns arrays that are populated as errors occur.
 */
export function attachConsoleCapture(page: Page): ConsoleCapture {
  const consoleErrors: string[] = [];
  const pageErrors: string[] = [];

  page.on('console', (msg: ConsoleMessage) => {
    if (msg.type() !== 'error') return;

    // Include source location if available
    const loc = msg.location();
    const suffix = loc.url ? ` (${loc.url}:${loc.lineNumber}:${loc.columnNumber})` : '';
    consoleErrors.push(`${msg.text()}${suffix}`);
  });

  page.on('pageerror', (err) => {
    pageErrors.push(err.message);
  });

  return { consoleErrors, pageErrors };
}

/**
 * Attaches console and page error listeners that are commonly expected during tests
 * and should be ignored. Returns filtered captures.
 */
export function attachFilteredConsoleCapture(
  page: Page,
  ignorePatterns: RegExp[] = []
): ConsoleCapture {
  const capture = attachConsoleCapture(page);

  // Default patterns to ignore (common dev-mode warnings, etc.)
  const defaultIgnorePatterns = [
    /Download the React DevTools/i,
    /Warning: ReactDOM.render is no longer supported/i,
    /Warning: findDOMNode is deprecated/i,
    /\[HMR\]/i,
    /hot module replacement/i,
  ];

  const allPatterns = [...defaultIgnorePatterns, ...ignorePatterns];

  // Create proxy arrays that filter on access
  const originalConsoleErrors = capture.consoleErrors;
  const originalPageErrors = capture.pageErrors;

  Object.defineProperty(capture, 'consoleErrors', {
    get: () =>
      originalConsoleErrors.filter((err) => !allPatterns.some((pattern) => pattern.test(err))),
  });

  Object.defineProperty(capture, 'pageErrors', {
    get: () =>
      originalPageErrors.filter((err) => !allPatterns.some((pattern) => pattern.test(err))),
  });

  return capture;
}

// ============================================================================
// Wait Utilities
// ============================================================================

/**
 * Wait for a loading indicator to appear and disappear.
 * Useful for testing loading states.
 */
export async function waitForLoadingMarker(
  page: Page,
  label: string,
  options?: { timeout?: number }
): Promise<void> {
  const timeout = options?.timeout ?? 10000;
  const loadingMarker = page.getByLabel(label);

  // Wait for loading to appear
  await expect(loadingMarker).toBeVisible({ timeout });

  // Wait for loading to complete
  await expect(loadingMarker).toBeHidden({ timeout });
}

/**
 * Wait for any loading state to complete.
 * Looks for common loading marker patterns.
 */
export async function waitForAnyLoadingComplete(
  page: Page,
  options?: { timeout?: number }
): Promise<void> {
  const timeout = options?.timeout ?? 10000;

  // Common loading marker labels
  const loadingLabels = [
    'Loading system health',
    'Loading backend status',
    'Loading table data',
    'Loading training jobs...',
    'Loading metrics...',
    'Loading adapters...',
    'Loading models...',
  ];

  // Wait for any visible loading markers to disappear
  for (const label of loadingLabels) {
    const marker = page.getByLabel(label);
    try {
      // If it's visible, wait for it to hide
      if (await marker.isVisible()) {
        await expect(marker).toBeHidden({ timeout });
      }
    } catch {
      // Ignore if marker doesn't exist
    }
  }

  // Also check for data-testid loading states
  const testIdMarker = page.locator('[data-testid="loading-state"]');
  try {
    const count = await testIdMarker.count();
    if (count > 0) {
      await expect(testIdMarker.first()).toBeHidden({ timeout });
    }
  } catch {
    // Ignore
  }
}

/**
 * Wait for network requests to complete (network idle).
 */
export async function waitForNetworkIdle(
  page: Page,
  options?: { timeout?: number }
): Promise<void> {
  await page.waitForLoadState('networkidle', { timeout: options?.timeout ?? 10000 });
}

/**
 * Wait for a specific element to be stable (no layout changes).
 */
export async function waitForElementStable(
  page: Page,
  selector: string,
  options?: { timeout?: number; stableTime?: number }
): Promise<void> {
  const timeout = options?.timeout ?? 5000;
  const stableTime = options?.stableTime ?? 200;

  const element = page.locator(selector);
  await element.waitFor({ state: 'visible', timeout });

  // Wait for no layout shifts
  let lastBox = await element.boundingBox();
  let stableStart = Date.now();

  while (Date.now() - stableStart < stableTime) {
    await page.waitForTimeout(50);
    const currentBox = await element.boundingBox();

    if (
      !lastBox ||
      !currentBox ||
      lastBox.x !== currentBox.x ||
      lastBox.y !== currentBox.y ||
      lastBox.width !== currentBox.width ||
      lastBox.height !== currentBox.height
    ) {
      lastBox = currentBox;
      stableStart = Date.now();
    }
  }
}

// ============================================================================
// Screenshot Utilities
// ============================================================================

/**
 * Take a screenshot on test failure.
 * Should be called in afterEach hook.
 */
export async function screenshotOnFailure(
  page: Page,
  testInfo: TestInfo
): Promise<void> {
  if (testInfo.status !== testInfo.expectedStatus) {
    // Construct a clean filename from the test title
    const screenshotName = testInfo.title.replace(/[^a-zA-Z0-9]/g, '-').toLowerCase();

    await page.screenshot({
      path: testInfo.outputPath(`${screenshotName}-failure.png`),
      fullPage: true,
    });
  }
}

// ============================================================================
// Authentication Helpers
// ============================================================================

/**
 * Navigate to a path with dev bypass authentication.
 * Appends ?dev=true to the URL to trigger dev bypass mode.
 */
export async function gotoWithDevBypass(
  page: Page,
  path: string,
  options?: { waitForNetworkIdle?: boolean }
): Promise<void> {
  const separator = path.includes('?') ? '&' : '?';
  const url = `${path}${separator}dev=true`;

  await page.goto(url);

  if (options?.waitForNetworkIdle !== false) {
    await page.waitForLoadState('networkidle');
  }
}

/**
 * Setup authentication state for the page.
 * Uses dev bypass mode with API mocks.
 */
export async function setupAuthentication(
  page: Page,
  options: AuthOptions & ApiMockOptions = {}
): Promise<void> {
  const userOptions: MockUserOptions = {
    role: options.role ?? 'admin',
    email: options.email,
    displayName: options.displayName,
    userId: options.userId,
    tenantId: options.tenantId,
  };

  await setupApiMocks(page, {
    ...options,
    user: userOptions,
  });
}

// ============================================================================
// Page Object Factory
// ============================================================================

/**
 * Create a page object context with common utilities attached.
 */
export function createPageObjectContext(
  page: Page,
  consoleCapture: ConsoleCapture
): PageObjectContext {
  return {
    page,
    consoleCapture,

    async goto(path: string, options?: { devBypass?: boolean }) {
      const devBypass = options?.devBypass ?? true;

      if (devBypass) {
        await gotoWithDevBypass(page, path);
      } else {
        await page.goto(path);
        await page.waitForLoadState('networkidle');
      }
    },

    async waitForLoadingComplete(label?: string) {
      if (label) {
        await waitForLoadingMarker(page, label);
      } else {
        await waitForAnyLoadingComplete(page);
      }
    },

    async waitForNetworkIdle() {
      await waitForNetworkIdle(page);
    },

    assertNoConsoleErrors() {
      expect(consoleCapture.pageErrors, `Page errors: ${consoleCapture.pageErrors.join('\n')}`).toEqual([]);
      expect(consoleCapture.consoleErrors, `Console errors: ${consoleCapture.consoleErrors.join('\n')}`).toEqual([]);
    },
  };
}

// ============================================================================
// Extended Test Fixtures
// ============================================================================

/**
 * Extended test with custom fixtures for AdapterOS testing.
 *
 * Usage:
 * ```ts
 * import { test, expect } from '../fixtures/test-fixtures';
 *
 * test('my test', async ({ authenticatedPage }) => {
 *   await authenticatedPage.goto('/dashboard');
 *   await authenticatedPage.waitForLoadingComplete('Loading system health');
 *   authenticatedPage.assertNoConsoleErrors();
 * });
 * ```
 */
export const test = base.extend<TestFixtures>({
  // Allow tests to override mock options
  apiMockOptions: [{}, { scope: 'test' }],

  consoleCapture: async ({ page }, use) => {
    const capture = attachFilteredConsoleCapture(page);
    await use(capture);
  },

  authenticatedPage: async ({ page, consoleCapture, apiMockOptions }, use, testInfo) => {
    // Setup API mocks before navigation
    await setupApiMocks(page, apiMockOptions);

    // Create page object context
    const context = createPageObjectContext(page, consoleCapture);

    // Run the test
    await use(context);

    // Screenshot on failure
    await screenshotOnFailure(page, testInfo);
  },
});

/**
 * Export expect for convenience
 */
export { expect };

// ============================================================================
// Common Assertions
// ============================================================================

/**
 * Assert that a page heading is visible.
 */
export async function expectHeadingVisible(page: Page, name: string): Promise<void> {
  await expect(page.getByRole('heading', { name, exact: true }).first()).toBeVisible();
}

/**
 * Assert that a specific text is visible on the page.
 */
export async function expectTextVisible(page: Page, text: string, exact = true): Promise<void> {
  await expect(page.getByText(text, { exact })).toBeVisible();
}

/**
 * Assert that a data-cy element is visible.
 */
export async function expectDataCyVisible(page: Page, dataCy: string): Promise<void> {
  await expect(page.locator(`[data-cy="${dataCy}"]`)).toBeVisible();
}

/**
 * Assert that a table row with specific text is visible.
 */
export async function expectTableRowVisible(page: Page, text: string): Promise<void> {
  await expect(
    page.locator('[data-slot="table-body"]').getByText(text, { exact: true })
  ).toBeVisible();
}

// ============================================================================
// Test Helpers
// ============================================================================

/**
 * Create a test that uses authenticated page with custom mock options.
 *
 * @example
 * ```ts
 * import { createAuthenticatedTest } from '../fixtures/test-fixtures';
 *
 * const test = createAuthenticatedTest({
 *   user: { role: 'viewer' },
 *   systemStatus: { inferenceReady: false },
 * });
 *
 * test('viewer cannot see admin features', async ({ authenticatedPage }) => {
 *   await authenticatedPage.goto('/dashboard');
 *   // ...
 * });
 * ```
 */
export function createAuthenticatedTest(defaultMockOptions: ApiMockOptions) {
  return test.extend<TestFixtures>({
    apiMockOptions: [defaultMockOptions, { scope: 'test' }],
  });
}

/**
 * Create a test for a specific user role.
 */
export function createRoleTest(role: UserRole) {
  return createAuthenticatedTest({
    user: { role },
  });
}

// Pre-configured role tests
export const adminTest = createRoleTest('admin');
export const operatorTest = createRoleTest('operator');
export const sreTest = createRoleTest('sre');
export const complianceTest = createRoleTest('compliance');
export const viewerTest = createRoleTest('viewer');

// ============================================================================
// Re-exports from api-mocks for convenience
// ============================================================================

export {
  setupApiMocks,
  mockEndpoint,
  mockEndpointError,
  mockInferenceEndpoint,
  installSseStub,
  buildAuthHealthResponse,
  buildAuthConfigResponse,
  buildUserInfoResponse,
  buildTenantListResponse,
  buildSystemStatusResponse,
  buildWorkspaceListResponse,
  buildModelsListResponse,
  buildModelStatusResponse,
  buildAllModelsStatusResponse,
  buildAdaptersListResponse,
  buildBackendsResponse,
  buildBackendsCapabilitiesResponse,
  buildSystemMetricsResponse,
  buildMetricsSnapshotResponse,
  type ApiMockOptions,
  type MockUserOptions,
  type MockTenantOptions,
  type MockSystemStatusOptions,
  type MockWorkspaceOptions,
  type MockModelOptions,
  type MockAdapterOptions,
  type UserRole,
} from './api-mocks';
