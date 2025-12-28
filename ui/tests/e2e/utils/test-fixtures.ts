/**
 * Enhanced Test Fixtures for Playwright E2E Tests
 *
 * Provides pre-configured fixtures that automatically capture:
 * - Console errors
 * - API request/response logs
 * - Endpoint mismatches
 *
 * These are automatically attached to test results for the reporter.
 *
 * NOTE: This file should only be imported by test files (.spec.ts),
 * NOT by the reporter or config files.
 */

import { test as base, type Page } from '@playwright/test';
import type { ConsoleEntry, ApiLogEntry, EndpointMismatch } from './types';
import {
  createConsoleCollector,
  createApiLogger,
  attachArtifacts,
  waitForLoadingComplete,
  assertNoConsoleErrors,
  assertNoApiErrors,
  createMockHandler,
  assertSchemaVersion,
  testGoldPath,
  testInferenceReadiness,
  testEvidenceExport,
} from './helpers';

// Re-export helper functions
export {
  waitForLoadingComplete,
  assertNoConsoleErrors,
  assertNoApiErrors,
  createMockHandler,
  assertSchemaVersion,
  testGoldPath,
  testInferenceReadiness,
  testEvidenceExport,
};

// Re-export types
export type { ConsoleEntry, ApiLogEntry, EndpointMismatch };

// ============================================================================
// Extended Fixture Types
// ============================================================================

export interface TestFixtures {
  /** Console message collector - automatically attached to test results */
  consoleCollector: ReturnType<typeof createConsoleCollector>;
  /** API request logger - automatically attached to test results */
  apiLogger: ReturnType<typeof createApiLogger>;
  /** Endpoint mismatch tracker for manual reporting */
  endpointTracker: {
    add: (mismatch: Omit<EndpointMismatch, 'testName'>) => void;
    getAll: () => EndpointMismatch[];
  };
  /** Enhanced page with guards already attached */
  guardedPage: Page;
}

// ============================================================================
// Enhanced Test with Fixtures
// ============================================================================

/**
 * Enhanced test function with automatic artifact collection.
 *
 * @example
 * import { test, expect } from './utils/test-fixtures';
 *
 * test('my test with automatic artifact capture', async ({ guardedPage, consoleCollector }) => {
 *   await guardedPage.goto('/dashboard');
 *   // ... test actions ...
 *   // Console errors are automatically captured and reported
 * });
 */
export const test = base.extend<TestFixtures>({
  consoleCollector: async ({ page }, use, testInfo) => {
    const collector = createConsoleCollector(page);
    await use(collector);

    // Attach console errors to test results after test completes
    const errors = collector.getErrors();
    if (errors.length > 0 || testInfo.status !== 'passed') {
      await attachArtifacts(testInfo, { consoleErrors: errors });
    }
  },

  apiLogger: async ({ page }, use, testInfo) => {
    const logger = createApiLogger(page);
    await use(logger);

    // Attach API logs to test results after test completes
    const logs = logger.getLogs();
    const errors = logger.getErrors();
    if (logs.length > 0 || errors.length > 0) {
      await attachArtifacts(testInfo, { apiLogs: logs });
    }
  },

  endpointTracker: async ({ page: _page }, use, testInfo) => {
    const mismatches: EndpointMismatch[] = [];
    const tracker = {
      add: (mismatch: Omit<EndpointMismatch, 'testName'>) => {
        mismatches.push({
          ...mismatch,
          testName: testInfo.title,
        });
      },
      getAll: () => [...mismatches],
    };

    await use(tracker);

    // Attach endpoint mismatches to test results
    if (mismatches.length > 0) {
      await attachArtifacts(testInfo, { endpointMismatches: mismatches });
    }
  },

  guardedPage: async ({ page, consoleCollector: _collector, apiLogger: _logger }, use) => {
    // The collectors are already attached via their own fixtures
    // Just provide the page with all guards active
    await use(page);
  },
});

// Re-export expect for convenience
export { expect } from '@playwright/test';
