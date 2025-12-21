/**
 * Mock Appliers
 *
 * Utilities for creating mutable mock state that works with vi.mock() hoisting.
 *
 * NOTE: vi.mock() calls must be at module scope (they are hoisted by Vitest).
 * These utilities create mutable state objects that vi.mock() can reference,
 * allowing per-test customization via .update() and .reset() methods.
 */

export * from './auth';
