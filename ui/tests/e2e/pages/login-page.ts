/**
 * Login Page Object
 *
 * Page object for the AdapterOS login page.
 * Located at `/` when user is not authenticated.
 *
 * Features:
 * - Email/password login form
 * - TOTP (MFA) field support
 * - Dev bypass section (enabled via VITE_ENABLE_DEV_BYPASS)
 * - SystemHealthPanel showing backend status
 */

import { type Page, type Locator, expect } from '@playwright/test';
import { BasePage, type WaitOptions } from './base-page';

export type SystemHealthStatus = 'healthy' | 'degraded' | 'unhealthy' | 'unknown';

export interface LoginCredentials {
  email: string;
  password: string;
  totp?: string;
}

export class LoginPage extends BasePage {
  // ─────────────────────────────────────────────────────────────────────────────
  // Locators - Login Form
  // ─────────────────────────────────────────────────────────────────────────────

  /** Email input field */
  get emailInput(): Locator {
    return this.page.locator('[data-testid="login-email"], [data-cy="login-email"]');
  }

  /** Password input field */
  get passwordInput(): Locator {
    return this.page.locator('[data-testid="login-password"], [data-cy="login-password"]');
  }

  /** TOTP code input field */
  get totpInput(): Locator {
    return this.page.locator('[data-cy="login-totp"]');
  }

  /** Submit button */
  get submitButton(): Locator {
    return this.page.locator('[data-testid="login-submit"], [data-cy="login-submit"]');
  }

  /** "Use TOTP code" button to show TOTP field */
  get showTotpButton(): Locator {
    return this.getButton(/use totp code/i);
  }

  /** Login form container */
  get loginForm(): Locator {
    return this.page.locator('form[aria-label="Login form"]');
  }

  /** Email field error message */
  get emailError(): Locator {
    return this.page.locator('#email-error');
  }

  /** Password field error message */
  get passwordError(): Locator {
    return this.page.locator('#password-error');
  }

  /** TOTP field error message */
  get totpError(): Locator {
    return this.page.locator('#totp-error');
  }

  /** Login error alert (authentication failures) */
  get errorAlert(): Locator {
    return this.loginForm.locator('[role="alert"][class*="destructive"]');
  }

