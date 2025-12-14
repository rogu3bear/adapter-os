import { test, expect, type ConsoleMessage, type Page, type Route } from '@playwright/test';

const FIXED_NOW = '2025-01-01T00:00:00.000Z';

function attachConsoleGuards(page: Page) {
  const consoleErrors: string[] = [];
  const pageErrors: string[] = [];

  page.on('console', (msg: ConsoleMessage) => {
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

async function setupPilotDemoMocks(page: Page) {
  const now = FIXED_NOW;
  const delayMs = 800;
  const fulfillJson = (route: Route, body: unknown, status = 200) =>
    route.fulfill({
      status,
      contentType: 'application/json',
      body: JSON.stringify(body),
    });

  await page.route('**/healthz', async (route) => fulfillJson(route, { status: 'healthy' }));
  await page.route('**/healthz/all', async (route) =>
    fulfillJson(route, { status: 'healthy', components: {}, schema_version: '1.0' })
  );
  await page.route('**/readyz', async (route) => fulfillJson(route, { status: 'ready' }));

  await page.route('**/v1/**', async (route) => {
    const req = route.request();
    const url = new URL(req.url());
    const { pathname } = url;
    const method = req.method();

    if (method === 'OPTIONS') {
      return route.fulfill({ status: 204 });
    }

    if (pathname === '/v1/auth/me') {
      return fulfillJson(route, {
        schema_version: '1.0',
        user_id: 'user-1',
        email: 'dev@local',
        role: 'admin',
        created_at: now,
        display_name: 'Dev User',
        tenant_id: 'tenant-1',
        permissions: ['inference:execute', 'metrics:view', 'training:start', 'adapter:register'],
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
        session_mode: 'normal',
      });
    }

    if (pathname === '/v1/models') {
      return fulfillJson(route, {
        models: [
          {
            id: 'model-1',
            name: 'Demo Model',
            hash_b3: 'b3:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef',
            config_hash_b3: 'b3:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef',
            tokenizer_hash_b3: 'b3:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef',
            format: 'gguf',
            backend: 'coreml',
            size_bytes: 1_000_000,
            adapter_count: 1,
            training_job_count: 0,
            imported_at: now,
            updated_at: now,
            architecture: { architecture: 'decoder' },
          },
        ],
        total: 1,
      });
    }

    if (pathname === '/v1/models/model-1/validate') {
      return fulfillJson(route, {
        model_id: 'model-1',
        status: 'ready',
        valid: true,
        can_load: true,
        issues: [],
      });
    }

    if (pathname === '/v1/models/model-1/status') {
      return fulfillJson(route, {
        schema_version: '1.0',
        model_id: 'model-1',
        model_name: 'Demo Model',
        status: 'ready',
        is_loaded: true,
        updated_at: now,
      });
    }

    if (pathname === '/v1/models/status') {
      return fulfillJson(route, {
        schema_version: '1.0',
        model_id: 'model-1',
        model_name: 'Demo Model',
        status: 'ready',
        is_loaded: true,
        updated_at: now,
      });
    }

    if (pathname === '/v1/models/status/all') {
      return fulfillJson(route, {
        schema_version: '1.0',
        models: [
          {
            model_id: 'model-1',
            model_name: 'Demo Model',
            status: 'ready',
            is_loaded: true,
            updated_at: now,
          },
        ],
        total_memory_mb: 0,
        active_model_count: 1,
      });
    }

    if (pathname === '/v1/backends') {
      await new Promise((resolve) => setTimeout(resolve, delayMs));
      return fulfillJson(route, {
        schema_version: '1.0',
        backends: [
          { backend: 'coreml', status: 'healthy', mode: 'real' },
          { backend: 'auto', status: 'healthy', mode: 'auto' },
        ],
        default_backend: 'coreml',
      });
    }

    if (pathname === '/v1/backends/capabilities') {
      await new Promise((resolve) => setTimeout(resolve, delayMs));
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
          { backend: 'auto', capabilities: [{ name: 'auto', available: true }] },
        ],
      });
    }

    if (pathname === '/v1/adapters') {
      await new Promise((resolve) => setTimeout(resolve, delayMs));
      return fulfillJson(route, [
        {
          id: 'adapter-hot',
          adapter_id: 'adapter-hot',
          name: 'Hot Adapter',
          current_state: 'hot',
          runtime_state: 'hot',
          created_at: now,
          updated_at: now,
          lora_tier: 'prod',
          lora_scope: 'general',
          lora_strength: 1,
        },
      ]);
    }

    if (pathname === '/v1/adapter-stacks') {
      return fulfillJson(route, [
        {
          id: 'stack-1',
          name: 'Demo Stack',
          adapter_ids: ['adapter-hot'],
          description: 'Demo stack',
          created_at: now,
          updated_at: now,
        },
      ]);
    }

    if (pathname === '/v1/tenants/tenant-1/default-stack') {
      return fulfillJson(route, { schema_version: '1.0', stack_id: null });
    }

    if (pathname === '/v1/metrics/system') {
      await new Promise((resolve) => setTimeout(resolve, delayMs));
      return fulfillJson(route, {
        schema_version: '1.0',
        cpu_usage_percent: 1,
        memory_usage_pct: 1,
        memory_total_gb: 16,
        tokens_per_second: 0,
        latency_p95_ms: 0,
      });
    }

    if (pathname === '/v1/metrics/snapshot') {
      return fulfillJson(route, {
        schema_version: '1.0',
        gauges: {},
        counters: {},
        metrics: {},
      });
    }

    if (pathname === '/v1/metrics/quality') {
      return fulfillJson(route, { schema_version: '1.0' });
    }

    if (pathname === '/v1/metrics/adapters') {
      return fulfillJson(route, []);
    }

    if (pathname === '/v1/training/jobs') {
      await new Promise((resolve) => setTimeout(resolve, delayMs));
      return fulfillJson(route, {
        schema_version: '1.0',
        jobs: [],
        total: 0,
        page: 1,
        page_size: 20,
      });
    }

    if (pathname === '/v1/training/templates') {
      return fulfillJson(route, []);
    }

    if (pathname === '/v1/datasets') {
      return fulfillJson(route, []);
    }

    if (pathname === '/v1/repos') {
      return fulfillJson(route, []);
    }

    // Default: return a benign empty object so request() doesn't throw.
    return fulfillJson(route, { schema_version: '1.0' });
  });
}

