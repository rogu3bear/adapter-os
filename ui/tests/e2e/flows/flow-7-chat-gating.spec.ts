/**
 * Flow 7: Chat is gated until a model is active
 *
 * Tests that the chat interface properly gates user input when no base model
 * is loaded, displays appropriate messaging, and provides actionable UI to
 * resolve the issue.
 *
 * Preconditions:
 * - Workspace selected
 * - No active base model OR no loaded model
 *
 * Expected outcomes:
 * - Chat input is disabled
 * - UI displays reason: "Base model required" or similar
 * - UI provides direct action: "Load base model" button
 * - No background requests spam the API
 */

import { test, expect, type Page, type Route, type Request } from '@playwright/test';

const FIXED_NOW = '2025-01-01T00:00:00.000Z';

// =============================================================================
// Test Utilities
// =============================================================================

interface MockOptions {
  /** Whether the base model is ready (default: false for gating tests) */
  baseModelReady?: boolean;
  /** Model status: 'ready' | 'no-model' | 'loading' | 'error' */
  modelStatus?: 'ready' | 'no-model' | 'loading' | 'error';
  /** Model ID when loaded */
  modelId?: string;
  /** Model name when loaded */
  modelName?: string;
}

interface RequestTracker {
  /** All tracked requests */
  requests: { url: string; method: string; timestamp: number }[];
  /** Count requests matching a pattern */
  countMatching: (pattern: RegExp) => number;
  /** Get requests matching a pattern */
  getMatching: (pattern: RegExp) => { url: string; method: string; timestamp: number }[];
  /** Clear tracked requests */
  clear: () => void;
}

function createRequestTracker(page: Page): RequestTracker {
  const requests: { url: string; method: string; timestamp: number }[] = [];

  page.on('request', (request: Request) => {
    requests.push({
      url: request.url(),
      method: request.method(),
      timestamp: Date.now(),
    });
  });

  return {
    requests,
    countMatching: (pattern: RegExp) =>
      requests.filter((r) => pattern.test(r.url)).length,
    getMatching: (pattern: RegExp) =>
      requests.filter((r) => pattern.test(r.url)),
    clear: () => {
      requests.length = 0;
    },
  };
}

