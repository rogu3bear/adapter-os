/**
 * Test Reporter for Playwright E2E Tests
 *
 * Provides comprehensive test reporting with:
 * - Markdown summary report generation
 * - Per-flow artifact capture (screenshots, console errors, API logs)
 * - Key indicator tracking (gold path, inference readiness, evidence export)
 * - Endpoint mismatch detection
 *
 * NOTE: This file should only be imported by playwright.config.ts.
 * Test files should import from ./helpers.ts and ./types.ts instead.
 */

import type {
  FullConfig,
  FullResult,
  Reporter,
  Suite,
  TestCase,
  TestResult,
  TestStep,
} from '@playwright/test/reporter';
import * as fs from 'node:fs';
import * as path from 'node:path';

// Import shared types
import type {
  FlowStatus,
  ApiLogEntry,
  ConsoleEntry,
  EndpointMismatch,
  FlowResult,
  KeyIndicators,
  TestRunReport,
  AdapterOsReporterOptions,
} from './types';

// Re-export types for convenience
export type {
  FlowStatus,
  ApiLogEntry,
  ConsoleEntry,
  EndpointMismatch,
  FlowResult,
  KeyIndicators,
  TestRunReport,
  AdapterOsReporterOptions,
};

// ============================================================================
// Artifact Collector
// ============================================================================

/**
 * Collects and manages test artifacts during execution.
 * Used within test files to capture console errors, API logs, etc.
 */
export class ArtifactCollector {
  private consoleEntries: ConsoleEntry[] = [];
  private apiLogs: ApiLogEntry[] = [];
  private endpointMismatches: EndpointMismatch[] = [];

  /** Record a console message */
  addConsoleEntry(entry: Omit<ConsoleEntry, 'timestamp'>): void {
    this.consoleEntries.push({
      ...entry,
      timestamp: new Date().toISOString(),
    });
  }

  /** Record an API request/response */
  addApiLog(entry: Omit<ApiLogEntry, 'timestamp'>): void {
    this.apiLogs.push({
      ...entry,
      timestamp: new Date().toISOString(),
    });
  }

  /** Record an endpoint mismatch */
  addEndpointMismatch(mismatch: EndpointMismatch): void {
    this.endpointMismatches.push(mismatch);
  }

  /** Get all console errors */
  getConsoleErrors(): ConsoleEntry[] {
    return this.consoleEntries.filter((entry) => entry.type === 'error');
  }

  /** Get all console entries */
  getAllConsoleEntries(): ConsoleEntry[] {
    return [...this.consoleEntries];
  }

  /** Get all API logs */
  getApiLogs(): ApiLogEntry[] {
    return [...this.apiLogs];
  }

  /** Get all endpoint mismatches */
  getEndpointMismatches(): EndpointMismatch[] {
    return [...this.endpointMismatches];
  }

  /** Clear all collected artifacts */
  clear(): void {
    this.consoleEntries = [];
    this.apiLogs = [];
    this.endpointMismatches = [];
  }
}

// ============================================================================
// Report Generator
// ============================================================================

/**
 * Generates markdown reports from test run data.
 */
export class ReportGenerator {
  private static readonly STATUS_ICONS: Record<FlowStatus, string> = {
    passed: 'PASS',
    failed: 'FAIL',
    skipped: 'SKIP',
    timedOut: 'TIMEOUT',
  };

  /** Format duration in human-readable form */
  private static formatDuration(ms: number): string {
    if (ms < 1000) {
      return `${ms}ms`;
    }
    const seconds = ms / 1000;
    if (seconds < 60) {
      return `${seconds.toFixed(1)}s`;
    }
    const minutes = Math.floor(seconds / 60);
    const remainingSeconds = seconds % 60;
    return `${minutes}m ${remainingSeconds.toFixed(0)}s`;
  }

