/**
 * Console error catcher for Playwright tests.
 *
 * Collects console.error messages during a test and fails the test if any
 * severe (non-benign) errors are encountered. Import and wire into
 * beforeEach/afterEach or use the `withConsoleCatcher` wrapper.
 */

import type { Page } from '@playwright/test';

/** Patterns that are known benign and should not fail tests. */
const BENIGN_PATTERNS: RegExp[] = [
  // Browser extensions / devtools noise
  /failed to load resource.*favicon/i,
  /download the react devtools/i,
  // WASM-specific
  /wasm streaming compile failed/i,
  /WebAssembly.instantiateStreaming/i,
  // Network transients during test teardown
  /net::ERR_ABORTED/i,
  /net::ERR_CONNECTION_REFUSED/i,
  /failed to fetch/i,
  // SSE reconnect noise
  /EventSource.*error/i,
  // Benign Leptos hydration warnings
  /already borrowed/i,
  // ResizeObserver (browser-internal, benign)
  /ResizeObserver loop/i,
  // Service-worker registration 404 (sw.js not served in test mode)
  /bad HTTP response code \(404\).*fetching the script/i,
];

/** Hydration mismatch signatures that should always fail even if they partially match benign noise. */
const HYDRATION_MISMATCH_SEVERE_PATTERNS: RegExp[] = [
  /hydration (failed|error|mismatch)/i,
  /hydration mismatch/i,
  /hydration key/i,
  /text content does not match server-rendered html/i,
  /expected server html to contain a matching/i,
  /server rendered html.*(does not|doesn't|did not|didn't) match/i,
  /did not match.*server/i,
];

export interface ConsoleMessage {
  type: string;
  text: string;
  url: string;
}

export class ConsoleCatcher {
  private errors: ConsoleMessage[] = [];
  private handler: ((msg: import('@playwright/test').ConsoleMessage) => void) | null = null;

  /** Start capturing console.error messages on the page. */
  attach(page: Page): void {
    this.errors = [];
    this.handler = (msg) => {
      if (msg.type() === 'error') {
        const text = msg.text();
        const isSevereHydrationMismatch = HYDRATION_MISMATCH_SEVERE_PATTERNS.some((pat) =>
          pat.test(text)
        );
        const isBenign = !isSevereHydrationMismatch && BENIGN_PATTERNS.some((pat) => pat.test(text));
        if (isSevereHydrationMismatch || !isBenign) {
          this.errors.push({
            type: msg.type(),
            text,
            url: msg.location()?.url ?? '',
          });
        }
      }
    };
    page.on('console', this.handler);
  }

  /** Stop capturing and detach the listener. */
  detach(page: Page): void {
    if (this.handler) {
      page.removeListener('console', this.handler);
      this.handler = null;
    }
  }

  /** Return collected severe errors. */
  getErrors(): ConsoleMessage[] {
    return [...this.errors];
  }

  /** Clear collected errors (useful between navigation steps). */
  clear(): void {
    this.errors = [];
  }

  /**
   * Assert no severe console errors occurred. Call in afterEach.
   * Returns a formatted message if errors exist, or null if clean.
   */
  assertClean(): string | null {
    if (this.errors.length === 0) return null;
    const summary = this.errors
      .map((e, i) => `  ${i + 1}. ${e.text}${e.url ? ` (${e.url})` : ''}`)
      .join('\n');
    return `${this.errors.length} console error(s) detected:\n${summary}`;
  }
}

/**
 * Wire a ConsoleCatcher into a test's beforeEach/afterEach lifecycle.
 *
 * Usage in a spec file:
 * ```ts
 * import { test, expect } from '@playwright/test';
 * import { useConsoleCatcher } from '../helpers/console-catcher';
 *
 * const catcher = useConsoleCatcher(test);
 *
 * test('my test', async ({ page }) => {
 *   // ... test code ...
 *   // catcher auto-fails on afterEach if severe console errors occurred
 * });
 * ```
 */
export function useConsoleCatcher(
  testFn: typeof import('@playwright/test').test
): ConsoleCatcher {
  const catcher = new ConsoleCatcher();

  testFn.beforeEach(async ({ page }) => {
    catcher.attach(page);
  });

  testFn.afterEach(async ({ page }, testInfo) => {
    catcher.detach(page);
    const result = catcher.assertClean();
    if (result && testInfo.status === 'passed') {
      // Fail the test by throwing - only when the test itself passed,
      // to avoid masking the real failure with console noise.
      throw new Error(result);
    }
    catcher.clear();
  });

  return catcher;
}
