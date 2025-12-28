/**
 * E2E Test Utilities Index
 *
 * Re-exports all test utilities for convenient imports.
 *
 * @example
 * import {
 *   test,
 *   expect,
 *   createConsoleCollector,
 *   createApiLogger,
 *   waitForLoadingComplete,
 * } from './utils';
 */

// Reporter utilities
export {
  // Types
  type FlowStatus,
  type ApiLogEntry,
  type ConsoleEntry,
  type EndpointMismatch,
  type FlowResult,
  type KeyIndicators,
  type TestRunReport,
  type AdapterOsReporterOptions,
  // Classes
  ArtifactCollector,
  ReportGenerator,
  KeyIndicatorDetector,
  // Helper functions
  createConsoleCollector,
  createApiLogger,
  attachArtifacts,
} from './test-reporter';

// Test fixtures
export {
  // Enhanced test function with fixtures
  test,
  expect,
  // Helper types
  type TestFixtures,
  // Helper functions
  waitForLoadingComplete,
  assertNoConsoleErrors,
  assertNoApiErrors,
  createMockHandler,
  assertSchemaVersion,
  testGoldPath,
  testInferenceReadiness,
  testEvidenceExport,
} from './test-fixtures';

// Re-export the reporter as default for Playwright config
export { default as AdapterOsReporter } from './test-reporter';
