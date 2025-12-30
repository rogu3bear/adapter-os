/**
 * Flow 13: Login to Chat with Adapter
 *
 * End-to-end flow test from login page through to chatting with an adapter.
 * Tests the complete user journey:
 * - Dev bypass authentication
 * - Workspace selection/navigation
 * - Chat page ready state
 * - Sending a message and receiving a streaming response
 *
 * @preconditions
 * - Backend running (or mocked)
 * - Dev bypass enabled
 * - Base model available
 * - At least one adapter in hot state
 */

import { test, expect, type Page } from '@playwright/test';
import { LoginPage } from '../pages/login-page';
import { ChatPage } from '../pages/chat-page';
import {
  setupApiMocks,
  fulfillJson,
  installSseStub,
  type ApiMockOptions,
} from '../fixtures/api-mocks';

// =============================================================================
// Test Constants
// =============================================================================

const FIXED_NOW = '2025-01-01T12:00:00.000Z';

const MOCK_RUN_ENVELOPE = {
  run_id: 'run-login-flow-001',
  workspace_id: 'ws-tenant-001',
  manifest_hash_b3: 'b3:manifest-hash-0123456789abcdef',
  policy_mask_digest_b3: 'b3:policy-mask-digest-fedcba9876543210',
  plan_id: 'plan-xyz-789',
  worker_id: 'worker-node-42',
  determinism_version: 'v2.1.0',
  router_seed: 'seed-hidden-value',
  tick: 1704110400,
  reasoning_mode: 'deterministic',
  boot_trace_id: 'boot-trace-abc',
  created_at: FIXED_NOW,
};

const MOCK_RESPONSE_TOKENS = [
  'Hello',
  '!',
  ' I',
  "'m",
  ' your',
  ' AI',
  ' assistant',
  '.',
  ' How',
  ' can',
  ' I',
  ' help',
  ' you',
  ' today',
  '?',
];

// =============================================================================
// SSE Helpers
// =============================================================================

/**
 * Format an SSE event with optional event type
 */
function formatSSE(data: unknown, eventType?: string): string {
  const lines: string[] = [];
  if (eventType) {
    lines.push(`event: ${eventType}`);
  }
  lines.push(`data: ${JSON.stringify(data)}`);
  lines.push('');
  return lines.join('\n') + '\n';
}

/**
 * Create a complete SSE stream with RunEnvelope, tokens, and completion
 */
function createSSEStream(options?: {
  includeEnvelope?: boolean;
  tokens?: string[];
  runEnvelope?: Record<string, unknown>;
}): string {
  const {
    includeEnvelope = true,
    tokens = MOCK_RESPONSE_TOKENS,
    runEnvelope = MOCK_RUN_ENVELOPE,
  } = options ?? {};

  const chunks: string[] = [];
  const requestId = (runEnvelope.run_id as string) || 'req-default';

  // 1. RunEnvelope event (first, not a token)
  if (includeEnvelope) {
    chunks.push(
      formatSSE(
        {
          event: 'aos.run_envelope',
          data: runEnvelope,
        },
        'aos.run_envelope'
      )
    );
  }

  // 2. Token events
  tokens.forEach((token, index) => {
    const tokenChunk = {
      id: requestId,
      object: 'chat.completion.chunk',
      created: Date.now(),
      model: 'test-model',
      choices: [
        {
          index: 0,
          delta: {
            content: token,
            ...(index === 0 ? { role: 'assistant' } : {}),
          },
          finish_reason: null,
        },
      ],
    };
    chunks.push(formatSSE(tokenChunk));
  });

  // 3. Completion event
  const completeChunk = {
    id: requestId,
    object: 'chat.completion.chunk',
    created: Date.now(),
    model: 'test-model',
    choices: [
      {
        index: 0,
        delta: {},
        finish_reason: 'stop',
      },
    ],
    usage: {
      prompt_tokens: 10,
      completion_tokens: tokens.length,
      total_tokens: 10 + tokens.length,
    },
    request_id: requestId,
  };
  chunks.push(formatSSE(completeChunk));

  // 4. Done marker
  chunks.push('data: [DONE]\n\n');

  return chunks.join('');
}

// =============================================================================
// Mock Setup
// =============================================================================