  /** Lockout alert (too many failed attempts) */
  get lockoutAlert(): Locator {
    return this.loginForm.locator('[role="alert"]').filter({ hasText: /locked|too many/i });
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // Locators - Dev Bypass Section
  // ─────────────────────────────────────────────────────────────────────────────

  /** Dev bypass section container */
  get devBypassSection(): Locator {
    return this.page.locator('section').filter({ hasText: /development mode/i });
  }

  /** Dev bypass button */
  get devBypassButton(): Locator {
    return this.getButton(/use dev bypass/i);
  }

  /** Dev bypass error alert */
  get devBypassError(): Locator {
    return this.devBypassSection.locator('[role="alert"]');
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // Locators - System Health Panel
  // ─────────────────────────────────────────────────────────────────────────────

  /** System health panel container */
  get systemHealthPanel(): Locator {
    return this.page.locator('section').filter({ hasText: /system status/i });
  }

  /** Control plane status section (in sidebar) */
  get controlPlaneStatus(): Locator {
    return this.page.locator('aside').filter({ hasText: /control plane/i });
  }

  /** System status badge showing overall health */
  get systemStatusBadge(): Locator {
    return this.controlPlaneStatus.locator('span.rounded-full').first();
  }

  /** Refresh status button */
  get refreshStatusButton(): Locator {
    return this.systemHealthPanel.getByRole('button', { name: /refresh/i });
  }

  /** Details toggle button */
  get detailsButton(): Locator {
    return this.systemHealthPanel.getByRole('button', { name: /details/i });
  }

  /** Hide details button */
  get hideDetailsButton(): Locator {
    return this.systemHealthPanel.getByRole('button', { name: /hide/i });
  }

  /** Health details panel (expanded view) */
  get healthDetailsPanel(): Locator {
    return this.systemHealthPanel.locator('.rounded-md.border.bg-muted');
  }

  /** Health component rows in details panel */
  get healthComponentRows(): Locator {
    return this.healthDetailsPanel.locator('div.flex.items-center.justify-between');
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // Locators - Page Structure
  // ─────────────────────────────────────────────────────────────────────────────

  /** Main page container */
  get mainContainer(): Locator {
    return this.page.locator('main');
  }

  /** Page header with AdapterOS branding */
  get pageHeader(): Locator {
    return this.page.locator('header');
  }

  /** AdapterOS logo/title */
  get pageTitle(): Locator {
    return this.page.getByRole('heading', { name: /adapteros/i });
  }

  /** "Welcome back" heading in login section */
  get welcomeHeading(): Locator {
    return this.page.getByRole('heading', { name: /welcome back/i });
  }

  /** System starting message (when waiting for services) */
  get systemStartingMessage(): Locator {
    return this.page.getByRole('heading', { name: /system starting/i });
  }

  /** Config loading message */
  get configLoadingMessage(): Locator {
    return this.page.getByRole('heading', { name: /preparing sign-in/i });
  }

  /** Control plane unavailable error */
  get controlPlaneUnavailable(): Locator {
    return this.page.locator('[class*="fetch-error"]').filter({ hasText: /control plane unavailable/i });
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // Page URL
  // ─────────────────────────────────────────────────────────────────────────────

  get url(): string {
    return '/';
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // Actions - Authentication
  // ─────────────────────────────────────────────────────────────────────────────

  /**
   * Wait for the login page to be ready for interaction.
   */
  async waitForReady(options?: WaitOptions): Promise<void> {
    await super.waitForReady(options);
    // Wait for either login form or system starting message
    await this.page.waitForSelector(
      '[data-testid="login-email"], [data-cy="login-email"], h2:has-text("System starting")',
      { timeout: options?.timeout ?? this.defaultTimeout }
    );
  }

  /**
   * Wait for the login form to be visible and ready.
   */
  async waitForLoginForm(options?: WaitOptions): Promise<void> {
    await this.waitForVisible(this.emailInput, options);
    await this.waitForVisible(this.passwordInput, options);
    await this.waitForVisible(this.submitButton, options);
  }

  /**
   * Fill in login credentials.
   */
  async fillCredentials(credentials: LoginCredentials): Promise<void> {
    await this.fillField(this.emailInput, credentials.email);
    await this.fillField(this.passwordInput, credentials.password);

    if (credentials.totp) {
      // Show TOTP field if not already visible
      if (!(await this.totpInput.isVisible())) {
        await this.showTotpButton.click();
        await this.waitForVisible(this.totpInput);
      }
      await this.fillField(this.totpInput, credentials.totp);
    }
  }

  /**
   * Submit the login form.
   */
  async submitLogin(): Promise<void> {
    await this.submitButton.click();
  }

  /**
   * Perform complete login flow.
   */
  async login(credentials: LoginCredentials): Promise<void> {
    await this.fillCredentials(credentials);
    await this.submitLogin();
  }

  /**
   * Login and wait for navigation to dashboard.
   */
  async loginAndWaitForDashboard(credentials: LoginCredentials): Promise<void> {
    await this.login(credentials);
    await this.waitForUrl(/\/(dashboard|home|chat)/i);
  }

  /**
   * Use dev bypass to authenticate (development mode only).
   */
  async useDevBypass(): Promise<void> {
    await expect(this.devBypassSection).toBeVisible();
    await this.devBypassButton.click();
  }

  /**
   * Use dev bypass and wait for navigation.
   */
  async devBypassAndWaitForDashboard(): Promise<void> {
    await this.useDevBypass();
    await this.waitForUrl(/\/(dashboard|home|chat)/i);
  }

  /**
   * Show the TOTP field.
   */
  async showTotpField(): Promise<void> {
    if (!(await this.totpInput.isVisible())) {
      await this.showTotpButton.click();
      await this.waitForVisible(this.totpInput);
    }
  }

  /**
   * Clear all login form fields.
   */
  async clearForm(): Promise<void> {
    await this.clearField(this.emailInput);
    await this.clearField(this.passwordInput);
    if (await this.totpInput.isVisible()) {
      await this.clearField(this.totpInput);
    }
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // Actions - System Health
  // ─────────────────────────────────────────────────────────────────────────────

  /**
   * Refresh the system health status.
   */
  async refreshSystemHealth(): Promise<void> {
    await this.refreshStatusButton.click();
  }

  /**
   * Expand the health details panel.
   */
  async expandHealthDetails(): Promise<void> {
    if (!(await this.healthDetailsPanel.isVisible())) {
      await this.detailsButton.click();
      await this.waitForVisible(this.healthDetailsPanel);
    }
  }

  /**
   * Collapse the health details panel.
   */
  async collapseHealthDetails(): Promise<void> {
    if (await this.healthDetailsPanel.isVisible()) {
      await this.hideDetailsButton.click();
      await this.waitForHidden(this.healthDetailsPanel);
    }
  }

  /**
   * Get the current system health status text.
   */
  async getSystemHealthStatus(): Promise<string> {
    const statusText = await this.systemStatusBadge.textContent();
    return statusText?.toLowerCase().trim() ?? 'unknown';
  }

  /**
   * Get health component statuses from the details panel.
   */
  async getHealthComponents(): Promise<Array<{ name: string; status: string }>> {
    await this.expandHealthDetails();
    const rows = await this.healthComponentRows.all();
    const components: Array<{ name: string; status: string }> = [];

    for (const row of rows) {
      const name = await row.locator('.font-medium').first().textContent();
      const status = await row.locator('.capitalize').last().textContent();
      if (name && status) {
        components.push({
          name: name.trim(),
          status: status.trim().toLowerCase(),
        });
      }
    }

    return components;
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // Assertions
  // ─────────────────────────────────────────────────────────────────────────────

  /**
   * Assert that the login page is displayed.
   */
  async expectLoginPageVisible(): Promise<void> {
    await expect(this.pageTitle).toBeVisible();
    await expect(this.mainContainer).toBeVisible();
  }

  /**
   * Assert that the login form is visible and ready.
   */
  async expectLoginFormVisible(): Promise<void> {
    await expect(this.emailInput).toBeVisible();
    await expect(this.passwordInput).toBeVisible();
    await expect(this.submitButton).toBeVisible();
  }

  /**
   * Assert that dev bypass section is visible.
   */
  async expectDevBypassVisible(): Promise<void> {
    await expect(this.devBypassSection).toBeVisible();
    await expect(this.devBypassButton).toBeVisible();
  }

  /**
   * Assert that dev bypass section is not visible.
   */
  async expectDevBypassHidden(): Promise<void> {
    await expect(this.devBypassSection).toBeHidden();
  }

  /**
   * Assert that system health panel shows specific status.
   */
  async expectSystemHealthStatus(status: SystemHealthStatus): Promise<void> {
    const currentStatus = await this.getSystemHealthStatus();
    expect(currentStatus).toBe(status);
  }

  /**
   * Assert that a login error is displayed.
   */
  async expectLoginError(errorText?: string | RegExp): Promise<void> {
    await expect(this.errorAlert).toBeVisible();
    if (errorText) {
      await expect(this.errorAlert).toContainText(errorText);
    }
  }

  /**
   * Assert that account is locked out.
   */
  async expectLockout(): Promise<void> {
    await expect(this.lockoutAlert).toBeVisible();
  }

  /**
   * Assert that submit button is disabled.
   */
  async expectSubmitDisabled(): Promise<void> {
    await expect(this.submitButton).toBeDisabled();
  }

  /**
   * Assert that submit button is enabled.
   */
  async expectSubmitEnabled(): Promise<void> {
    await expect(this.submitButton).toBeEnabled();
  }

  /**
   * Assert field validation error is shown.
   */
  async expectFieldError(
    field: 'email' | 'password' | 'totp',
    errorText?: string | RegExp
  ): Promise<void> {
    const errorLocator =
      field === 'email'
        ? this.emailError
        : field === 'password'
          ? this.passwordError
          : this.totpError;

    await expect(errorLocator).toBeVisible();
    if (errorText) {
      await expect(errorLocator).toContainText(errorText);
    }
  }

  /**
   * Assert that system is starting (waiting for services).
   */
  async expectSystemStarting(): Promise<void> {
    await expect(this.systemStartingMessage).toBeVisible();
  }

  /**
   * Assert that control plane is unavailable.
   */
  async expectControlPlaneUnavailable(): Promise<void> {
    await expect(this.controlPlaneUnavailable).toBeVisible();
  }

  /**
   * Assert that TOTP field is visible.
   */
  async expectTotpFieldVisible(): Promise<void> {
    await expect(this.totpInput).toBeVisible();
  }

  /**
   * Assert that TOTP field is hidden.
   */
  async expectTotpFieldHidden(): Promise<void> {
    await expect(this.totpInput).toBeHidden();
  }
}

export default LoginPage;
