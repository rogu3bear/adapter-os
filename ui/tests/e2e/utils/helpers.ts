/**
 * Test Helper Functions for E2E Tests
 *
 * Provides utility functions for capturing console errors, API logs,
 * and attaching artifacts to test results.
 *
 * NOTE: This file imports from @playwright/test for Page and TestInfo types.
 * It should only be imported by test files, not by the reporter.
 */

import type { Page, TestInfo, Route } from '@playwright/test';
import type { ConsoleEntry, ApiLogEntry, EndpointMismatch } from './types';

// ============================================================================
// Console Collector
// ============================================================================

/**
 * Creates a console error collector for use in Playwright tests.
 *
 * @example
 * test('my test', async ({ page }) => {
 *   const collector = createConsoleCollector(page);
 *   await page.goto('/');
 *   // ... test actions ...
 *   const errors = collector.getErrors();
 *   expect(errors).toHaveLength(0);
 * });
 */
export function createConsoleCollector(page: Page) {
  const entries: ConsoleEntry[] = [];

  page.on('console', (msg) => {
    const type = msg.type() as ConsoleEntry['type'];
    const loc = msg.location();
    entries.push({
      type,
      text: msg.text(),
      location: loc.url ? `${loc.url}:${loc.lineNumber}:${loc.columnNumber}` : undefined,
      timestamp: new Date().toISOString(),
    });
  });

  return {
    getAll: () => [...entries],
    getErrors: () => entries.filter((e) => e.type === 'error'),
    getWarnings: () => entries.filter((e) => e.type === 'warn'),
    clear: () => {
      entries.length = 0;
    },
  };
}

// ============================================================================
// API Logger
// ============================================================================

/**
 * Creates an API request logger for use in Playwright tests.
 *
 * @example
 * test('my test', async ({ page }) => {
 *   const apiLogger = createApiLogger(page);
 *   await page.goto('/');
 *   // ... test actions ...
 *   const logs = apiLogger.getLogs();
 *   const errors = apiLogger.getErrors();
 * });
 */
export function createApiLogger(page: Page) {
  const logs: ApiLogEntry[] = [];

  page.on('request', (request) => {
    const url = request.url();
    // Only log API requests
    if (url.includes('/v1/') || url.includes('/api/') || url.includes('/healthz')) {
      logs.push({
        timestamp: new Date().toISOString(),
        method: request.method(),
        url,
        requestBody: request.postData() ? tryParseJson(request.postData()) : undefined,
      });
    }
  });

  page.on('response', (response) => {
    const url = response.url();
    // Find matching request log and update it
    const requestLog = [...logs].reverse().find((log) => log.url === url && !log.status);
    if (requestLog) {
      requestLog.status = response.status();
      requestLog.duration = Date.now() - new Date(requestLog.timestamp).getTime();
    }
  });

  page.on('requestfailed', (request) => {
    const url = request.url();
    const requestLog = [...logs].reverse().find((log) => log.url === url && !log.status);
    if (requestLog) {
      requestLog.error = request.failure()?.errorText || 'Request failed';
    }
  });

  return {
    getLogs: () => [...logs],
    getErrors: () => logs.filter((log) => log.error || (log.status && log.status >= 400)),
    clear: () => {
      logs.length = 0;
    },
  };
}

/** Try to parse JSON, return original string on failure */
function tryParseJson(data: string | null): unknown {
  if (!data) return undefined;
  try {
    return JSON.parse(data);
  } catch {
    return data;
  }
}

// ============================================================================
// Artifact Attachment
// ============================================================================

/**
 * Attaches collected artifacts to the test for the reporter to pick up.
 *
 * @example
 * test.afterEach(async ({}, testInfo) => {
 *   await attachArtifacts(testInfo, {
 *     consoleErrors: collector.getErrors(),
 *     apiLogs: apiLogger.getLogs(),
 *   });
 * });
 */
export async function attachArtifacts(
  testInfo: TestInfo,
  artifacts: {
    consoleErrors?: ConsoleEntry[];
    apiLogs?: ApiLogEntry[];
    endpointMismatches?: EndpointMismatch[];
  }
): Promise<void> {
  if (artifacts.consoleErrors && artifacts.consoleErrors.length > 0) {
    await testInfo.attach('console-errors', {
      body: JSON.stringify(artifacts.consoleErrors),
      contentType: 'application/json',
    });
  }

  if (artifacts.apiLogs && artifacts.apiLogs.length > 0) {
    await testInfo.attach('api-logs', {
      body: JSON.stringify(artifacts.apiLogs),
      contentType: 'application/json',
    });
  }

  if (artifacts.endpointMismatches && artifacts.endpointMismatches.length > 0) {
    await testInfo.attach('endpoint-mismatches', {
      body: JSON.stringify(artifacts.endpointMismatches),
      contentType: 'application/json',
    });
  }
}

// ============================================================================
// Loading State Helpers
// ============================================================================