  /** Generate markdown report from test run data */
  static generateMarkdownReport(report: TestRunReport): string {
    const lines: string[] = [];

    // Header
    lines.push('# Test Run Report');
    lines.push('');
    lines.push(`Generated: ${report.timestamp}`);
    lines.push('');

    // Summary section
    lines.push('## Summary');
    lines.push(`- Total: ${report.totalTests} tests`);
    lines.push(`- Passed: ${report.passed}`);
    lines.push(`- Failed: ${report.failed}`);
    if (report.skipped > 0) {
      lines.push(`- Skipped: ${report.skipped}`);
    }
    if (report.timedOut > 0) {
      lines.push(`- Timed Out: ${report.timedOut}`);
    }
    lines.push(`- Duration: ${this.formatDuration(report.duration)}`);
    lines.push('');

    // Flow Results table
    lines.push('## Flow Results');
    lines.push('| Flow | Status | Duration | Notes |');
    lines.push('|------|--------|----------|-------|');

    report.flowResults.forEach((flow, index) => {
      const statusIcon = this.STATUS_ICONS[flow.status];
      const duration = this.formatDuration(flow.duration);
      const notes = this.formatFlowNotes(flow);
      lines.push(`| Flow ${index}: ${this.escapeMarkdownTableCell(flow.name)} | ${statusIcon} | ${duration} | ${notes} |`);
    });
    lines.push('');

    // Key Indicators section
    lines.push('## Key Indicators');
    lines.push(`- Gold path: ${report.keyIndicators.goldPathOk ? 'PASS' : 'FAIL'}`);
    lines.push(`- Inference readiness: ${report.keyIndicators.inferenceReadinessSurfaced ? 'PASS' : 'FAIL'}`);
    lines.push(`- Evidence export: ${report.keyIndicators.evidenceExportOk ? 'PASS' : 'FAIL'}`);
    lines.push('');

    // Endpoint Mismatches section
    lines.push('## Endpoint Mismatches');
    if (report.endpointMismatches.length === 0) {
      lines.push('- None observed');
    } else {
      report.endpointMismatches.forEach((mismatch) => {
        lines.push(`- **${mismatch.testName}**: Expected \`${mismatch.expected}\`, got \`${mismatch.actual}\``);
        if (mismatch.details) {
          lines.push(`  - ${mismatch.details}`);
        }
      });
    }
    lines.push('');

    // Failed tests details
    const failedFlows = report.flowResults.filter((f) => f.status === 'failed');
    if (failedFlows.length > 0) {
      lines.push('## Failed Test Details');
      lines.push('');
      failedFlows.forEach((flow) => {
        lines.push(`### ${flow.name}`);
        if (flow.errorMessage) {
          lines.push('');
          lines.push('**Error:**');
          lines.push('```');
          lines.push(flow.errorMessage);
          lines.push('```');
        }
        if (flow.consoleErrors.length > 0) {
          lines.push('');
          lines.push('**Console Errors:**');
          flow.consoleErrors.forEach((err) => {
            lines.push(`- \`${err.text}\`${err.location ? ` (${err.location})` : ''}`);
          });
        }
        if (flow.screenshotPath) {
          lines.push('');
          lines.push(`**Screenshot:** [View](${flow.screenshotPath})`);
        }
        lines.push('');
      });
    }

    // API Log summary for failed tests
    const flowsWithApiErrors = report.flowResults.filter(
      (f) => f.status === 'failed' && f.apiLogs.some((log) => log.error || (log.status && log.status >= 400))
    );
    if (flowsWithApiErrors.length > 0) {
      lines.push('## API Errors');
      lines.push('');
      flowsWithApiErrors.forEach((flow) => {
        const errorLogs = flow.apiLogs.filter((log) => log.error || (log.status && log.status >= 400));
        if (errorLogs.length > 0) {
          lines.push(`### ${flow.name}`);
          lines.push('');
          lines.push('| Method | URL | Status | Error |');
          lines.push('|--------|-----|--------|-------|');
          errorLogs.forEach((log) => {
            const error = log.error || `HTTP ${log.status}`;
            lines.push(`| ${log.method} | ${this.truncateUrl(log.url)} | ${log.status || 'N/A'} | ${error} |`);
          });
          lines.push('');
        }
      });
    }

    return lines.join('\n');
  }

  /** Format flow notes for table cell */
  private static formatFlowNotes(flow: FlowResult): string {
    const notes: string[] = [];

    if (flow.errorMessage) {
      const shortError = flow.errorMessage.split('\n')[0].substring(0, 50);
      notes.push(`Error: ${shortError}...`);
    }

    if (flow.consoleErrors.length > 0) {
      const firstError = flow.consoleErrors[0].text.substring(0, 40);
      notes.push(`Console error: ${firstError}...`);
    }

    if (flow.retryCount > 0) {
      notes.push(`Retries: ${flow.retryCount}`);
    }

    return this.escapeMarkdownTableCell(notes.join('; ') || '');
  }

