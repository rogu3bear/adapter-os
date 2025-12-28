/**
 * Chat Page Object
 *
 * Page object for the AdapterOS chat/workbench page.
 * Three-column layout with sessions, chat interface, and trace/evidence panels.
 *
 * Features:
 * - Left rail: Sessions, Datasets, Stacks tabs
 * - Center: ChatInterface with message input and streaming display
 * - Right rail: Evidence/Trace panel (collapsible)
 * - Workspace/Stack selection
 * - Developer/Kernel mode toggles
 */

import { type Page, type Locator, expect } from '@playwright/test';
import { BasePage, type WaitOptions } from './base-page';

export type StreamMode = 'tokens' | 'chunks';
export type LeftRailTab = 'sessions' | 'datasets' | 'stacks';

export interface ChatMessage {
  role: 'user' | 'assistant';
  content: string;
  traceId?: string;
}

export class ChatPage extends BasePage {
  // ─────────────────────────────────────────────────────────────────────────────
  // Locators - Top Bar
  // ─────────────────────────────────────────────────────────────────────────────

  /** Stream mode toggle switch */
  get streamModeSwitch(): Locator {
    return this.page.locator('#stream-mode');
  }

  /** Stream mode label */
  get streamModeLabel(): Locator {
    return this.page.locator('label[for="stream-mode"]');
  }

  /** Developer mode toggle switch */
  get developerModeSwitch(): Locator {
    return this.page.locator('#developer-mode');
  }

  /** Mode indicator (User Mode / OS Mode / Kernel Mode) */
  get modeIndicator(): Locator {
    return this.page.locator('.flex.items-center.gap-3.rounded-md.border');
  }

  /** Current mode name display */
  get currentModeName(): Locator {
    return this.modeIndicator.locator('.font-semibold');
  }

  /** Stack name display in top bar */
  get activeStackName(): Locator {
    return this.page.locator('[data-testid="active-stack-name"]');
  }

