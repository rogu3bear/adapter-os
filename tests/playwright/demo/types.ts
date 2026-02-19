/**
 * Demo operation types and pacing configuration.
 *
 * Types only — no runtime code. Imported by harness, mocks, and operations.
 */

import type { Page } from '@playwright/test';

// ---------------------------------------------------------------------------
// Pacing
// ---------------------------------------------------------------------------

export interface DemoPacing {
  /** Delay after navigating to a new page (ms). */
  afterNav: number;
  /** Delay after a UI action (click, select, etc.) (ms). */
  afterAction: number;
  /** How long a narration overlay stays visible (ms). */
  narrationDwell: number;
  /** Per-character delay for simulated typing (ms). */
  typeDelay: number;
  /** Delay after typing completes (ms). */
  afterType: number;
  /** Final dwell at the end of an operation (ms). */
  finalDwell: number;
}

export const DEFAULT_PACING: DemoPacing = {
  afterNav: 1200,
  afterAction: 800,
  narrationDwell: 2500,
  typeDelay: 60,
  afterType: 600,
  finalDwell: 2000,
};

// ---------------------------------------------------------------------------
// Mock presets
// ---------------------------------------------------------------------------

export type MockPreset =
  | 'system-ready'
  | 'infer-stream'
  | 'trace-detail'
  | 'replay'
  | 'adapters-list'
  | 'training-status'
  | 'documents-list'
  | 'datasets-list';

// ---------------------------------------------------------------------------
// Demo context (passed to operations)
// ---------------------------------------------------------------------------

export interface DemoContext {
  page: Page;
  pacing: DemoPacing;

  /** Show a narration overlay, dwell for `pacing.narrationDwell`, then hide. */
  narrate(text: string): Promise<void>;

  /** Type text character-by-character into a selector. */
  typeHuman(selector: string, text: string): Promise<void>;

  /** Scroll an element into view with a settle pause. */
  scrollTo(selector: string): Promise<void>;

  /** Pause for a fixed duration. */
  dwell(ms: number): Promise<void>;
}

// ---------------------------------------------------------------------------
// Operation metadata
// ---------------------------------------------------------------------------

export interface DemoOperationMeta {
  /** Unique identifier, also used as the Playwright test name. */
  id: string;
  /** Human-readable title shown in reports. */
  title: string;
  /** Mock presets this operation requires. */
  mocks: MockPreset[];
  /** Optional tags for filtering (e.g. 'chat', 'training'). */
  tags?: string[];
}
