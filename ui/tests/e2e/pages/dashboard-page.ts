/**
 * Dashboard Page Object
 *
 * Page object for the AdapterOS dashboard/home page.
 * Displays workspace status and getting started steps for the MVP flow.
 *
 * Features:
 * - Workspace status card with selection
 * - Getting started checklist (Select Workspace, Load Base Model, Upload Data, Start Tune, Chat)
 * - Progress indicator
 * - Quick navigation links
 */

import { type Page, type Locator, expect } from '@playwright/test';
import { BasePage, type WaitOptions } from './base-page';

export type StepStatus = 'done' | 'active' | 'pending';

export interface Step {
  id: string;
  title: string;
  status: StepStatus;
}

export class DashboardPage extends BasePage {
  // ─────────────────────────────────────────────────────────────────────────────
  // Locators - Workspace Status Card
  // ─────────────────────────────────────────────────────────────────────────────

  /** Workspace status card */
  get workspaceStatusCard(): Locator {
    return this.getCard(/workspace status/i);
  }

  /** Active workspace badge */
  get activeWorkspaceBadge(): Locator {
    return this.workspaceStatusCard.locator('[class*="badge"]').first();
  }

  /** Active workspace name display */
  get activeWorkspaceName(): Locator {
    return this.workspaceStatusCard.locator('.text-muted-foreground').last();
  }

  /** Progress bar */
  get progressBar(): Locator {
    return this.workspaceStatusCard.locator('[role="progressbar"], [aria-label="MVP flow progress"]');
  }

  /** Progress percentage text */
  get progressPercentage(): Locator {
    return this.workspaceStatusCard.locator('.text-right.text-sm');
  }