  /** Latency display */
  get latencyDisplay(): Locator {
    return this.page.locator('[data-testid="latency-ms"]');
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // Locators - Left Rail
  // ─────────────────────────────────────────────────────────────────────────────

  /** Left rail container */
  get leftRail(): Locator {
    return this.page.locator('[data-testid="left-rail"]');
  }

  /** Sessions tab trigger */
  get sessionsTab(): Locator {
    return this.leftRail.getByRole('tab', { name: /sessions/i });
  }

  /** Datasets tab trigger */
  get datasetsTab(): Locator {
    return this.leftRail.getByRole('tab', { name: /datasets/i });
  }

  /** Stacks tab trigger */
  get stacksTab(): Locator {
    return this.leftRail.getByRole('tab', { name: /stacks/i });
  }

  /** Sessions list */
  get sessionsList(): Locator {
    return this.leftRail.locator('[data-testid="sessions-list"]');
  }

  /** Session items */
  get sessionItems(): Locator {
    return this.sessionsList.locator('[data-testid="session-item"]');
  }

  /** Create new session button */
  get newSessionButton(): Locator {
    return this.leftRail.getByRole('button', { name: /new|create/i });
  }

  /** Stacks list */
  get stacksList(): Locator {
    return this.leftRail.locator('[data-testid="stacks-list"]');
  }

  /** Stack items */
  get stackItems(): Locator {
    return this.stacksList.locator('[data-testid="stack-item"]');
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // Locators - Chat Interface (Center)
  // ─────────────────────────────────────────────────────────────────────────────

  /** Chat container */
  get chatContainer(): Locator {
    return this.page.locator('[data-testid="chat-interface"]');
  }

  /** Message input textarea */
  get messageInput(): Locator {
    return this.page.locator('textarea').first();
  }

  /** Send message button */
  get sendButton(): Locator {
    return this.page.getByRole('button').filter({ has: this.page.locator('svg.lucide-send') });
  }

  /** Messages container */
  get messagesContainer(): Locator {
    return this.page.locator('[data-testid="messages-container"]');
  }

  /** All chat messages */
  get chatMessages(): Locator {
    return this.messagesContainer.locator('[data-testid="chat-message"]');
  }

  /** User messages */
  get userMessages(): Locator {
    return this.chatMessages.filter({ has: this.page.locator('[data-role="user"]') });
  }

  /** Assistant messages */
  get assistantMessages(): Locator {
    return this.chatMessages.filter({ has: this.page.locator('[data-role="assistant"]') });
  }

  /** Streaming message indicator */
  get streamingIndicator(): Locator {
    return this.page.locator('[data-testid="streaming-indicator"]');
  }

  /** Model loading block (inline) */
  get modelLoadingBlock(): Locator {
    return this.page.locator('[data-testid="inline-model-loading"]');
  }

  /** Loading overlay */
  get loadingOverlay(): Locator {
    return this.page.locator('[data-testid="chat-loading-overlay"]');
  }

  /** Empty state / no messages indicator */
  get emptyState(): Locator {
    return this.page.locator('[data-testid="chat-empty-state"]');
  }

  /** Stack selector in chat */
  get stackSelector(): Locator {
    return this.page.locator('[data-testid="stack-selector"]');
  }

  /** No model loaded warning */
  get noModelWarning(): Locator {
    return this.page.locator('[data-testid="no-model-warning"]');
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // Locators - Right Rail (Evidence/Trace)
  // ─────────────────────────────────────────────────────────────────────────────

  /** Right rail container */
  get rightRail(): Locator {
    return this.page.locator('[data-testid="right-rail"]');
  }

  /** Right rail toggle button (for collapsed state) */
  get rightRailToggle(): Locator {
    return this.page.locator('[data-testid="right-rail-toggle"]');
  }

  /** Evidence panel */
  get evidencePanel(): Locator {
    return this.rightRail.locator('[data-testid="evidence-panel"]');
  }

  /** Trace summary panel */
  get traceSummaryPanel(): Locator {
    return this.rightRail.locator('[data-testid="trace-summary-panel"]');
  }

  /** Trace loading indicator */
  get traceLoading(): Locator {
    return this.rightRail.locator('.animate-spin');
  }

  /** No trace message */
  get noTraceMessage(): Locator {
    return this.rightRail.getByText(/send a message to see trace/i);
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // Locators - Adapter Controls
  // ─────────────────────────────────────────────────────────────────────────────

  /** Adapter attachment chips */
  get adapterChips(): Locator {
    return this.page.locator('[data-testid="adapter-chip"]');
  }

  /** Adapter suggestion panel */
  get adapterSuggestion(): Locator {
    return this.page.locator('[data-testid="adapter-suggestion"]');
  }

  /** Auto-attach toggle */
  get autoAttachToggle(): Locator {
    return this.page.locator('[data-testid="auto-attach-toggle"]');
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // Page URL
  // ─────────────────────────────────────────────────────────────────────────────

  get url(): string {
    return '/chat';
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // Actions - Navigation & Setup
  // ─────────────────────────────────────────────────────────────────────────────

  /**
   * Wait for the chat page to be ready.
   */
  async waitForReady(options?: WaitOptions): Promise<void> {
    await super.waitForReady(options);
    // Wait for message input to be present (may be disabled if no model)
    await this.page.waitForSelector('textarea', {
      timeout: options?.timeout ?? this.defaultTimeout,
    });
  }

  /**
   * Wait for the chat to be ready for input (model loaded).
   */
  async waitForChatReady(options?: WaitOptions): Promise<void> {
    await this.waitForReady(options);
    await expect(this.messageInput).toBeEnabled({ timeout: options?.timeout });
  }

  /**
   * Select a tab in the left rail.
   */
  async selectLeftRailTab(tab: LeftRailTab): Promise<void> {
    const tabLocator =
      tab === 'sessions'
        ? this.sessionsTab
        : tab === 'datasets'
          ? this.datasetsTab
          : this.stacksTab;

    await tabLocator.click();
  }

  /**
   * Toggle the right rail open/closed.
   */
  async toggleRightRail(): Promise<void> {
    await this.rightRailToggle.click();
  }

  /**
   * Expand the right rail if collapsed.
   */
  async expandRightRail(): Promise<void> {
    const isVisible = await this.rightRail.isVisible();
    if (!isVisible) {
      await this.toggleRightRail();
      await this.waitForVisible(this.rightRail);
    }
  }

  /**
   * Collapse the right rail if expanded.
   */
  async collapseRightRail(): Promise<void> {
    const isVisible = await this.rightRail.isVisible();
    if (isVisible) {
      await this.toggleRightRail();
      await this.waitForHidden(this.rightRail);
    }
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // Actions - Mode Toggles
  // ─────────────────────────────────────────────────────────────────────────────

  /**
   * Set the stream mode.
   */
  async setStreamMode(mode: StreamMode): Promise<void> {
    const isTokenMode = await this.streamModeSwitch.isChecked();
    const wantTokenMode = mode === 'tokens';

    if (isTokenMode !== wantTokenMode) {
      await this.streamModeSwitch.click();
    }
  }

  /**
   * Toggle developer mode on/off.
   */
  async toggleDeveloperMode(): Promise<void> {
    await this.developerModeSwitch.click();
  }

  /**
   * Enable developer mode.
   */
  async enableDeveloperMode(): Promise<void> {
    const isEnabled = await this.developerModeSwitch.isChecked();
    if (!isEnabled) {
      await this.developerModeSwitch.click();
    }
  }

  /**
   * Disable developer mode.
   */
  async disableDeveloperMode(): Promise<void> {
    const isEnabled = await this.developerModeSwitch.isChecked();
    if (isEnabled) {
      await this.developerModeSwitch.click();
    }
  }

  /**
   * Get the current mode name (User Mode / OS Mode / Kernel Mode).
   */
  async getCurrentMode(): Promise<string> {
    const text = await this.currentModeName.textContent();
    return text?.trim() ?? '';
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // Actions - Chat
  // ─────────────────────────────────────────────────────────────────────────────

  /**
   * Type a message in the input field.
   */
  async typeMessage(message: string): Promise<void> {
    await this.fillField(this.messageInput, message);
  }

  /**
   * Clear the message input.
   */
  async clearMessage(): Promise<void> {
    await this.clearField(this.messageInput);
  }

  /**
   * Send a message.
   */
  async sendMessage(message: string): Promise<void> {
    await this.typeMessage(message);
    await this.sendButton.click();
  }

  /**
   * Send a message and wait for the response to complete.
   */
  async sendMessageAndWaitForResponse(
    message: string,
    options?: WaitOptions
  ): Promise<void> {
    const initialCount = await this.assistantMessages.count();
    await this.sendMessage(message);

    // Wait for streaming to start and complete
    await this.waitForHidden(this.streamingIndicator, {
      timeout: options?.timeout ?? 60_000,
    });

    // Verify we got a new assistant message
    await expect(this.assistantMessages).toHaveCount(initialCount + 1, {
      timeout: options?.timeout ?? 30_000,
    });
  }

  /**
   * Get the content of the last assistant message.
   */
  async getLastAssistantMessage(): Promise<string> {
    const messages = await this.assistantMessages.all();
    if (messages.length === 0) {
      return '';
    }
    const lastMessage = messages[messages.length - 1];
    return (await lastMessage.textContent()) ?? '';
  }

  /**
   * Get all messages as structured data.
   */
  async getAllMessages(): Promise<ChatMessage[]> {
    const messages: ChatMessage[] = [];
    const messageElements = await this.chatMessages.all();

    for (const element of messageElements) {
      const role = (await element.getAttribute('data-role')) as 'user' | 'assistant';
      const content = (await element.textContent()) ?? '';
      const traceId = (await element.getAttribute('data-trace-id')) ?? undefined;

      messages.push({ role, content, traceId });
    }

    return messages;
  }

  /**
   * Select a message to view its trace.
   */
  async selectMessage(index: number): Promise<void> {
    const messages = await this.chatMessages.all();
    if (index >= 0 && index < messages.length) {
      await messages[index].click();
    }
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // Actions - Sessions
  // ─────────────────────────────────────────────────────────────────────────────

  /**
   * Create a new chat session.
   */
  async createNewSession(): Promise<void> {
    await this.selectLeftRailTab('sessions');
    await this.newSessionButton.click();
  }

  /**
   * Select a session by name.
   */
  async selectSession(name: string | RegExp): Promise<void> {
    await this.selectLeftRailTab('sessions');
    const session = this.sessionItems.filter({ hasText: name });
    await session.click();
  }

  /**
   * Get session names.
   */
  async getSessionNames(): Promise<string[]> {
    await this.selectLeftRailTab('sessions');
    const items = await this.sessionItems.all();
    const names: string[] = [];

    for (const item of items) {
      const text = await item.textContent();
      if (text) {
        names.push(text.trim());
      }
    }

    return names;
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // Actions - Stacks
  // ─────────────────────────────────────────────────────────────────────────────

  /**
   * Select a stack by name.
   */
  async selectStack(name: string | RegExp): Promise<void> {
    await this.selectLeftRailTab('stacks');
    const stack = this.stackItems.filter({ hasText: name });
    await stack.click();
  }

  /**
   * Get stack names.
   */
  async getStackNames(): Promise<string[]> {
    await this.selectLeftRailTab('stacks');
    const items = await this.stackItems.all();
    const names: string[] = [];

    for (const item of items) {
      const text = await item.textContent();
      if (text) {
        names.push(text.trim());
      }
    }

    return names;
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // Assertions
  // ─────────────────────────────────────────────────────────────────────────────

  /**
   * Assert that the chat page is displayed.
   */
  async expectChatPageVisible(): Promise<void> {
    await expect(this.messageInput).toBeVisible();
  }

  /**
   * Assert that the message input is enabled (model is loaded).
   */
  async expectInputEnabled(): Promise<void> {
    await expect(this.messageInput).toBeEnabled();
  }

  /**
   * Assert that the message input is disabled (no model loaded).
   */
  async expectInputDisabled(): Promise<void> {
    await expect(this.messageInput).toBeDisabled();
  }

  /**
   * Assert that a model loading message is shown.
   */
  async expectModelLoading(): Promise<void> {
    await expect(this.modelLoadingBlock).toBeVisible();
  }

  /**
   * Assert that no model is loaded warning is shown.
   */
  async expectNoModelWarning(): Promise<void> {
    await expect(this.noModelWarning).toBeVisible();
  }

  /**
   * Assert that a specific number of messages exist.
   */
  async expectMessageCount(count: number): Promise<void> {
    await expect(this.chatMessages).toHaveCount(count);
  }

  /**
   * Assert that the chat is empty.
   */
  async expectEmptyChat(): Promise<void> {
    await expect(this.chatMessages).toHaveCount(0);
  }

  /**
   * Assert that streaming is in progress.
   */
  async expectStreaming(): Promise<void> {
    await expect(this.streamingIndicator).toBeVisible();
  }

  /**
   * Assert that streaming is complete.
   */
  async expectStreamingComplete(): Promise<void> {
    await expect(this.streamingIndicator).toBeHidden();
  }

  /**
   * Assert the current stream mode.
   */
  async expectStreamMode(mode: StreamMode): Promise<void> {
    const label = await this.streamModeLabel.textContent();
    expect(label?.toLowerCase()).toContain(mode);
  }

  /**
   * Assert the current UI mode.
   */
  async expectMode(mode: 'User Mode' | 'OS Mode' | 'Kernel Mode'): Promise<void> {
    const current = await this.getCurrentMode();
    expect(current).toBe(mode);
  }

  /**
   * Assert that the right rail is visible.
   */
  async expectRightRailVisible(): Promise<void> {
    await expect(this.rightRail).toBeVisible();
  }

  /**
   * Assert that the right rail is hidden.
   */
  async expectRightRailHidden(): Promise<void> {
    await expect(this.rightRail).toBeHidden();
  }

  /**
   * Assert that evidence panel is visible.
   */
  async expectEvidencePanelVisible(): Promise<void> {
    await expect(this.evidencePanel).toBeVisible();
  }

  /**
   * Assert that trace is loading.
   */
  async expectTraceLoading(): Promise<void> {
    await expect(this.traceLoading).toBeVisible();
  }

  /**
   * Assert that trace summary is visible.
   */
  async expectTraceSummaryVisible(): Promise<void> {
    await expect(this.traceSummaryPanel).toBeVisible();
  }

  /**
   * Assert the last message contains specific text.
   */
  async expectLastMessageContains(text: string | RegExp): Promise<void> {
    const lastMessage = await this.getLastAssistantMessage();
    if (typeof text === 'string') {
      expect(lastMessage).toContain(text);
    } else {
      expect(lastMessage).toMatch(text);
    }
  }

  /**
   * Assert a stack is selected.
   */
  async expectStackSelected(name: string | RegExp): Promise<void> {
    await expect(this.activeStackName).toContainText(name);
  }

  /**
   * Assert adapter chips are visible.
   */
  async expectAdapterChipsVisible(): Promise<void> {
    await expect(this.adapterChips.first()).toBeVisible();
  }
}

export default ChatPage;
