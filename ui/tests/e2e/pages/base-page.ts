/**
 * Base Page Object
 *
 * Abstract base class providing common methods and utilities for all page objects.
 * Implements standard patterns for navigation, waiting, assertions, and element interactions.
 */

import { type Page, type Locator, expect } from '@playwright/test';

export interface WaitOptions {
  timeout?: number;
  state?: 'attached' | 'detached' | 'visible' | 'hidden';
}

export interface NavigationOptions {
  waitUntil?: 'load' | 'domcontentloaded' | 'networkidle' | 'commit';
  timeout?: number;
}

export abstract class BasePage {
  readonly page: Page;

  /** Default timeout for page operations in milliseconds */
  protected readonly defaultTimeout = 30_000;

  constructor(page: Page) {
    this.page = page;
  }

  /**
   * Abstract method that subclasses must implement to define the page URL.
   */
  abstract get url(): string;

  /**
   * Navigate to this page.
   */
  async goto(options?: NavigationOptions): Promise<void> {
    await this.page.goto(this.url, {
      waitUntil: options?.waitUntil ?? 'domcontentloaded',
      timeout: options?.timeout ?? this.defaultTimeout,
    });
  }

  /**
   * Wait for the page to be fully loaded and ready for interaction.
   * Subclasses should override this to wait for page-specific elements.
   */
  async waitForReady(options?: WaitOptions): Promise<void> {
    await this.page.waitForLoadState('domcontentloaded', {
      timeout: options?.timeout ?? this.defaultTimeout,
    });
  }

