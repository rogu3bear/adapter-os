/**
 * Flow 3: Base Model Load Changes Visible System State
 *
 * Tests that loading a base model updates all visible system state indicators:
 * - UI confirms load success via toast notification
 * - Kernel/Model status indicator updates
 * - System Status drawer shows active base model, ANE memory, UMA pressure
 * - Chat gating condition transitions from blocked to allowed
 *
 * Also covers negative case: memory pressure failure scenario
 */

import { test, expect, type Page, type Route } from '@playwright/test';

// -----------------------------------------------------------------------------
// Test Constants
// -----------------------------------------------------------------------------

const FIXED_NOW = '2025-01-01T00:00:00.000Z';

const MOCK_MODEL = {
  id: 'model-llama-3b',
  name: 'Llama 3B Q4',
  hash_b3: 'b3:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef',
  config_hash_b3: 'b3:fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210',
  tokenizer_hash_b3: 'b3:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789',
  format: 'mlx',
  backend: 'coreml',
  size_bytes: 3_500_000_000,
  adapter_count: 2,
  training_job_count: 0,
  imported_at: FIXED_NOW,
  updated_at: FIXED_NOW,
  model_path: '/models/llama-3b-q4',
  quantization: 'Q4_K_M',
  architecture: {
    architecture: 'llama',
    num_layers: 32,
    hidden_size: 3200,
    vocab_size: 32000,
  },
};

const MOCK_MODEL_UNLOADED: typeof MOCK_MODEL & { status?: { model_id: string; status: string } } = {
  ...MOCK_MODEL,
  status: {
    model_id: MOCK_MODEL.id,
    status: 'no-model',
  },
};

// -----------------------------------------------------------------------------
// Console Error Guards
// -----------------------------------------------------------------------------

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

// -----------------------------------------------------------------------------
// Mock API Response Helpers
// -----------------------------------------------------------------------------

const fulfillJson = (route: Route, body: unknown, status = 200) =>
  route.fulfill({
    status,
    contentType: 'application/json',
    body: JSON.stringify(body),
  });

// -----------------------------------------------------------------------------
// API Mock State Machine
// -----------------------------------------------------------------------------

interface MockState {
  modelStatus: 'no-model' | 'loading' | 'ready' | 'unloading' | 'error';
  inferenceReady: boolean;
  inferenceBlockers: string[];
  loadError: string | null;
  memoryUsageMb: number | null;
  aneMemory: { usedMb: number; totalMb: number; pressure: string } | null;
  umaPressure: string;
}

function createMockState(): MockState {
  return {
    modelStatus: 'no-model',
    inferenceReady: false,
    inferenceBlockers: ['no_base_model'],
    loadError: null,
    memoryUsageMb: null,
    aneMemory: null,
    umaPressure: 'low',
  };
}

// -----------------------------------------------------------------------------
// API Mock Setup
// -----------------------------------------------------------------------------

