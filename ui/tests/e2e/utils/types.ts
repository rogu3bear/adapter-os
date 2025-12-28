/**
 * Shared Types for E2E Test Utilities
 *
 * These types are used by both the reporter and test fixtures.
 * Keep this file free of any Playwright test imports to avoid
 * circular dependency issues with the reporter.
 */

// ============================================================================
// Core Types
// ============================================================================

/** Status indicator for key test flows */
export type FlowStatus = 'passed' | 'failed' | 'skipped' | 'timedOut';

/** API request/response log entry */
export interface ApiLogEntry {
  timestamp: string;
  method: string;
  url: string;
  status?: number;
  duration?: number;
  requestBody?: unknown;
  responseBody?: unknown;
  error?: string;
}

/** Console message captured during test execution */
export interface ConsoleEntry {
  type: 'log' | 'error' | 'warn' | 'info' | 'debug';
  text: string;
  location?: string;
  timestamp: string;
}

/** Endpoint mismatch detected during test execution */
export interface EndpointMismatch {
  expected: string;
  actual: string;
  testName: string;
  details?: string;
}

/** Per-flow test result with artifacts */
export interface FlowResult {
  name: string;
  status: FlowStatus;
  duration: number;
  notes: string[];
  consoleErrors: ConsoleEntry[];
  apiLogs: ApiLogEntry[];
  screenshotPath?: string;
  errorMessage?: string;
  retryCount: number;
}

/** Key indicators for test run summary */
export interface KeyIndicators {
  goldPathOk: boolean;
  inferenceReadinessSurfaced: boolean;
  evidenceExportOk: boolean;
}

/** Complete test run report data */
export interface TestRunReport {
  timestamp: string;
  totalTests: number;
  passed: number;
  failed: number;
  skipped: number;
  timedOut: number;
  duration: number;
  flowResults: FlowResult[];
  keyIndicators: KeyIndicators;
  endpointMismatches: EndpointMismatch[];
}

/** Configuration options for the reporter */
export interface AdapterOsReporterOptions {
  /** Output directory for reports (default: 'test-results') */
  outputDir?: string;
  /** Report filename (default: 'test-report.md') */
  reportFilename?: string;
  /** Whether to include API logs in report (default: true) */
  includeApiLogs?: boolean;
  /** Whether to capture screenshots on failure (default: true) */
  screenshotOnFailure?: boolean;
  /** Whether to output JSON report alongside markdown (default: false) */
  outputJson?: boolean;
}