async function setupChatGatingMocks(page: Page, options: MockOptions = {}) {
  const {
    baseModelReady = false,
    modelStatus = 'no-model',
    modelId = baseModelReady ? 'model-1' : 'none',
    modelName = baseModelReady ? 'Demo Model' : null,
  } = options;

  const now = FIXED_NOW;

  const fulfillJson = (route: Route, body: unknown, status = 200) =>
    route.fulfill({
      status,
      contentType: 'application/json',
      body: JSON.stringify(body),
    });

  // Health endpoints
  await page.route('**/healthz', async (route) =>
    fulfillJson(route, { status: 'healthy' })
  );
  await page.route('**/healthz/all', async (route) =>
    fulfillJson(route, { status: 'healthy', components: {}, schema_version: '1.0' })
  );
  await page.route('**/readyz', async (route) =>
    fulfillJson(route, { ready: true, checks: { db: { ok: true }, worker: { ok: true } } })
  );

  // API routes
  await page.route('**/v1/**', async (route) => {
    const req = route.request();
    const url = new URL(req.url());
    const { pathname } = url;
    const method = req.method();

    if (method === 'OPTIONS') {
      return route.fulfill({ status: 204 });
    }

    // Auth endpoints
    if (pathname === '/v1/auth/me') {
      return fulfillJson(route, {
        schema_version: '1.0',
        user_id: 'user-1',
        email: 'dev@local',
        role: 'admin',
        created_at: now,
        display_name: 'Dev User',
        tenant_id: 'tenant-1',
        permissions: ['inference:execute', 'models:manage'],
        last_login_at: now,
        mfa_enabled: false,
        token_last_rotated_at: now,
        admin_tenants: ['*'],
      });
    }

    if (pathname === '/v1/auth/tenants') {
      return fulfillJson(route, {
        schema_version: '1.0',
        tenants: [{ id: 'tenant-1', name: 'System', role: 'admin' }],
      });
    }

    if (pathname === '/v1/auth/tenants/switch') {
      return fulfillJson(route, {
        schema_version: '1.0',
        token: 'mock-token',
        user_id: 'user-1',
        tenant_id: 'tenant-1',
        role: 'admin',
        expires_in: 3600,
        tenants: [{ id: 'tenant-1', name: 'System', role: 'admin' }],
        admin_tenants: ['*'],
      });
    }

    // Model status endpoint - critical for gating behavior
    if (pathname === '/v1/models/status') {
      return fulfillJson(route, {
        schema_version: '1.0',
        model_id: modelId,
        model_name: modelName,
        status: modelStatus,
        is_loaded: baseModelReady,
        updated_at: now,
      });
    }

    // Model list
    if (pathname === '/v1/models') {
      return fulfillJson(route, {
        models: baseModelReady
          ? [
              {
                id: 'model-1',
                name: 'Demo Model',
                hash_b3: 'b3:hash',
                format: 'gguf',
                backend: 'coreml',
                size_bytes: 1_000_000,
                adapter_count: 0,
                training_job_count: 0,
                imported_at: now,
                updated_at: now,
              },
            ]
          : [],
        total: baseModelReady ? 1 : 0,
      });
    }

    // Backends
    if (pathname === '/v1/backends') {
      return fulfillJson(route, {
        schema_version: '1.0',
        backends: [
          { backend: 'coreml', status: 'healthy', mode: 'real' },
        ],
        default_backend: 'coreml',
      });
    }

    if (pathname === '/v1/backends/capabilities') {
      return fulfillJson(route, {
        schema_version: '1.0',
        hardware: {
          ane_available: true,
          gpu_available: true,
          gpu_type: 'Apple GPU',
          cpu_model: 'Apple Silicon',
        },
        backends: [
          { backend: 'coreml', capabilities: [{ name: 'coreml', available: true }] },
        ],
      });
    }

    // Adapters (empty for base-only mode)
    if (pathname === '/v1/adapters') {
      return fulfillJson(route, []);
    }

    // Adapter stacks
    if (pathname === '/v1/adapter-stacks') {
      return fulfillJson(route, []);
    }

    // Default stack
    if (pathname.includes('/default-stack')) {
      return fulfillJson(route, { schema_version: '1.0', stack_id: null });
    }

    // Chat sessions
    if (pathname.includes('/chat/sessions')) {
      if (method === 'GET') {
        return fulfillJson(route, { sessions: [], total: 0 });
      }
      if (method === 'POST') {
        return fulfillJson(route, {
          id: 'session-new',
          name: 'New Chat',
          stackId: '',
          messages: [],
          createdAt: now,
          updatedAt: now,
        });
      }
    }

    // Workspace active state
    if (pathname.includes('/workspace/active')) {
      return fulfillJson(route, {
        schema_version: '1.0',
        activeBaseModelId: baseModelReady ? modelId : null,
        activePlanId: null,
        activeAdapterIds: [],
      });
    }

    // System metrics (for loading overlay)
    if (pathname === '/v1/metrics/system') {
      return fulfillJson(route, {
        schema_version: '1.0',
        cpu_usage_percent: 1,
        memory_usage_pct: 1,
        memory_total_gb: 16,
      });
    }

    // Workers
    if (pathname === '/v1/workers') {
      return fulfillJson(route, { workers: [], total: 0 });
    }

    // Default fallback
    return fulfillJson(route, { schema_version: '1.0' });
  });
}

// =============================================================================
// Test Suite
// =============================================================================