  /** Escape special characters for markdown table cells */
  private static escapeMarkdownTableCell(text: string): string {
    return text.replace(/\|/g, '\\|').replace(/\n/g, ' ');
  }

  /** Truncate URL for display */
  private static truncateUrl(url: string, maxLength: number = 50): string {
    if (url.length <= maxLength) {
      return url;
    }
    return `${url.substring(0, maxLength - 3)}...`;
  }
}

// ============================================================================
// Key Indicator Detector
// ============================================================================

/**
 * Detects key indicators from test results.
 * Analyzes test names and outcomes to determine gold path, inference readiness, etc.
 */
export class KeyIndicatorDetector {
  private static readonly GOLD_PATH_PATTERNS = [
    /gold.?path/i,
    /critical.?path/i,
    /happy.?path/i,
    /smoke/i,
    /core.?flow/i,
    /dashboard.*load/i,
    /app.*loads/i,
  ];

  private static readonly INFERENCE_PATTERNS = [
    /inference/i,
    /playground/i,
    /prompt/i,
    /backend.*status/i,
    /model.*load/i,
  ];

  private static readonly EVIDENCE_EXPORT_PATTERNS = [
    /export/i,
    /evidence/i,
    /download/i,
    /artifact/i,
  ];

  /** Detect key indicators from flow results */
  static detect(flowResults: FlowResult[]): KeyIndicators {
    return {
      goldPathOk: this.checkPatternGroup(flowResults, this.GOLD_PATH_PATTERNS),
      inferenceReadinessSurfaced: this.checkPatternGroup(flowResults, this.INFERENCE_PATTERNS),
      evidenceExportOk: this.checkPatternGroup(flowResults, this.EVIDENCE_EXPORT_PATTERNS),
    };
  }

  /** Check if any matching tests passed */
  private static checkPatternGroup(flowResults: FlowResult[], patterns: RegExp[]): boolean {
    const matchingFlows = flowResults.filter((flow) =>
      patterns.some((pattern) => pattern.test(flow.name))
    );

    // If no matching tests found, consider it as "not applicable" (true)
    if (matchingFlows.length === 0) {
      return true;
    }

    // All matching tests must pass
    return matchingFlows.every((flow) => flow.status === 'passed');
  }
}

// ============================================================================
// Playwright Reporter Implementation
// ============================================================================

/**
 * Custom Playwright Reporter for Adapter OS E2E Tests.
 *
 * Implements the Playwright Reporter interface to capture test results
 * and generate comprehensive markdown reports.
 *
 * @example
 * // playwright.config.ts
 * export default defineConfig({
 *   reporter: [
 *     ['./tests/e2e/utils/test-reporter.ts', { outputDir: 'test-results' }],
 *     ['html'],
 *   ],
 * });
 */
export default class AdapterOsReporter implements Reporter {
  private readonly options: Required<AdapterOsReporterOptions>;
  private flowResults: FlowResult[] = [];
  private endpointMismatches: EndpointMismatch[] = [];
  private startTime: number = 0;
  private config: FullConfig | null = null;

  constructor(options: AdapterOsReporterOptions = {}) {
    this.options = {
      outputDir: options.outputDir ?? 'test-results',
      reportFilename: options.reportFilename ?? 'test-report.md',
      includeApiLogs: options.includeApiLogs ?? true,
      screenshotOnFailure: options.screenshotOnFailure ?? true,
      outputJson: options.outputJson ?? false,
    };
  }

  onBegin(config: FullConfig, _suite: Suite): void {
    this.config = config;
    this.startTime = Date.now();
    this.flowResults = [];
    this.endpointMismatches = [];

    // Ensure output directory exists
    const outputPath = path.resolve(config.rootDir, this.options.outputDir);
    if (!fs.existsSync(outputPath)) {
      fs.mkdirSync(outputPath, { recursive: true });
    }
  }

  onTestBegin(_test: TestCase, _result: TestResult): void {
    // Test started - nothing to do here
  }