  /** Switch/Select workspace button */
  get switchWorkspaceButton(): Locator {
    return this.workspaceStatusCard.getByRole('link', { name: /switch workspace|select workspace/i });
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // Locators - Getting Started Card
  // ─────────────────────────────────────────────────────────────────────────────

  /** Getting started card */
  get gettingStartedCard(): Locator {
    return this.getCard(/get started/i);
  }

  /** All step items */
  get stepItems(): Locator {
    return this.gettingStartedCard.locator('.rounded-lg.border.p-3');
  }

  /** Step: Select Workspace */
  get selectWorkspaceStep(): Locator {
    return this.getStepByTitle('Select Workspace');
  }

  /** Step: Load Base Model */
  get loadBaseModelStep(): Locator {
    return this.getStepByTitle('Load Base Model');
  }

  /** Step: Upload Data */
  get uploadDataStep(): Locator {
    return this.getStepByTitle('Upload Data');
  }

  /** Step: Start Tune */
  get startTuneStep(): Locator {
    return this.getStepByTitle('Start Tune');
  }

  /** Step: Chat */
  get chatStep(): Locator {
    return this.getStepByTitle('Chat');
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // Locators - Navigation Links
  // ─────────────────────────────────────────────────────────────────────────────

  /** Link to workspaces page */
  get workspacesLink(): Locator {
    return this.getLink(/workspaces/i);
  }

  /** Link to base models page */
  get baseModelsLink(): Locator {
    return this.selectWorkspaceStep.getByRole('link', { name: /open/i });
  }

  /** Link to documents/data upload page */
  get documentsLink(): Locator {
    return this.uploadDataStep.getByRole('link', { name: /open/i });
  }

  /** Link to training page */
  get trainingLink(): Locator {
    return this.startTuneStep.getByRole('link', { name: /open/i });
  }

  /** Link to chat page */
  get chatLink(): Locator {
    return this.chatStep.getByRole('link', { name: /open/i });
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // Page URL
  // ─────────────────────────────────────────────────────────────────────────────

  get url(): string {
    return '/dashboard';
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // Private Helpers
  // ─────────────────────────────────────────────────────────────────────────────

  /**
   * Get a step item by its title.
   */
  private getStepByTitle(title: string): Locator {
    return this.stepItems.filter({ hasText: title });
  }

  /**
   * Get the status badge within a step.
   */
  private getStepStatusBadge(step: Locator): Locator {
    return step.locator('[class*="badge"]');
  }

  /**
   * Get the open button within a step.
   */
  private getStepOpenButton(step: Locator): Locator {
    return step.getByRole('link', { name: /open/i });
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // Actions
  // ─────────────────────────────────────────────────────────────────────────────

  /**
   * Wait for the dashboard page to be ready.
   */
  async waitForReady(options?: WaitOptions): Promise<void> {
    await super.waitForReady(options);
    await this.waitForVisible(this.workspaceStatusCard, options);
    await this.waitForVisible(this.gettingStartedCard, options);
  }

  /**
   * Navigate to switch workspace.
   */
  async goToSwitchWorkspace(): Promise<void> {
    await this.clickAndWaitForNavigation(this.switchWorkspaceButton);
  }

  /**
   * Open a specific step by clicking its Open button.
   */
  async openStep(stepTitle: string): Promise<void> {
    const step = this.getStepByTitle(stepTitle);
    const openButton = this.getStepOpenButton(step);
    await this.clickAndWaitForNavigation(openButton);
  }

  /**
   * Navigate to workspaces page via step.
   */
  async goToWorkspaces(): Promise<void> {
    await this.openStep('Select Workspace');
  }

  /**
   * Navigate to base models page via step.
   */
  async goToBaseModels(): Promise<void> {
    await this.openStep('Load Base Model');
  }

  /**
   * Navigate to documents/upload page via step.
   */
  async goToDocuments(): Promise<void> {
    await this.openStep('Upload Data');
  }

  /**
   * Navigate to training page via step.
   */
  async goToTraining(): Promise<void> {
    await this.openStep('Start Tune');
  }

  /**
   * Navigate to chat page via step.
   */
  async goToChat(): Promise<void> {
    await this.openStep('Chat');
  }

  /**
   * Get the current progress percentage.
   */
  async getProgressPercentage(): Promise<number> {
    const text = await this.progressPercentage.textContent();
    const match = text?.match(/(\d+)%/);
    return match ? parseInt(match[1], 10) : 0;
  }

  /**
   * Get the status of a specific step.
   */
  async getStepStatus(stepTitle: string): Promise<StepStatus> {
    const step = this.getStepByTitle(stepTitle);
    const badge = this.getStepStatusBadge(step);
    const text = await badge.textContent();
    const normalized = text?.toLowerCase().trim();

    if (normalized === 'done') return 'done';
    if (normalized === 'next') return 'active';
    return 'pending';
  }

  /**
   * Get all steps with their statuses.
   */
  async getAllSteps(): Promise<Step[]> {
    const steps: Step[] = [];
    const stepTitles = [
      'Select Workspace',
      'Load Base Model',
      'Upload Data',
      'Start Tune',
      'Chat',
    ];

    for (const title of stepTitles) {
      const status = await this.getStepStatus(title);
      steps.push({
        id: title.toLowerCase().replace(/\s+/g, '-'),
        title,
        status,
      });
    }

    return steps;
  }

  /**
   * Get the active workspace name.
   */
  async getActiveWorkspace(): Promise<string | null> {
    const text = await this.activeWorkspaceName.textContent();
    if (!text || text.includes('No workspace selected')) {
      return null;
    }
    return text.trim();
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // Assertions
  // ─────────────────────────────────────────────────────────────────────────────

  /**
   * Assert that the dashboard is displayed.
   */
  async expectDashboardVisible(): Promise<void> {
    await expect(this.workspaceStatusCard).toBeVisible();
    await expect(this.gettingStartedCard).toBeVisible();
  }

  /**
   * Assert that a workspace is selected.
   */
  async expectWorkspaceSelected(name?: string): Promise<void> {
    await expect(this.activeWorkspaceBadge).toContainText(/selected/i);
    if (name) {
      await expect(this.activeWorkspaceName).toContainText(name);
    }
  }

  /**
   * Assert that no workspace is selected.
   */
  async expectNoWorkspaceSelected(): Promise<void> {
    await expect(this.activeWorkspaceBadge).toContainText(/not selected/i);
    await expect(this.activeWorkspaceName).toContainText(/no workspace selected/i);
  }

  /**
   * Assert the progress percentage.
   */
  async expectProgress(percentage: number): Promise<void> {
    const actual = await this.getProgressPercentage();
    expect(actual).toBe(percentage);
  }

  /**
   * Assert a step has a specific status.
   */
  async expectStepStatus(stepTitle: string, status: StepStatus): Promise<void> {
    const actual = await this.getStepStatus(stepTitle);
    expect(actual).toBe(status);
  }

  /**
   * Assert the current active step (marked as "Next").
   */
  async expectActiveStep(stepTitle: string): Promise<void> {
    await this.expectStepStatus(stepTitle, 'active');
  }

  /**
   * Assert all getting started steps are visible.
   */
  async expectAllStepsVisible(): Promise<void> {
    await expect(this.selectWorkspaceStep).toBeVisible();
    await expect(this.loadBaseModelStep).toBeVisible();
    await expect(this.uploadDataStep).toBeVisible();
    await expect(this.startTuneStep).toBeVisible();
    await expect(this.chatStep).toBeVisible();
  }

  /**
   * Assert that the step count is correct.
   */
  async expectStepCount(count: number): Promise<void> {
    await expect(this.stepItems).toHaveCount(count);
  }
}

export default DashboardPage;