test.describe('Flow 7: Chat is gated until a model is active', () => {
  test.describe('when no model is loaded', () => {
    test('chat input is disabled with model gating active', async ({ page }) => {
      await setupChatGatingMocks(page, {
        baseModelReady: false,
        modelStatus: 'no-model',
      });

      await page.goto('/chat');

      // Wait for page to stabilize
      await page.waitForLoadState('networkidle');

      // The chat input should be disabled when no model is loaded
      const chatInput = page.locator('[data-testid="chat-input"]');
      await expect(chatInput).toBeVisible();
      await expect(chatInput).toBeDisabled();
    });

    test('displays model gating card with reason and action', async ({ page }) => {
      await setupChatGatingMocks(page, {
        baseModelReady: false,
        modelStatus: 'no-model',
      });

      await page.goto('/chat');
      await page.waitForLoadState('networkidle');

      // The gating card should be visible with title "Base model required"
      const gatingCard = page.locator('text=Base model required');
      await expect(gatingCard).toBeVisible();

      // Should show explanation text
      const explanation = page.locator('text=Load an active base model before running chat');
      await expect(explanation).toBeVisible();

      // Should have a "Load base model" button as direct action
      const loadButton = page.getByRole('button', { name: /load base model/i });
      await expect(loadButton).toBeVisible();
      await expect(loadButton).toBeEnabled();
    });

    test('send button is disabled when model gate is active', async ({ page }) => {
      await setupChatGatingMocks(page, {
        baseModelReady: false,
        modelStatus: 'no-model',
      });

      await page.goto('/chat');
      await page.waitForLoadState('networkidle');

      // Find the send button (Submit button with Send icon)
      const sendButton = page.locator('button[aria-label*="Send"]');
      await expect(sendButton).toBeDisabled();
    });

    test('has refresh button to check model status', async ({ page }) => {
      await setupChatGatingMocks(page, {
        baseModelReady: false,
        modelStatus: 'no-model',
      });

      await page.goto('/chat');
      await page.waitForLoadState('networkidle');

      // Should have a Refresh button
      const refreshButton = page.getByRole('button', { name: /refresh/i });
      await expect(refreshButton).toBeVisible();
      await expect(refreshButton).toBeEnabled();
    });
  });

  test.describe('when model becomes ready', () => {
    test('chat input becomes enabled after model loads', async ({ page }) => {
      // Start with no model
      await setupChatGatingMocks(page, {
        baseModelReady: false,
        modelStatus: 'no-model',
      });

      await page.goto('/chat');
      await page.waitForLoadState('networkidle');

      // Verify initially disabled
      const chatInput = page.locator('[data-testid="chat-input"]');
      await expect(chatInput).toBeDisabled();

      // Now update the mock to return model as ready
      await page.route('**/v1/models/status**', async (route) => {
        await route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({
            schema_version: '1.0',
            model_id: 'model-1',
            model_name: 'Demo Model',
            status: 'ready',
            is_loaded: true,
            updated_at: FIXED_NOW,
          }),
        });
      });

      // Click refresh to trigger status check
      const refreshButton = page.getByRole('button', { name: /refresh/i });
      await refreshButton.click();

      // Wait for the model status to update (polling interval or manual refresh)
      await expect(chatInput).toBeEnabled({ timeout: 10000 });

      // The gating card should be hidden
      const gatingCard = page.locator('text=Base model required');
      await expect(gatingCard).toBeHidden();
    });

    test('user can type and send when model is ready', async ({ page }) => {
      await setupChatGatingMocks(page, {
        baseModelReady: true,
        modelStatus: 'ready',
        modelId: 'model-1',
        modelName: 'Demo Model',
      });

      await page.goto('/chat');
      await page.waitForLoadState('networkidle');

      // Chat input should be enabled
      const chatInput = page.locator('[data-testid="chat-input"]');
      await expect(chatInput).toBeEnabled();

      // User should be able to type
      await chatInput.fill('Hello, this is a test message');
      await expect(chatInput).toHaveValue('Hello, this is a test message');

      // Send button should be enabled when there's input
      const sendButton = page.locator('button[aria-label*="Send"]');
      await expect(sendButton).toBeEnabled();
    });
  });

  test.describe('API request behavior', () => {
    test('does not spam model status API with excessive requests', async ({ page }) => {
      const tracker = createRequestTracker(page);

      await setupChatGatingMocks(page, {
        baseModelReady: false,
        modelStatus: 'no-model',
      });

      await page.goto('/chat');
      await page.waitForLoadState('networkidle');

      // Wait a bit to collect requests
      await page.waitForTimeout(3000);

      // Count model status requests
      const modelStatusRequests = tracker.getMatching(/\/v1\/models\/status/);

      // Should not have excessive requests (allow for initial + maybe 1-2 polling)
      // The default polling interval is 5000ms, so in 3 seconds we should see at most 1-2
      expect(modelStatusRequests.length).toBeLessThanOrEqual(3);
    });

    test('does not send inference requests when model gate is active', async ({ page }) => {
      const tracker = createRequestTracker(page);

      await setupChatGatingMocks(page, {
        baseModelReady: false,
        modelStatus: 'no-model',
      });

      await page.goto('/chat');
      await page.waitForLoadState('networkidle');

      // Try to interact with the disabled input (shouldn't work but let's verify)
      const chatInput = page.locator('[data-testid="chat-input"]');

      // Force-fill the input via JavaScript (bypassing disabled state)
      await page.evaluate(() => {
        const input = document.querySelector('[data-testid="chat-input"]') as HTMLTextAreaElement;
        if (input) {
          input.value = 'Test message';
          input.dispatchEvent(new Event('input', { bubbles: true }));
        }
      });

      // Try pressing Enter to send (should be blocked)
      await chatInput.press('Enter');

      await page.waitForTimeout(500);

      // Verify no inference requests were made
      const inferRequests = tracker.getMatching(/\/v1\/infer/);
      expect(inferRequests.length).toBe(0);
    });
  });

  test.describe('model loading state', () => {
    test('shows loading state when model is being loaded', async ({ page }) => {
      await setupChatGatingMocks(page, {
        baseModelReady: false,
        modelStatus: 'loading',
        modelId: 'model-1',
        modelName: 'Demo Model',
      });

      await page.goto('/chat');
      await page.waitForLoadState('networkidle');

      // Should still gate the input during loading
      const chatInput = page.locator('[data-testid="chat-input"]');
      await expect(chatInput).toBeDisabled();

      // Status should indicate loading
      const statusText = page.locator('text=Loading base model');
      await expect(statusText).toBeVisible();
    });

    test('shows error state when model fails to load', async ({ page }) => {
      await setupChatGatingMocks(page, {
        baseModelReady: false,
        modelStatus: 'error',
        modelId: 'model-1',
        modelName: 'Demo Model',
      });

      await page.goto('/chat');
      await page.waitForLoadState('networkidle');

      // Should still gate the input on error
      const chatInput = page.locator('[data-testid="chat-input"]');
      await expect(chatInput).toBeDisabled();

      // The gating card should be visible
      const gatingCard = page.locator('text=Base model required');
      await expect(gatingCard).toBeVisible();
    });
  });

  test.describe('accessibility', () => {
    test('disabled input has proper aria attributes', async ({ page }) => {
      await setupChatGatingMocks(page, {
        baseModelReady: false,
        modelStatus: 'no-model',
      });

      await page.goto('/chat');
      await page.waitForLoadState('networkidle');

      const chatInput = page.locator('[data-testid="chat-input"]');

      // Check that the input is properly marked as disabled
      await expect(chatInput).toHaveAttribute('disabled', '');

      // Check aria-label exists for screen readers
      const ariaLabel = await chatInput.getAttribute('aria-label');
      expect(ariaLabel).toBeTruthy();
    });

    test('gating card is keyboard navigable', async ({ page }) => {
      await setupChatGatingMocks(page, {
        baseModelReady: false,
        modelStatus: 'no-model',
      });

      await page.goto('/chat');
      await page.waitForLoadState('networkidle');

      // Tab to the load button
      const loadButton = page.getByRole('button', { name: /load base model/i });

      // Focus should be reachable via keyboard
      await loadButton.focus();
      await expect(loadButton).toBeFocused();

      // Button should be activatable with Enter
      await expect(loadButton).toBeEnabled();
    });
  });
});