  onTestEnd(test: TestCase, result: TestResult): void {
    const flowResult = this.createFlowResult(test, result);
    this.flowResults.push(flowResult);

    // Extract endpoint mismatches from attachments or errors
    this.extractEndpointMismatches(test, result);
  }

  onStepBegin(_test: TestCase, _result: TestResult, _step: TestStep): void {
    // Step started - nothing to do here
  }

  onStepEnd(_test: TestCase, _result: TestResult, _step: TestStep): void {
    // Step ended - nothing to do here
  }

  async onEnd(result: FullResult): Promise<void> {
    const duration = Date.now() - this.startTime;
    const report = this.buildReport(duration);

    // Write markdown report
    await this.writeMarkdownReport(report);

    // Optionally write JSON report
    if (this.options.outputJson) {
      await this.writeJsonReport(report);
    }

    // Log summary to console
    this.logSummary(report, result);
  }

  onError(error: { message: string }): void {
    console.error('[AdapterOsReporter] Error:', error.message);
  }

  printsToStdio(): boolean {
    return true;
  }

  /** Create FlowResult from test case and result */
  private createFlowResult(test: TestCase, result: TestResult): FlowResult {
    const consoleErrors = this.extractConsoleErrors(result);
    const apiLogs = this.extractApiLogs(result);
    const screenshotPath = this.extractScreenshotPath(result);

    return {
      name: this.buildTestName(test),
      status: this.mapStatus(result.status),
      duration: result.duration,
      notes: [],
      consoleErrors,
      apiLogs,
      screenshotPath,
      errorMessage: result.error?.message,
      retryCount: result.retry,
    };
  }

  /** Build full test name including parent suite names */
  private buildTestName(test: TestCase): string {
    const parts: string[] = [];
    let parent = test.parent;
    while (parent) {
      if (parent.title) {
        parts.unshift(parent.title);
      }
      parent = parent.parent;
    }
    parts.push(test.title);
    return parts.join(' > ');
  }

  /** Map Playwright status to FlowStatus */
  private mapStatus(status: TestResult['status']): FlowStatus {
    switch (status) {
      case 'passed':
        return 'passed';
      case 'failed':
        return 'failed';
      case 'skipped':
        return 'skipped';
      case 'timedOut':
        return 'timedOut';
      case 'interrupted':
        return 'failed';
      default:
        return 'failed';
    }
  }

  /** Extract console errors from test result attachments */
  private extractConsoleErrors(result: TestResult): ConsoleEntry[] {
    const errors: ConsoleEntry[] = [];

    // Check for console errors in stdout/stderr
    if (result.stdout.length > 0) {
      result.stdout.forEach((output) => {
        const text = typeof output === 'string' ? output : output.toString('utf-8');
        if (text.toLowerCase().includes('error')) {
          errors.push({
            type: 'error',
            text: text.trim(),
            timestamp: new Date().toISOString(),
          });
        }
      });
    }

    if (result.stderr.length > 0) {
      result.stderr.forEach((output) => {
        const text = typeof output === 'string' ? output : output.toString('utf-8');
        errors.push({
          type: 'error',
          text: text.trim(),
          timestamp: new Date().toISOString(),
        });
      });
    }

    // Check attachments for console logs
    result.attachments.forEach((attachment) => {
      if (attachment.name === 'console-errors' && attachment.body) {
        try {
          const parsed = JSON.parse(attachment.body.toString('utf-8'));
          if (Array.isArray(parsed)) {
            errors.push(...parsed);
          }
        } catch {
          // Ignore parse errors
        }
      }
    });

    return errors;
  }

  /** Extract API logs from test result attachments */
  private extractApiLogs(result: TestResult): ApiLogEntry[] {
    if (!this.options.includeApiLogs) {
      return [];
    }

    const logs: ApiLogEntry[] = [];

    result.attachments.forEach((attachment) => {
      if (attachment.name === 'api-logs' && attachment.body) {
        try {
          const parsed = JSON.parse(attachment.body.toString('utf-8'));
          if (Array.isArray(parsed)) {
            logs.push(...parsed);
          }
        } catch {
          // Ignore parse errors
        }
      }
    });

    return logs;
  }

  /** Extract screenshot path from test result */
  private extractScreenshotPath(result: TestResult): string | undefined {
    const screenshot = result.attachments.find(
      (att) => att.name === 'screenshot' && att.contentType.startsWith('image/')
    );
    return screenshot?.path;
  }