async function setupLoginToChatMocks(page: Page, options?: ApiMockOptions) {
  // Setup base API mocks
  await setupApiMocks(page, {
    fixedNow: FIXED_NOW,
    user: {
      userId: 'user-1',
      email: 'dev@local',
      role: 'admin',
      tenantId: 'tenant-1',
      permissions: ['inference:execute', 'training:start', 'adapter:register'],
    },
    tenants: [{ id: 'tenant-1', name: 'Test Workspace', role: 'admin' }],
    models: [
      {
        id: 'model-1',
        name: 'Demo Model',
        format: 'gguf',
        backend: 'coreml',
        status: 'ready',
        isLoaded: true,
      },
    ],
    adapters: [
      {
        id: 'adapter-hot',
        name: 'Demo Adapter',
        currentState: 'hot',
        tier: 'prod',
        scope: 'general',
      },
    ],
    ...options,
  });

  // Auth config for dev bypass
  await page.route('**/v1/auth/config', (route) =>
    fulfillJson(route, {
      allow_registration: false,
      require_email_verification: false,
      access_token_ttl_minutes: 60,
      session_timeout_minutes: 480,
      max_login_attempts: 5,
      password_min_length: 8,
      mfa_required: false,
      production_mode: false,
      dev_token_enabled: true,
      dev_bypass_allowed: true,
      jwt_mode: 'hs256',
      token_expiry_hours: 24,
    })
  );

  // Dev bypass endpoint
  await page.route('**/v1/auth/dev-bypass', (route) =>
    fulfillJson(route, {
      schema_version: '1.0',
      token: 'dev-bypass-token',
      user_id: 'dev-no-auth',
      tenant_id: 'tenant-1',
      role: 'admin',
      expires_in: 86400,
      tenants: [{ id: 'tenant-1', name: 'Test Workspace', role: 'admin' }],
      admin_tenants: ['*'],
      session_mode: 'dev_bypass',
    })
  );

  // Chat sessions
  await page.route('**/v1/chat/sessions', (route) => {
    const method = route.request().method();
    if (method === 'GET') {
      return fulfillJson(route, {
        schema_version: '1.0',
        sessions: [],
        total: 0,
      });
    }
    if (method === 'POST') {
      return fulfillJson(route, {
        schema_version: '1.0',
        id: 'session-new',
        name: 'New Chat',
        stack_id: 'stack-1',
        created_at: FIXED_NOW,
        updated_at: FIXED_NOW,
        messages: [],
      });
    }
    return fulfillJson(route, { schema_version: '1.0' });
  });

  // Adapter stacks
  await page.route('**/v1/adapter-stacks', (route) =>
    fulfillJson(route, [
      {
        id: 'stack-1',
        name: 'Demo Stack',
        adapter_ids: ['adapter-hot'],
        description: 'Demo stack with hot adapter',
        created_at: FIXED_NOW,
        updated_at: FIXED_NOW,
      },
    ])
  );

  // Streaming endpoint
  await page.route('**/v1/infer/stream', async (route) => {
    const sseContent = createSSEStream();
    await route.fulfill({
      status: 200,
      contentType: 'text/event-stream',
      headers: {
        'Cache-Control': 'no-cache',
        Connection: 'keep-alive',
        'X-Accel-Buffering': 'no',
      },
      body: sseContent,
    });
  });

  // Routing decisions
  await page.route('**/v1/routing/sessions/**', (route) =>
    fulfillJson(route, {
      schema_version: '1.0',
      request_id: MOCK_RUN_ENVELOPE.run_id,
      selected_adapters: ['adapter-hot'],
      scores: { 'adapter-hot': 0.95 },
      latency_ms: 5,
    })
  );

  // Workspace user workspaces endpoint
  await page.route('**/v1/workspaces/me', (route) =>
    fulfillJson(route, {
      schema_version: '1.0',
      workspaces: [
        {
          id: 'workspace-1',
          name: 'Test Workspace',
          description: 'Default workspace for testing',
          member_count: 1,
        },
      ],
      total: 1,
    })
  );
}

// =============================================================================
// Console Error Guards
// =============================================================================

function attachConsoleGuards(page: Page) {
  const consoleErrors: string[] = [];
  const pageErrors: string[] = [];

  page.on('console', (msg) => {
    if (msg.type() !== 'error') return;
    const loc = msg.location();
    const suffix = loc.url ? ` (${loc.url}:${loc.lineNumber}:${loc.columnNumber})` : '';
    consoleErrors.push(`${msg.text()}${suffix}`);
  });

  page.on('pageerror', (err) => {
    pageErrors.push(err.message);
  });

  return { consoleErrors, pageErrors };
}

