/**
 * Demo harness — narration overlay, human-like typing, and pacing helpers.
 *
 * Adapts the narration concept from var/demo-playwright/record_demo.cjs into
 * composable utilities that work within Playwright's test runner.
 */

import type { Page } from '@playwright/test';
import type { DemoContext, DemoPacing } from './types';
import { DEFAULT_PACING } from './types';

// ---------------------------------------------------------------------------
// Narration overlay
// ---------------------------------------------------------------------------

const NARRATION_ID = 'aos-demo-narration';

/** Inject a narration overlay styled per Liquid Glass Tier 3. */
export async function showNarration(page: Page, text: string): Promise<void> {
  await page.evaluate(
    ({ id, content }) => {
      let el = document.getElementById(id);
      if (!el) {
        el = document.createElement('div');
        el.id = id;
        Object.assign(el.style, {
          position: 'fixed',
          bottom: '48px',
          left: '50%',
          transform: 'translateX(-50%)',
          maxWidth: '720px',
          padding: '16px 28px',
          borderRadius: '14px',
          // Liquid Glass Tier 3: 15.6px blur, 85% alpha, white border 0.30
          backdropFilter: 'blur(15.6px)',
          WebkitBackdropFilter: 'blur(15.6px)',
          background: 'hsla(0, 0%, 10%, 0.85)',
          border: '1px solid hsla(0, 0%, 100%, 0.30)',
          color: '#f0f0f0',
          fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif',
          fontSize: '17px',
          lineHeight: '1.5',
          textAlign: 'center',
          zIndex: '2147483647',
          pointerEvents: 'none',
          transition: 'opacity 0.3s ease',
          opacity: '0',
        });
        document.body.appendChild(el);
      }
      el.textContent = content;
      // Force reflow then fade in.
      void el.offsetHeight;
      el.style.opacity = '1';
    },
    { id: NARRATION_ID, content: text }
  );
}

/** Fade out and remove the narration overlay. */
export async function hideNarration(page: Page): Promise<void> {
  await page.evaluate((id) => {
    const el = document.getElementById(id);
    if (!el) return;
    el.style.opacity = '0';
    setTimeout(() => el.remove(), 350);
  }, NARRATION_ID);
}

// ---------------------------------------------------------------------------
// Typing
// ---------------------------------------------------------------------------

/**
 * Type text character-by-character into a locator.
 * Accepts CSS selectors or `data-testid` values (auto-prefixed).
 */
export async function typeHuman(
  page: Page,
  selector: string,
  text: string,
  delayMs: number
): Promise<void> {
  const locator = selector.startsWith('[')
    ? page.locator(selector)
    : page.getByTestId(selector);
  await locator.pressSequentially(text, { delay: delayMs });
}

// ---------------------------------------------------------------------------
// Scroll
// ---------------------------------------------------------------------------

const SCROLL_SETTLE_MS = 300;

/** Scroll an element into view and pause for visual settle. */
export async function scrollIntoView(page: Page, selector: string): Promise<void> {
  const locator = selector.startsWith('[')
    ? page.locator(selector)
    : page.getByTestId(selector);
  await locator.scrollIntoViewIfNeeded();
  await page.waitForTimeout(SCROLL_SETTLE_MS);
}

// ---------------------------------------------------------------------------
// Env-based pacing overrides
// ---------------------------------------------------------------------------

/** Read pacing overrides from environment variables. */
export function pacingFromEnv(): Partial<DemoPacing> {
  const overrides: Partial<DemoPacing> = {};
  const read = (envKey: string): number | undefined => {
    const raw = process.env[envKey];
    if (!raw) return undefined;
    const parsed = Number.parseInt(raw, 10);
    return Number.isFinite(parsed) && parsed >= 0 ? parsed : undefined;
  };

  const afterNav = read('DEMO_AFTER_NAV_MS');
  if (afterNav !== undefined) overrides.afterNav = afterNav;

  const afterAction = read('DEMO_AFTER_ACTION_MS');
  if (afterAction !== undefined) overrides.afterAction = afterAction;

  const narrationDwell = read('DEMO_NARRATION_DWELL_MS');
  if (narrationDwell !== undefined) overrides.narrationDwell = narrationDwell;

  const typeDelay = read('DEMO_TYPE_DELAY_MS');
  if (typeDelay !== undefined) overrides.typeDelay = typeDelay;

  const afterType = read('DEMO_AFTER_TYPE_MS');
  if (afterType !== undefined) overrides.afterType = afterType;

  const finalDwell = read('DEMO_FINAL_DWELL_MS');
  if (finalDwell !== undefined) overrides.finalDwell = finalDwell;

  return overrides;
}

// ---------------------------------------------------------------------------
// DemoContext factory
// ---------------------------------------------------------------------------

/** Create a DemoContext binding all helpers to a page with merged pacing. */
export function createDemoContext(
  page: Page,
  pacingOverrides?: Partial<DemoPacing>
): DemoContext {
  const pacing: DemoPacing = { ...DEFAULT_PACING, ...pacingOverrides };

  return {
    page,
    pacing,

    async narrate(text: string): Promise<void> {
      await showNarration(page, text);
      await page.waitForTimeout(pacing.narrationDwell);
      await hideNarration(page);
      // Brief pause after narration fades.
      await page.waitForTimeout(350);
    },

    async typeHuman(selector: string, text: string): Promise<void> {
      await typeHuman(page, selector, text, pacing.typeDelay);
      await page.waitForTimeout(pacing.afterType);
    },

    async scrollTo(selector: string): Promise<void> {
      await scrollIntoView(page, selector);
    },

    async dwell(ms: number): Promise<void> {
      await page.waitForTimeout(ms);
    },
  };
}
