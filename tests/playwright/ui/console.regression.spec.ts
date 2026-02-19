/**
 * Console regression guard.
 *
 * Fails if console warnings exceed threshold after UI load.
 * Prevents signal-tracking and other noise from creeping back.
 */

import { test, expect } from '@playwright/test';
import { gotoAndBootstrap } from './utils';

/** Max allowed console warnings during load + settle. */
const WARN_THRESHOLD = 15;

/** Benign warning patterns (e.g. font decode, integrity, known browser quirks). */
const BENIGN_WARN_PATTERNS: RegExp[] = [
  /OTS parsing error/i,
  /decode failed.*font/i,
  /integrity.*preload/i,
  /favicon/i,
];

test('console warnings stay under threshold', { tag: ['@smoke', '@console'] }, async ({ page }) => {
  test.setTimeout(60_000);

  const warnings: { text: string; type: string }[] = [];

  page.on('console', (msg) => {
    const type = msg.type();
    if (type === 'warning' || type === 'warn') {
      const text = msg.text();
      const isBenign = BENIGN_WARN_PATTERNS.some((p) => p.test(text));
      if (!isBenign) {
        warnings.push({ text, type });
      }
    }
  });

  await gotoAndBootstrap(page, '/dashboard', { mode: 'ui-only' });
  // Allow UI to settle (SSE, metrics, status center, etc.)
  await page.waitForTimeout(3_000);

  const count = warnings.length;
  if (count > WARN_THRESHOLD) {
    const sample = warnings.slice(0, 10).map((w) => `  - ${w.text.slice(0, 120)}`).join('\n');
    throw new Error(
      `Console warnings (${count}) exceed threshold (${WARN_THRESHOLD}). Sample:\n${sample}`
    );
  }
});