  /**
   * Check if the page is currently displayed.
   */
  async isDisplayed(): Promise<boolean> {
    const currentUrl = this.page.url();
    return currentUrl.includes(this.url);
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // Element Interaction Helpers
  // ─────────────────────────────────────────────────────────────────────────────

  /**
   * Get a locator by test ID (data-testid attribute).
   */
  getByTestId(testId: string): Locator {
    return this.page.getByTestId(testId);
  }

  /**
   * Get a locator by Cypress-style test ID (data-cy attribute).
   */
  getByCy(cy: string): Locator {
    return this.page.locator(`[data-cy="${cy}"]`);
  }

  /**
   * Get a locator by role with accessible name.
   */
  getByRole(
    role: Parameters<Page['getByRole']>[0],
    options?: Parameters<Page['getByRole']>[1]
  ): Locator {
    return this.page.getByRole(role, options);
  }

  /**
   * Get a locator by text content.
   */
  getByText(text: string | RegExp, options?: { exact?: boolean }): Locator {
    return this.page.getByText(text, options);
  }

  /**
   * Get a locator by label text (for form elements).
   */
  getByLabel(text: string | RegExp, options?: { exact?: boolean }): Locator {
    return this.page.getByLabel(text, options);
  }

  /**
   * Get a locator by placeholder text.
   */
  getByPlaceholder(text: string | RegExp, options?: { exact?: boolean }): Locator {
    return this.page.getByPlaceholder(text, options);
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // Wait Helpers
  // ─────────────────────────────────────────────────────────────────────────────

  /**
   * Wait for an element to be visible.
   */
  async waitForVisible(locator: Locator, options?: WaitOptions): Promise<void> {
    await locator.waitFor({
      state: 'visible',
      timeout: options?.timeout ?? this.defaultTimeout,
    });
  }

  /**
   * Wait for an element to be hidden.
   */
  async waitForHidden(locator: Locator, options?: WaitOptions): Promise<void> {
    await locator.waitFor({
      state: 'hidden',
      timeout: options?.timeout ?? this.defaultTimeout,
    });
  }

  /**
   * Wait for an element to be attached to the DOM.
   */
  async waitForAttached(locator: Locator, options?: WaitOptions): Promise<void> {
    await locator.waitFor({
      state: 'attached',
      timeout: options?.timeout ?? this.defaultTimeout,
    });
  }

  /**
   * Wait for an element to be detached from the DOM.
   */
  async waitForDetached(locator: Locator, options?: WaitOptions): Promise<void> {
    await locator.waitFor({
      state: 'detached',
      timeout: options?.timeout ?? this.defaultTimeout,
    });
  }

  /**
   * Wait for network to be idle (useful after triggering actions).
   */
  async waitForNetworkIdle(options?: { timeout?: number }): Promise<void> {
    await this.page.waitForLoadState('networkidle', {
      timeout: options?.timeout ?? this.defaultTimeout,
    });
  }

  /**
   * Wait for a specific URL pattern.
   */
  async waitForUrl(
    url: string | RegExp | ((url: URL) => boolean),
    options?: { timeout?: number }
  ): Promise<void> {
    await this.page.waitForURL(url, {
      timeout: options?.timeout ?? this.defaultTimeout,
    });
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // Action Helpers
  // ─────────────────────────────────────────────────────────────────────────────

  /**
   * Click an element and wait for navigation.
   */
  async clickAndWaitForNavigation(
    locator: Locator,
    options?: NavigationOptions
  ): Promise<void> {
    await Promise.all([
      this.page.waitForURL(/.*/, {
        timeout: options?.timeout ?? this.defaultTimeout,
        waitUntil: options?.waitUntil ?? 'domcontentloaded',
      }),
      locator.click(),
    ]);
  }

  /**
   * Fill a form field and optionally blur it.
   */
  async fillField(
    locator: Locator,
    value: string,
    options?: { blur?: boolean }
  ): Promise<void> {
    await locator.fill(value);
    if (options?.blur) {
      await locator.blur();
    }
  }

  /**
   * Clear a form field.
   */
  async clearField(locator: Locator): Promise<void> {
    await locator.clear();
  }

  /**
   * Select an option from a select element.
   */
  async selectOption(
    locator: Locator,
    value: string | { label?: string; value?: string; index?: number }
  ): Promise<void> {
    await locator.selectOption(value);
  }

  /**
   * Check a checkbox or radio button.
   */
  async check(locator: Locator): Promise<void> {
    await locator.check();
  }

  /**
   * Uncheck a checkbox.
   */
  async uncheck(locator: Locator): Promise<void> {
    await locator.uncheck();
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // Assertion Helpers
  // ─────────────────────────────────────────────────────────────────────────────

  /**
   * Assert that an element is visible.
   */
  async expectVisible(locator: Locator): Promise<void> {
    await expect(locator).toBeVisible();
  }

  /**
   * Assert that an element is hidden.
   */
  async expectHidden(locator: Locator): Promise<void> {
    await expect(locator).toBeHidden();
  }

  /**
   * Assert that an element is enabled.
   */
  async expectEnabled(locator: Locator): Promise<void> {
    await expect(locator).toBeEnabled();
  }

  /**
   * Assert that an element is disabled.
   */
  async expectDisabled(locator: Locator): Promise<void> {
    await expect(locator).toBeDisabled();
  }

  /**
   * Assert that an element contains specific text.
   */
  async expectText(locator: Locator, text: string | RegExp): Promise<void> {
    await expect(locator).toContainText(text);
  }

  /**
   * Assert that an element has specific value (for inputs).
   */
  async expectValue(locator: Locator, value: string | RegExp): Promise<void> {
    await expect(locator).toHaveValue(value);
  }

  /**
   * Assert the page title.
   */
  async expectTitle(title: string | RegExp): Promise<void> {
    await expect(this.page).toHaveTitle(title);
  }

  /**
   * Assert the current URL.
   */
  async expectUrl(url: string | RegExp): Promise<void> {
    await expect(this.page).toHaveURL(url);
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // Screenshot & Debugging
  // ─────────────────────────────────────────────────────────────────────────────

  /**
   * Take a screenshot of the page.
   */
  async screenshot(name: string): Promise<Buffer> {
    return this.page.screenshot({ path: `screenshots/${name}.png`, fullPage: true });
  }

  /**
   * Get the page content as HTML.
   */
  async getContent(): Promise<string> {
    return this.page.content();
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // Toast / Notification Helpers
  // ─────────────────────────────────────────────────────────────────────────────

  /**
   * Get toast notifications (using Sonner toast library).
   */
  get toasts(): Locator {
    return this.page.locator('[data-sonner-toast]');
  }

  /**
   * Wait for a toast with specific text to appear.
   */
  async waitForToast(text: string | RegExp, options?: WaitOptions): Promise<Locator> {
    const toast = this.toasts.filter({ hasText: text });
    await toast.waitFor({
      state: 'visible',
      timeout: options?.timeout ?? this.defaultTimeout,
    });
    return toast;
  }

  /**
   * Assert that a success toast is shown.
   */
  async expectSuccessToast(text: string | RegExp): Promise<void> {
    const toast = await this.waitForToast(text);
    await expect(toast).toBeVisible();
  }

  /**
   * Assert that an error toast is shown.
   */
  async expectErrorToast(text: string | RegExp): Promise<void> {
    const toast = await this.waitForToast(text);
    await expect(toast).toBeVisible();
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // Common UI Components
  // ─────────────────────────────────────────────────────────────────────────────

  /**
   * Get loading spinner elements.
   */
  get loadingSpinner(): Locator {
    return this.page.locator('.animate-spin');
  }

  /**
   * Get skeleton loading placeholders.
   */
  get skeletons(): Locator {
    return this.page.locator('.animate-pulse');
  }

  /**
   * Wait for loading to complete (no spinners or skeletons).
   */
  async waitForLoadingComplete(options?: WaitOptions): Promise<void> {
    await this.waitForHidden(this.loadingSpinner, options);
    await this.waitForHidden(this.skeletons, options);
  }

  /**
   * Get all badge elements.
   */
  getBadge(text: string | RegExp): Locator {
    return this.page.locator('[class*="badge"]').filter({ hasText: text });
  }

  /**
   * Get a card by its title.
   */
  getCard(title: string | RegExp): Locator {
    return this.page
      .locator('[class*="card"]')
      .filter({ has: this.page.locator('[class*="card-title"]', { hasText: title }) });
  }

  /**
   * Get a button by its text content.
   */
  getButton(text: string | RegExp): Locator {
    return this.page.getByRole('button', { name: text });
  }

  /**
   * Get a link by its text content.
   */
  getLink(text: string | RegExp): Locator {
    return this.page.getByRole('link', { name: text });
  }

  /**
   * Get an alert by variant.
   */
  getAlert(variant?: 'default' | 'destructive' | 'warning'): Locator {
    if (variant) {
      return this.page.locator(`[data-variant="${variant}"], [role="alert"]`);
    }
    return this.page.locator('[role="alert"]');
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // Dialog / Modal Helpers
  // ─────────────────────────────────────────────────────────────────────────────

  /**
   * Get the currently open dialog.
   */
  get dialog(): Locator {
    return this.page.getByRole('dialog');
  }

  /**
   * Get the sheet (side panel).
   */
  get sheet(): Locator {
    return this.page.locator('[data-state="open"][role="dialog"]');
  }

  /**
   * Close the currently open dialog.
   */
  async closeDialog(): Promise<void> {
    const closeButton = this.dialog.locator('button[aria-label="Close"], button:has-text("Close")');
    if (await closeButton.isVisible()) {
      await closeButton.click();
    } else {
      await this.page.keyboard.press('Escape');
    }
    await this.waitForHidden(this.dialog);
  }

  /**
   * Close the currently open sheet.
   */
  async closeSheet(): Promise<void> {
    const closeButton = this.sheet.locator('button[aria-label="Close"], button:has-text("Close")');
    if (await closeButton.isVisible()) {
      await closeButton.click();
    } else {
      await this.page.keyboard.press('Escape');
    }
    await this.waitForHidden(this.sheet);
  }
}

export default BasePage;