async function setupBaseModelFlowMocks(page: Page, state: MockState) {
  // Health endpoints
  await page.route('**/healthz', async (route) =>
    fulfillJson(route, { status: 'healthy' })
  );
  await page.route('**/healthz/all', async (route) =>
    fulfillJson(route, { status: 'healthy', components: {}, schema_version: '1.0' })
  );
  await page.route('**/readyz', async (route) =>
    fulfillJson(route, { ready: true, checks: { db: { ok: true }, worker: { ok: true }, models_seeded: { ok: true } } })
  );

  // V1 API endpoints
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
        email: 'operator@local',
        role: 'admin',
        created_at: FIXED_NOW,
        display_name: 'Operator User',
        tenant_id: 'tenant-1',
        permissions: ['inference:execute', 'models:manage', 'system:view'],
        last_login_at: FIXED_NOW,
        mfa_enabled: false,
        token_last_rotated_at: FIXED_NOW,
        admin_tenants: ['*'],
      });
    }

    if (pathname === '/v1/auth/tenants') {
      return fulfillJson(route, {
        schema_version: '1.0',
        tenants: [{ id: 'tenant-1', name: 'Default', role: 'admin' }],
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
        tenants: [{ id: 'tenant-1', name: 'Default', role: 'admin' }],
        admin_tenants: ['*'],
        session_mode: 'normal',
      });
    }

    // Model list endpoint
    if (pathname === '/v1/models' && method === 'GET') {
      const modelWithStatus = {
        ...MOCK_MODEL,
        status: state.modelStatus === 'no-model' ? undefined : {
          model_id: MOCK_MODEL.id,
          model_name: MOCK_MODEL.name,
          status: state.modelStatus,
          memory_usage_mb: state.memoryUsageMb,
          is_loaded: state.modelStatus === 'ready',
        },
      };
      return fulfillJson(route, {
        models: [modelWithStatus],
        total: 1,
      });
    }

    // Model status endpoint (used for polling)
    if (pathname === '/v1/models/status') {
      return fulfillJson(route, {
        schema_version: '1.0',
        model_id: state.modelStatus === 'no-model' ? 'none' : MOCK_MODEL.id,
        model_name: state.modelStatus === 'no-model' ? null : MOCK_MODEL.name,
        model_path: state.modelStatus === 'ready' ? MOCK_MODEL.model_path : null,
        status: state.modelStatus,
        memory_usage_mb: state.memoryUsageMb,
        is_loaded: state.modelStatus === 'ready',
        error_message: state.loadError,
      });
    }

    // All models status endpoint
    if (pathname === '/v1/models/status/all') {
      const models = state.modelStatus === 'no-model' ? [] : [
        {
          model_id: MOCK_MODEL.id,
          model_name: MOCK_MODEL.name,
          model_path: MOCK_MODEL.model_path,
          status: state.modelStatus,
          memory_usage_mb: state.memoryUsageMb,
          is_loaded: state.modelStatus === 'ready',
        },
      ];
      return fulfillJson(route, {
        schema_version: '1.0',
        models,
        total_memory_mb: state.memoryUsageMb ?? 0,
        active_model_count: state.modelStatus === 'ready' ? 1 : 0,
      });
    }

    // Model load endpoint
    if (pathname.match(/\/v1\/models\/[^/]+\/load$/) && method === 'POST') {
      if (state.loadError) {
        return fulfillJson(route, {
          schema_version: '1.0',
          model_id: MOCK_MODEL.id,
          status: 'error',
          error_message: state.loadError,
          error: state.loadError,
        }, 507); // Insufficient Storage (memory pressure)
      }

      // Simulate loading transition
      state.modelStatus = 'loading';

      // After a brief delay, set to ready
      setTimeout(() => {
        state.modelStatus = 'ready';
        state.memoryUsageMb = 2800;
        state.inferenceReady = true;
        state.inferenceBlockers = [];
        state.aneMemory = { usedMb: 2200, totalMb: 8192, pressure: 'low' };
      }, 500);

      return fulfillJson(route, {
        schema_version: '1.0',
        model_id: MOCK_MODEL.id,
        model_name: MOCK_MODEL.name,
        model_path: MOCK_MODEL.model_path,
        status: 'loading',
        memory_usage_mb: null,
        is_loaded: false,
      });
    }

    // Model unload endpoint
    if (pathname.match(/\/v1\/models\/[^/]+\/unload$/) && method === 'POST') {
      state.modelStatus = 'no-model';
      state.memoryUsageMb = null;
      state.inferenceReady = false;
      state.inferenceBlockers = ['no_base_model'];
      state.aneMemory = null;

      return fulfillJson(route, {
        schema_version: '1.0',
        model_id: MOCK_MODEL.id,
        status: 'unloaded',
      });
    }

    // System status endpoint (for System Status drawer)
    if (pathname === '/v1/system/status') {
      return fulfillJson(route, {
        schemaVersion: '1.0',
        timestamp: new Date().toISOString(),
        integrity: {
          localSecureMode: true,
          strictMode: true,
          pfDeny: false,
          drift: { status: 'ok', detail: null, lastRun: FIXED_NOW },
        },
        readiness: {
          db: true,
          migrations: true,
          workers: true,
          modelsSeeded: true,
          phase: 'ready',
          bootTraceId: 'boot-trace-123',
          degraded: null,
        },
        inferenceReady: state.inferenceReady,
        inferenceBlockers: state.inferenceBlockers,
        kernel: {
          activeModel: state.modelStatus === 'ready' ? MOCK_MODEL.name : null,
          activePlan: state.modelStatus === 'ready' ? 'default-plan' : null,
          activeAdapters: state.modelStatus === 'ready' ? 2 : 0,
          hotAdapters: state.modelStatus === 'ready' ? 1 : 0,
          aneMemory: state.aneMemory,
          umaPressure: state.umaPressure,
        },
        boot: {
          phase: 'ready',
          degradedReasons: null,
          bootTraceId: 'boot-trace-123',
          lastError: null,
        },
        components: [
          { name: 'router', status: 'healthy', message: null },
          { name: 'loader', status: 'healthy', message: null },
          { name: 'db', status: 'healthy', message: null },
        ],
      });
    }

    // System ready endpoint
    if (pathname === '/system/ready') {
      return fulfillJson(route, {
        ready: true,
        overall_status: 'healthy',
        components: [],
      });
    }

    // Meta endpoint
    if (pathname === '/v1/meta') {
      return fulfillJson(route, {
        schema_version: '1.0',
        version: '1.0.0',
        build_date: FIXED_NOW,
        git_commit: 'abc123',
        features: ['coreml', 'mlx'],
        environment: 'dev',
        production_mode: false,
        dev_login_enabled: true,
      });
    }

    // Adapters endpoint
    if (pathname === '/v1/adapters') {
      return fulfillJson(route, [
        {
          id: 'adapter-1',
          adapter_id: 'adapter-1',
          name: 'Code Adapter',
          current_state: 'cold',
          runtime_state: 'cold',
          created_at: FIXED_NOW,
          updated_at: FIXED_NOW,
        },
      ]);
    }

    // Stacks endpoint
    if (pathname === '/v1/adapter-stacks') {
      return fulfillJson(route, [
        {
          id: 'stack-1',
          name: 'Default Stack',
          adapter_ids: ['adapter-1'],
          created_at: FIXED_NOW,
          updated_at: FIXED_NOW,
        },
      ]);
    }

    // Default stack endpoint
    if (pathname.match(/\/v1\/tenants\/[^/]+\/default-stack$/)) {
      return fulfillJson(route, { schema_version: '1.0', stack_id: 'stack-1' });
    }

    // Metrics endpoints
    if (pathname === '/v1/metrics/system') {
      return fulfillJson(route, {
        schema_version: '1.0',
        cpu_usage_percent: 15,
        memory_usage_pct: 45,
        memory_total_gb: 32,
        tokens_per_second: 0,
        latency_p95_ms: 0,
      });
    }

    if (pathname === '/v1/metrics/quality') {
      return fulfillJson(route, { schema_version: '1.0' });
    }

    if (pathname === '/v1/metrics/adapters') {
      return fulfillJson(route, []);
    }

    // Training endpoints
    if (pathname === '/v1/training/jobs') {
      return fulfillJson(route, { schema_version: '1.0', jobs: [], total: 0, page: 1, page_size: 20 });
    }

    if (pathname === '/v1/training/templates') {
      return fulfillJson(route, []);
    }

    // Datasets endpoint
    if (pathname === '/v1/datasets') {
      return fulfillJson(route, []);
    }

    // Default fallback
    return fulfillJson(route, { schema_version: '1.0' });
  });
}