  /** Extract endpoint mismatches from test result */
  private extractEndpointMismatches(test: TestCase, result: TestResult): void {
    // Check for endpoint mismatches in attachments
    result.attachments.forEach((attachment) => {
      if (attachment.name === 'endpoint-mismatches' && attachment.body) {
        try {
          const parsed = JSON.parse(attachment.body.toString('utf-8'));
          if (Array.isArray(parsed)) {
            parsed.forEach((mismatch: EndpointMismatch) => {
              this.endpointMismatches.push({
                ...mismatch,
                testName: this.buildTestName(test),
              });
            });
          }
        } catch {
          // Ignore parse errors
        }
      }
    });

    // Also check error message for common endpoint mismatch patterns
    if (result.error?.message) {
      const mismatchPattern = /expected.*?(\S+).*?(?:but|got|received).*?(\S+)/i;
      const match = result.error.message.match(mismatchPattern);
      if (match && (match[1].includes('/') || match[2].includes('/'))) {
        this.endpointMismatches.push({
          expected: match[1],
          actual: match[2],
          testName: this.buildTestName(test),
          details: result.error.message.split('\n')[0],
        });
      }
    }
  }

  /** Build complete test report */
  private buildReport(duration: number): TestRunReport {
    const counts = this.flowResults.reduce(
      (acc, flow) => {
        acc[flow.status]++;
        return acc;
      },
      { passed: 0, failed: 0, skipped: 0, timedOut: 0 }
    );

    return {
      timestamp: new Date().toISOString(),
      totalTests: this.flowResults.length,
      passed: counts.passed,
      failed: counts.failed,
      skipped: counts.skipped,
      timedOut: counts.timedOut,
      duration,
      flowResults: this.flowResults,
      keyIndicators: KeyIndicatorDetector.detect(this.flowResults),
      endpointMismatches: this.endpointMismatches,
    };
  }

  /** Write markdown report to file */
  private async writeMarkdownReport(report: TestRunReport): Promise<void> {
    if (!this.config) return;

    const outputPath = path.resolve(
      this.config.rootDir,
      this.options.outputDir,
      this.options.reportFilename
    );
    const markdown = ReportGenerator.generateMarkdownReport(report);

    await fs.promises.writeFile(outputPath, markdown, 'utf-8');
  }

  /** Write JSON report to file */
  private async writeJsonReport(report: TestRunReport): Promise<void> {
    if (!this.config) return;

    const jsonFilename = this.options.reportFilename.replace(/\.md$/, '.json');
    const outputPath = path.resolve(
      this.config.rootDir,
      this.options.outputDir,
      jsonFilename
    );

    await fs.promises.writeFile(outputPath, JSON.stringify(report, null, 2), 'utf-8');
  }

  /** Log summary to console */
  private logSummary(report: TestRunReport, result: FullResult): void {
    console.log('\n');
    console.log('='.repeat(60));
    console.log('  ADAPTER OS TEST REPORT');
    console.log('='.repeat(60));
    console.log('');
    console.log(`  Total:    ${report.totalTests} tests`);
    console.log(`  Passed:   ${report.passed}`);
    console.log(`  Failed:   ${report.failed}`);
    if (report.skipped > 0) console.log(`  Skipped:  ${report.skipped}`);
    if (report.timedOut > 0) console.log(`  Timed Out: ${report.timedOut}`);
    console.log('');
    console.log('  Key Indicators:');
    console.log(`    Gold path:           ${report.keyIndicators.goldPathOk ? 'PASS' : 'FAIL'}`);
    console.log(`    Inference readiness: ${report.keyIndicators.inferenceReadinessSurfaced ? 'PASS' : 'FAIL'}`);
    console.log(`    Evidence export:     ${report.keyIndicators.evidenceExportOk ? 'PASS' : 'FAIL'}`);
    console.log('');
    if (report.endpointMismatches.length > 0) {
      console.log(`  Endpoint Mismatches: ${report.endpointMismatches.length} detected`);
    }
    console.log('');
    console.log(`  Result: ${result.status.toUpperCase()}`);
    console.log('='.repeat(60));
    console.log('');
  }
}