test.describe('Pilot demo smoke', () => {
  test('dashboard shows loading marker then content', async ({ page }) => {
    const { consoleErrors, pageErrors } = attachConsoleGuards(page);
    await setupPilotDemoMocks(page);

    await page.goto('/dashboard');

    const loadingMarker = page.getByLabel('Loading system health');
    await expect(loadingMarker).toBeVisible();
    await expect(loadingMarker).toBeHidden();

    await expect(page.getByRole('heading', { name: 'Dashboard', exact: true }).first()).toBeVisible();

    expect(pageErrors, `page errors: ${pageErrors.join('\n')}`).toEqual([]);
    expect(consoleErrors, `console errors: ${consoleErrors.join('\n')}`).toEqual([]);
  });

  test('inference shows loading marker then playground', async ({ page }) => {
    const { consoleErrors, pageErrors } = attachConsoleGuards(page);
    await setupPilotDemoMocks(page);

    await page.goto('/inference');

    const loadingMarker = page.getByLabel('Loading backend status');
    await expect(loadingMarker).toBeVisible();
    await expect(loadingMarker).toBeHidden();

    await expect(page.getByRole('heading', { name: 'Inference Playground', exact: true }).first()).toBeVisible();
    await expect(page.locator('[data-cy="prompt-input"]')).toBeVisible();

    expect(pageErrors, `page errors: ${pageErrors.join('\n')}`).toEqual([]);
    expect(consoleErrors, `console errors: ${consoleErrors.join('\n')}`).toEqual([]);
  });

  test('adapters shows loading marker then table content', async ({ page }) => {
    const { consoleErrors, pageErrors } = attachConsoleGuards(page);
    await setupPilotDemoMocks(page);

    await page.goto('/adapters');

    const loadingMarker = page.getByLabel('Loading table data');
    await expect(loadingMarker).toBeVisible();
    await expect(loadingMarker).toBeHidden();

    await expect(page.getByRole('heading', { name: 'Adapters', exact: true }).first()).toBeVisible();
    await expect(page.locator('[data-slot="table-body"]').getByText('Hot Adapter', { exact: true })).toBeVisible();

    expect(pageErrors, `page errors: ${pageErrors.join('\n')}`).toEqual([]);
    expect(consoleErrors, `console errors: ${consoleErrors.join('\n')}`).toEqual([]);
  });

  test('training shows loading marker then empty state', async ({ page }) => {
    const { consoleErrors, pageErrors } = attachConsoleGuards(page);
    await setupPilotDemoMocks(page);

    await page.goto('/training/jobs');

    const loadingMarker = page.getByLabel('Loading training jobs...');
    await expect(loadingMarker).toBeVisible();
    await expect(loadingMarker).toBeHidden();

    await expect(page.getByRole('heading', { name: 'Training', exact: true }).first()).toBeVisible();
    await expect(page.getByText('No training jobs found', { exact: true })).toBeVisible();

    expect(pageErrors, `page errors: ${pageErrors.join('\n')}`).toEqual([]);
    expect(consoleErrors, `console errors: ${consoleErrors.join('\n')}`).toEqual([]);
  });

  test('metrics shows loading marker then content', async ({ page }) => {
    const { consoleErrors, pageErrors } = attachConsoleGuards(page);
    await setupPilotDemoMocks(page);

    await page.goto('/metrics');

    const loadingMarker = page.getByLabel('Loading metrics...');
    await expect(loadingMarker).toBeVisible();
    await expect(loadingMarker).toBeHidden();

    await expect(page.getByRole('heading', { name: 'Metrics', exact: true }).first()).toBeVisible();
    await expect(page.getByRole('link', { name: 'View related telemetry', exact: true })).toBeVisible();

    expect(pageErrors, `page errors: ${pageErrors.join('\n')}`).toEqual([]);
    expect(consoleErrors, `console errors: ${consoleErrors.join('\n')}`).toEqual([]);
  });
});