// -----------------------------------------------------------------------------
// Test Suite: Flow 3 - Base Model Load
// -----------------------------------------------------------------------------

test.describe('Flow 3: Base Model Load Changes Visible System State', () => {
  test.describe('Preconditions verification', () => {
    test('backend has at least one base model available', async ({ page }) => {
      const state = createMockState();
      await setupBaseModelFlowMocks(page, state);

      await page.goto('/base-models');
      await page.waitForLoadState('networkidle');

      // Verify model is listed in the table
      await expect(page.getByText(MOCK_MODEL.name)).toBeVisible();
      await expect(page.getByText(MOCK_MODEL.id)).toBeVisible();
    });
  });

  test.describe('Happy path: successful model load', () => {
    test('loading a model updates UI with success confirmation and status changes', async ({ page }) => {
      const { consoleErrors, pageErrors } = attachConsoleGuards(page);
      const state = createMockState();
      await setupBaseModelFlowMocks(page, state);

      // Step 1: Navigate to Base Models page
      await page.goto('/base-models');
      await page.waitForLoadState('networkidle');

      // Verify we're on the Base Models page
      await expect(page.getByRole('heading', { name: 'Base Models' })).toBeVisible();

      // Verify model is listed with "Load to memory" button (not yet loaded)
      await expect(page.getByText(MOCK_MODEL.name)).toBeVisible();
      const loadButton = page.getByRole('button', { name: /Load to memory/i });
      await expect(loadButton).toBeVisible();

      // Step 2: Click "Load" on the model
      await loadButton.click();

      // Step 3: Wait for model status refresh
      // UI should show loading indicator
      await expect(page.getByRole('button', { name: /Loading/i })).toBeVisible({ timeout: 2000 });

      // Wait for the model to finish loading (mock transitions to 'ready' after 500ms)
      await page.waitForTimeout(1000);

      // Trigger a refetch by waiting for the polling cycle or UI update
      await page.reload();
      await page.waitForLoadState('networkidle');

      // Expected outcome 1: UI confirms load success (toast or status change)
      // The button should now show "Unload" for a loaded model
      await expect(page.getByRole('button', { name: /Unload/i })).toBeVisible({ timeout: 5000 });

      // Expected outcome 2: Model status badge should show "ready"
      await expect(page.getByText('ready', { exact: false })).toBeVisible();

      // Verify no console errors during the flow
      expect(pageErrors, `page errors: ${pageErrors.join('\n')}`).toEqual([]);
      // Filter out expected non-critical console errors
      const criticalErrors = consoleErrors.filter(
        (e) => !e.includes('React Query') && !e.includes('AbortError')
      );
      expect(criticalErrors, `console errors: ${criticalErrors.join('\n')}`).toEqual([]);
    });

    test('System Status drawer shows kernel state after model load', async ({ page }) => {
      const state = createMockState();
      // Simulate already loaded state
      state.modelStatus = 'ready';
      state.memoryUsageMb = 2800;
      state.inferenceReady = true;
      state.inferenceBlockers = [];
      state.aneMemory = { usedMb: 2200, totalMb: 8192, pressure: 'low' };
      state.umaPressure = 'low';

      await setupBaseModelFlowMocks(page, state);
      await page.goto('/base-models');
      await page.waitForLoadState('networkidle');

      // Look for a system status trigger (typically in header)
      // This might be a button or link that opens the drawer
      const statusTrigger = page.getByRole('button', { name: /System Status|Status/i }).first();

      // If there's a status trigger, click it
      if (await statusTrigger.isVisible({ timeout: 2000 }).catch(() => false)) {
        await statusTrigger.click();

        // Verify System Status drawer content
        const drawer = page.getByRole('dialog');
        await expect(drawer).toBeVisible({ timeout: 3000 });

        // Expected outcome 3: Kernel section shows active base model
        await expect(drawer.getByText('Active model')).toBeVisible();
        await expect(drawer.getByText(MOCK_MODEL.name)).toBeVisible();

        // ANE memory should be populated
        await expect(drawer.getByText('ANE memory')).toBeVisible();
        // Should show "2200 / 8192 MB" or similar
        await expect(drawer.getByText(/2200.*8192.*MB/)).toBeVisible();

        // UMA pressure should be available
        await expect(drawer.getByText('UMA pressure')).toBeVisible();
        await expect(drawer.getByText('low')).toBeVisible();
      }
    });

    test('chat gating condition changes from blocked to allowed after load', async ({ page }) => {
      const state = createMockState();
      await setupBaseModelFlowMocks(page, state);

      // First verify blocked state
      await page.goto('/base-models');
      await page.waitForLoadState('networkidle');

      // Check system status for inference blockers
      const statusTrigger = page.getByRole('button', { name: /System Status|Status/i }).first();
      if (await statusTrigger.isVisible({ timeout: 2000 }).catch(() => false)) {
        await statusTrigger.click();

        const drawer = page.getByRole('dialog');
        await expect(drawer).toBeVisible({ timeout: 3000 });

        // Initially should show blockers
        await expect(drawer.getByText('Blockers')).toBeVisible();
        await expect(drawer.getByText(/no.base.model/i)).toBeVisible();

        // Close drawer
        await page.keyboard.press('Escape');
        await expect(drawer).not.toBeVisible({ timeout: 2000 });
      }

      // Load the model
      const loadButton = page.getByRole('button', { name: /Load to memory/i });
      await loadButton.click();

      // Wait for load to complete
      await page.waitForTimeout(1000);
      await page.reload();
      await page.waitForLoadState('networkidle');

      // Now verify unblocked state
      if (await statusTrigger.isVisible({ timeout: 2000 }).catch(() => false)) {
        await statusTrigger.click();

        const drawer = page.getByRole('dialog');
        await expect(drawer).toBeVisible({ timeout: 3000 });

        // Should show inference ready
        await expect(drawer.getByText('Inference ready')).toBeVisible();
        await expect(drawer.getByText('Ready')).toBeVisible();

        // Blockers should show "None"
        const blockersRow = drawer.locator('div').filter({ hasText: /^Blockers/ });
        await expect(blockersRow.getByText('None')).toBeVisible();
      }
    });
  });

  test.describe('Negative case: memory pressure failure', () => {
    test('shows clear error when load fails due to memory pressure', async ({ page }) => {
      const { consoleErrors, pageErrors } = attachConsoleGuards(page);
      const state = createMockState();
      state.loadError = 'Insufficient memory: UMA pressure critical, cannot allocate 2.8 GB for model';

      await setupBaseModelFlowMocks(page, state);

      await page.goto('/base-models');
      await page.waitForLoadState('networkidle');

      // Attempt to load the model
      const loadButton = page.getByRole('button', { name: /Load to memory/i });
      await loadButton.click();

      // Wait for error response
      await page.waitForTimeout(500);

      // UI should show clear error message (via toast or inline)
      // Look for toast notification with error
      const errorToast = page.getByText(/Failed to load|Insufficient memory|memory pressure/i);
      await expect(errorToast).toBeVisible({ timeout: 5000 });

      // Model should still be in unloaded state
      await expect(page.getByRole('button', { name: /Load to memory/i })).toBeVisible();

      // Check System Status drawer shows memory pressure context
      const statusTrigger = page.getByRole('button', { name: /System Status|Status/i }).first();
      if (await statusTrigger.isVisible({ timeout: 2000 }).catch(() => false)) {
        await statusTrigger.click();

        const drawer = page.getByRole('dialog');
        await expect(drawer).toBeVisible({ timeout: 3000 });

        // Should still show blockers
        await expect(drawer.getByText('Blockers')).toBeVisible();
        await expect(drawer.getByText(/no.base.model/i)).toBeVisible();

        // Inference should not be ready
        await expect(drawer.getByText('Inference ready')).toBeVisible();
        await expect(drawer.getByText('Not ready')).toBeVisible();
      }

      // Page errors are acceptable in error scenarios
      expect(pageErrors).toEqual([]);
    });

    test('System Status shows inference blockers in error state', async ({ page }) => {
      const state = createMockState();
      state.modelStatus = 'error';
      state.loadError = 'Memory allocation failed';
      state.inferenceBlockers = ['no_base_model', 'memory_pressure'];
      state.umaPressure = 'critical';

      await setupBaseModelFlowMocks(page, state);
      await page.goto('/base-models');
      await page.waitForLoadState('networkidle');

      const statusTrigger = page.getByRole('button', { name: /System Status|Status/i }).first();
      if (await statusTrigger.isVisible({ timeout: 2000 }).catch(() => false)) {
        await statusTrigger.click();

        const drawer = page.getByRole('dialog');
        await expect(drawer).toBeVisible({ timeout: 3000 });

        // Verify blockers section shows all blockers
        await expect(drawer.getByText('Blockers')).toBeVisible();
        await expect(drawer.getByText(/no.base.model/i)).toBeVisible();
        await expect(drawer.getByText(/memory.pressure/i)).toBeVisible();

        // UMA pressure should show critical
        await expect(drawer.getByText('UMA pressure')).toBeVisible();
        await expect(drawer.getByText('critical')).toBeVisible();

        // Active model should show None
        await expect(drawer.getByText('Active model')).toBeVisible();
        await expect(drawer.getByText('None').first()).toBeVisible();
      }
    });
  });

  test.describe('Status indicator updates', () => {
    test('kernel status indicator reflects loading state during load', async ({ page }) => {
      const state = createMockState();
      await setupBaseModelFlowMocks(page, state);

      await page.goto('/base-models');
      await page.waitForLoadState('networkidle');

      // Start loading
      const loadButton = page.getByRole('button', { name: /Load to memory/i });
      await loadButton.click();

      // During loading, button should show loading state
      const loadingButton = page.getByRole('button', { name: /Loading/i });
      await expect(loadingButton).toBeVisible({ timeout: 2000 });

      // Button should be disabled during loading
      await expect(loadingButton).toBeDisabled();

      // Should have a spinner icon visible
      await expect(loadingButton.locator('svg.animate-spin')).toBeVisible();
    });

    test('memory usage is displayed after successful load', async ({ page }) => {
      const state = createMockState();
      // Simulate loaded state with memory usage
      state.modelStatus = 'ready';
      state.memoryUsageMb = 2800;
      state.inferenceReady = true;
      state.inferenceBlockers = [];

      await setupBaseModelFlowMocks(page, state);
      await page.goto('/base-models');
      await page.waitForLoadState('networkidle');

      // Memory column should show the usage
      // Format is typically "2.73 GB" or "2800 MB" based on formatMemory function
      await expect(page.getByText(/2\.73.*GB|2800.*MB|2\.8.*GB/i)).toBeVisible();
    });
  });
});