/**
 * Waits for a loading state to appear and then disappear.
 *
 * @example
 * await waitForLoadingComplete(page, 'Loading system health');
 */
export async function waitForLoadingComplete(
  page: Page,
  loadingLabel: string,
  options?: { timeout?: number }
): Promise<void> {
  const timeout = options?.timeout ?? 30000;
  const loadingMarker = page.getByLabel(loadingLabel);

  await loadingMarker.waitFor({ state: 'visible', timeout });
  await loadingMarker.waitFor({ state: 'hidden', timeout });
}

// ============================================================================
// Assertion Helpers
// ============================================================================

/**
 * Verifies no console errors occurred during the test.
 *
 * @example
 * const errors = collector.getErrors();
 * assertNoConsoleErrors(errors);
 */
export function assertNoConsoleErrors(errors: ConsoleEntry[]): void {
  if (errors.length > 0) {
    const errorMessages = errors.map((e) => `  - ${e.text}`).join('\n');
    throw new Error(`Console errors detected:\n${errorMessages}`);
  }
}

/**
 * Verifies no API errors occurred during the test.
 *
 * @example
 * const apiErrors = apiLogger.getErrors();
 * assertNoApiErrors(apiErrors);
 */
export function assertNoApiErrors(logs: ApiLogEntry[]): void {
  const errors = logs.filter((log) => log.error || (log.status && log.status >= 400));
  if (errors.length > 0) {
    const errorMessages = errors
      .map((e) => `  - ${e.method} ${e.url}: ${e.error || `HTTP ${e.status}`}`)
      .join('\n');
    throw new Error(`API errors detected:\n${errorMessages}`);
  }
}

// ============================================================================
// Mock Helpers
// ============================================================================

/**
 * Creates a mock API response handler for consistent test setup.
 *
 * @example
 * await page.route('**\/v1/models', createMockHandler({ models: [], total: 0 }));
 */
export function createMockHandler(body: unknown, options?: { status?: number; delay?: number }) {
  return async (route: Route) => {
    if (options?.delay) {
      await new Promise((resolve) => setTimeout(resolve, options.delay));
    }
    await route.fulfill({
      status: options?.status ?? 200,
      contentType: 'application/json',
      body: JSON.stringify(body),
    });
  };
}

/**
 * Validates that an API response matches the expected schema version.
 *
 * @example
 * const response = await page.request.get('/v1/models');
 * const body = await response.json();
 * assertSchemaVersion(body, '1.0');
 */
export function assertSchemaVersion(body: unknown, expectedVersion: string): void {
  if (
    typeof body !== 'object' ||
    body === null ||
    !('schema_version' in body) ||
    (body as { schema_version: string }).schema_version !== expectedVersion
  ) {
    throw new Error(
      `Schema version mismatch: expected ${expectedVersion}, got ${
        typeof body === 'object' && body !== null && 'schema_version' in body
          ? (body as { schema_version: string }).schema_version
          : 'undefined'
      }`
    );
  }
}

// ============================================================================
// Key Indicator Test Helpers
// ============================================================================

/**
 * Test helper for gold path validation.
 * Use this to wrap critical flow tests.
 *
 * @example
 * test('gold path: app loads and dashboard displays', async ({ guardedPage }) => {
 *   await testGoldPath(guardedPage, async (page) => {
 *     await page.goto('/dashboard');
 *     await expect(page.getByRole('heading', { name: 'Dashboard' })).toBeVisible();
 *   });
 * });
 */
export async function testGoldPath(page: Page, testFn: (page: Page) => Promise<void>): Promise<void> {
  try {
    await testFn(page);
  } catch (error) {
    // Re-throw with gold path context
    throw new Error(`Gold path failed: ${error instanceof Error ? error.message : String(error)}`);
  }
}

/**
 * Test helper for inference readiness validation.
 *
 * @example
 * await testInferenceReadiness(page, async (page) => {
 *   await page.goto('/inference');
 *   await expect(page.locator('[data-cy="prompt-input"]')).toBeVisible();
 * });
 */
export async function testInferenceReadiness(
  page: Page,
  testFn: (page: Page) => Promise<void>
): Promise<void> {
  try {
    await testFn(page);
  } catch (error) {
    throw new Error(
      `Inference readiness check failed: ${error instanceof Error ? error.message : String(error)}`
    );
  }
}

/**
 * Test helper for evidence export validation.
 *
 * @example
 * await testEvidenceExport(page, async (page) => {
 *   await page.goto('/export');
 *   await page.click('[data-cy="export-button"]');
 *   // Verify download
 * });
 */
export async function testEvidenceExport(
  page: Page,
  testFn: (page: Page) => Promise<void>
): Promise<void> {
  try {
    await testFn(page);
  } catch (error) {
    throw new Error(
      `Evidence export check failed: ${error instanceof Error ? error.message : String(error)}`
    );
  }
}