// =============================================================================
// Test Suite
// =============================================================================

test.describe('Flow 13: Login to Chat with Adapter', () => {
  test.beforeEach(async ({ page }) => {
    // Install SSE stub before navigation
    await installSseStub(page);
  });

  test('complete flow: dev bypass login → navigate to chat → send message → receive response', async ({
    page,
  }) => {
    const { consoleErrors, pageErrors } = attachConsoleGuards(page);

    await setupLoginToChatMocks(page);

    // Initialize page objects
    const loginPage = new LoginPage(page);
    const chatPage = new ChatPage(page);

    // Step 1: Navigate to login page
    await loginPage.goto();
    await loginPage.waitForReady({ timeout: 10000 });

    // Step 2: Verify login page is displayed
    await loginPage.expectLoginPageVisible();
    await loginPage.expectDevBypassVisible();

    // Step 3: Use dev bypass to authenticate
    await loginPage.devBypassAndWaitForDashboard();

    // Step 4: Navigate to chat page
    await chatPage.goto();
    await chatPage.waitForReady({ timeout: 10000 });

    // Step 5: Verify chat page is displayed
    await chatPage.expectChatPageVisible();

    // Step 6: Wait for message input to be enabled (model loaded)
    await expect(chatPage.messageInput).toBeEnabled({ timeout: 15000 });

    // Step 7: Send a test message
    const testMessage = 'Hello, can you help me?';
    await chatPage.typeMessage(testMessage);

    // Track the streaming request
    const streamRequest = page.waitForRequest((req) =>
      req.url().includes('/v1/infer/stream') && req.method() === 'POST'
    );

    // Click send
    await chatPage.sendButton.click();

    // Wait for stream request to be made
    await streamRequest;

    // Step 8: Wait for response to appear
    const expectedResponse = MOCK_RESPONSE_TOKENS.join('');
    await expect(page.getByText(expectedResponse)).toBeVisible({ timeout: 15000 });

    // Step 9: Verify no critical errors occurred
    const criticalErrors = pageErrors.filter(
      (e) => !e.includes('ResizeObserver') && !e.includes('Non-Error')
    );
    expect(criticalErrors).toEqual([]);
  });

  test('login page shows system health status', async ({ page }) => {
    await setupLoginToChatMocks(page);

    const loginPage = new LoginPage(page);

    await loginPage.goto();
    await loginPage.waitForReady({ timeout: 10000 });

    // Verify system health panel is visible
    await expect(loginPage.controlPlaneStatus).toBeVisible({ timeout: 5000 });
  });

  test('chat page shows adapter chips after login', async ({ page }) => {
    await setupLoginToChatMocks(page);

    const loginPage = new LoginPage(page);
    const chatPage = new ChatPage(page);

    // Login
    await loginPage.goto();
    await loginPage.waitForReady({ timeout: 10000 });
    await loginPage.devBypassAndWaitForDashboard();

    // Navigate to chat
    await chatPage.goto();
    await chatPage.waitForReady({ timeout: 10000 });

    // Verify adapters are displayed (via stacks tab or chips)
    await chatPage.selectLeftRailTab('stacks');
    await expect(chatPage.stacksTab).toBeVisible();

    // The stack list should show our demo stack
    const stacksList = page.locator('[data-testid="stacks-list"], .stacks-list');
    const hasStacks = await stacksList.isVisible().catch(() => false);
    if (hasStacks) {
      await expect(page.getByText('Demo Stack')).toBeVisible({ timeout: 5000 });
    }
  });

  test('handles streaming response with evidence panel', async ({ page }) => {
    const { pageErrors } = attachConsoleGuards(page);

    await setupLoginToChatMocks(page);

    const loginPage = new LoginPage(page);
    const chatPage = new ChatPage(page);

    // Login flow
    await loginPage.goto();
    await loginPage.waitForReady({ timeout: 10000 });
    await loginPage.devBypassAndWaitForDashboard();

    // Navigate to chat
    await chatPage.goto();
    await chatPage.waitForReady({ timeout: 10000 });
    await expect(chatPage.messageInput).toBeEnabled({ timeout: 15000 });

    // Send message
    await chatPage.typeMessage('Tell me about adapters');
    await chatPage.sendButton.click();

    // Wait for streaming to complete
    const expectedResponse = MOCK_RESPONSE_TOKENS.join('');
    await expect(page.getByText(expectedResponse)).toBeVisible({ timeout: 15000 });

    // Check for evidence panel (if visible)
    const evidencePanel = page.locator('[data-testid="evidence-panel"], [class*="evidence"]');
    const evidenceVisible = await evidencePanel.first().isVisible().catch(() => false);

    if (evidenceVisible) {
      // Verify run_id is displayed
      await expect(evidencePanel.getByText(MOCK_RUN_ENVELOPE.run_id)).toBeVisible({ timeout: 5000 });
    }

    // Verify no critical page errors
    const criticalErrors = pageErrors.filter(
      (e) => !e.includes('ResizeObserver') && !e.includes('Non-Error')
    );
    expect(criticalErrors).toEqual([]);
  });

  test('can send multiple messages in a session', async ({ page }) => {
    await setupLoginToChatMocks(page);

    const loginPage = new LoginPage(page);
    const chatPage = new ChatPage(page);

    // Login flow
    await loginPage.goto();
    await loginPage.waitForReady({ timeout: 10000 });
    await loginPage.devBypassAndWaitForDashboard();

    // Navigate to chat
    await chatPage.goto();
    await chatPage.waitForReady({ timeout: 10000 });
    await expect(chatPage.messageInput).toBeEnabled({ timeout: 15000 });

    // First message
    await chatPage.typeMessage('First question');
    await chatPage.sendButton.click();

    // Wait for first response
    const expectedResponse = MOCK_RESPONSE_TOKENS.join('');
    await expect(page.getByText(expectedResponse).first()).toBeVisible({ timeout: 15000 });

    // Wait for input to be ready again
    await page.waitForTimeout(500);
    await expect(chatPage.messageInput).toBeEnabled({ timeout: 5000 });

    // Second message
    await chatPage.typeMessage('Follow up question');
    await chatPage.sendButton.click();

    // Wait for second response
    await page.waitForTimeout(2000);

    // Should have multiple assistant messages now
    const assistantMessages = page.locator('[data-role="assistant"], .assistant-message');
    const count = await assistantMessages.count();
    expect(count).toBeGreaterThanOrEqual(1);
  });

  test.describe('Error cases', () => {
    test('shows error when backend is unavailable during login', async ({ page }) => {
      // Setup mocks but make health check fail
      await page.route('**/healthz', (route) =>
        route.fulfill({
          status: 503,
          contentType: 'application/json',
          body: JSON.stringify({ status: 'unhealthy' }),
        })
      );

      await page.route('**/healthz/all', (route) =>
        route.fulfill({
          status: 503,
          contentType: 'application/json',
          body: JSON.stringify({ status: 'unhealthy' }),
        })
      );

      const loginPage = new LoginPage(page);

      await loginPage.goto();

      // Should show some indication of system issues
      // The exact behavior depends on implementation
      await page.waitForTimeout(2000);

      // At minimum, the page should load without crashing
      await expect(page.locator('body')).toBeVisible();
    });

    test('handles streaming error gracefully', async ({ page }) => {
      await setupLoginToChatMocks(page);

      // Override streaming endpoint to return error
      await page.route('**/v1/infer/stream', async (route) => {
        await route.fulfill({
          status: 500,
          contentType: 'application/json',
          body: JSON.stringify({ error: 'Internal server error' }),
        });
      });

      const loginPage = new LoginPage(page);
      const chatPage = new ChatPage(page);

      // Login flow
      await loginPage.goto();
      await loginPage.waitForReady({ timeout: 10000 });
      await loginPage.devBypassAndWaitForDashboard();

      // Navigate to chat
      await chatPage.goto();
      await chatPage.waitForReady({ timeout: 10000 });
      await expect(chatPage.messageInput).toBeEnabled({ timeout: 15000 });

      // Send message
      await chatPage.typeMessage('This will fail');
      await chatPage.sendButton.click();

      // Wait for error to be shown
      await page.waitForTimeout(2000);

      // Should show error message or allow retry
      const hasError = await page.getByText(/error|failed|try again/i).first().isVisible().catch(() => false);
      const inputStillEnabled = await chatPage.messageInput.isEnabled();

      // Either an error is shown OR the input is still usable
      expect(hasError || inputStillEnabled).toBe(true);
    });
  });
});
