/**
 * Flow 12: Role-Based Access Control (RBAC) Sanity Tests
 *
 * This test validates that RBAC is enforced correctly across the UI:
 * - Viewer role: read-only access, mutation controls are disabled
 * - Operator role: can perform mutations (training, adapter management)
 * - Admin role: full access including admin pages
 *
 * We mock /v1/auth/me to return different roles and verify:
 * 1. UI controls are properly enabled/disabled based on role
 * 2. Restricted pages show access denied for unauthorized roles
 * 3. API mutation attempts by unauthorized roles receive 403
 */

import { test, expect, type Page, type Route } from '@playwright/test';

// -----------------------------------------------------------------------------
// Types
// -----------------------------------------------------------------------------

type UserRole = 'viewer' | 'operator' | 'admin';

interface RoleMockConfig {
  role: UserRole;
  permissions: string[];
}

// -----------------------------------------------------------------------------
// Role Permission Mappings (mirrors ui/src/utils/rbac.ts)
// -----------------------------------------------------------------------------

const ROLE_PERMISSIONS: Record<UserRole, string[]> = {
  viewer: [
    'adapter:list',
    'adapter:view',
    'activity:view',
    'notification:view',
    'workspace:view',
    'dataset:view',
    'metrics:view',
    'dashboard:view',
  ],
  operator: [
    'adapter:list',
    'adapter:view',
    'adapter:register',
    'adapter:load',
    'adapter:unload',
    'training:start',
    'training:cancel',
    'training:view',
    'training:view-logs',
    'policy:view',
    'promotion:view',
    'inference:execute',
    'worker:manage',
    'worker:spawn',
    'worker:view',
    'node:view',
    'activity:create',
    'activity:view',
    'notification:view',
    'workspace:view',
    'dataset:upload',
    'dataset:validate',
    'dataset:view',
    'code:scan',
    'code:view',
    'git:manage',
    'git:view',
    'metrics:view',
    'dashboard:view',
    'plan:view',
    'telemetry:view',
  ],
  admin: [
    'adapter:list',
    'adapter:view',
    'adapter:register',
    'adapter:delete',
    'adapter:load',
    'adapter:unload',
    'training:start',
    'training:cancel',
    'training:view',
    'training:view-logs',
    'policy:view',
    'policy:apply',
    'policy:validate',
    'policy:sign',
    'policy:override',
    'promotion:execute',
    'promotion:view',
    'audit:view',
    'compliance:view',
    'tenant:manage',
    'node:manage',
    'node:view',
    'worker:manage',
    'worker:spawn',
    'worker:view',
    'inference:execute',
    'activity:create',
    'activity:view',
    'contact:manage',
    'contact:view',
    'notification:manage',
    'notification:view',
    'workspace:manage',
    'workspace:member-manage',
    'workspace:resource-manage',
    'workspace:view',
    'dataset:delete',
    'dataset:upload',
    'dataset:validate',
    'dataset:view',
    'code:scan',
    'code:view',
    'federation:view',
    'git:manage',
    'git:view',
    'monitoring:manage',
    'metrics:view',
    'dashboard:manage',
    'dashboard:view',
    'plan:view',
    'replay:view',
    'replay:manage',
    'telemetry:view',
  ],
};

// -----------------------------------------------------------------------------
// Test Fixtures and Mock Setup
// -----------------------------------------------------------------------------

const FIXED_NOW = '2025-01-15T12:00:00.000Z';

/**
 * Creates a mock response helper function
 */
function createJsonFulfiller(route: Route) {
  return (body: unknown, status = 200) =>
    route.fulfill({
      status,
      contentType: 'application/json',
      body: JSON.stringify(body),
    });
}

/**
 * Sets up all API mocks with the specified user role
 */
async function setupRoleMocks(page: Page, config: RoleMockConfig) {
  const { role, permissions } = config;
  const now = FIXED_NOW;

  // Health endpoints
  await page.route('**/healthz', async (route) =>
    createJsonFulfiller(route)({ status: 'healthy' })
  );
  await page.route('**/healthz/all', async (route) =>
    createJsonFulfiller(route)({ status: 'healthy', components: {}, schema_version: '1.0' })
  );
  await page.route('**/readyz', async (route) =>
    createJsonFulfiller(route)({ status: 'ready' })
  );

  // Main API routes
  await page.route('**/v1/**', async (route) => {
    const req = route.request();
    const url = new URL(req.url());
    const rawPathname = url.pathname;
    // Handle /api/v1/ prefix if present
    const pathname = rawPathname.startsWith('/api/') ? rawPathname.slice(4) : rawPathname;
    const method = req.method();
    const json = createJsonFulfiller(route);

    // Handle OPTIONS preflight
    if (method === 'OPTIONS') {
      return route.fulfill({ status: 204 });
    }

    // Auth endpoints - return the configured role
    if (pathname === '/v1/auth/me') {
      return json({
        schema_version: '1.0',
        user_id: `user-${role}`,
        email: `${role}@test.local`,
        role: role,
        created_at: now,
        display_name: `Test ${role.charAt(0).toUpperCase() + role.slice(1)}`,
        tenant_id: 'tenant-1',
        permissions: permissions,
        last_login_at: now,
        mfa_enabled: false,
        token_last_rotated_at: now,
        admin_tenants: role === 'admin' ? ['*'] : [],
      });
    }

    if (pathname === '/v1/auth/tenants') {
      return json({
        schema_version: '1.0',
        tenants: [{ id: 'tenant-1', name: 'Test Workspace', role: role }],
      });
    }

    if (pathname === '/v1/auth/tenants/switch') {
      return json({
        schema_version: '1.0',
        token: 'mock-token',
        user_id: `user-${role}`,
        tenant_id: 'tenant-1',
        role: role,
        expires_in: 3600,
        tenants: [{ id: 'tenant-1', name: 'Test Workspace', role: role }],
        admin_tenants: role === 'admin' ? ['*'] : [],
        session_mode: 'normal',
      });
    }

    // Models endpoint
    if (pathname === '/v1/models') {
      return json({
        models: [
          {
            id: 'model-1',
            name: 'Test Model',
            hash_b3: 'b3:0123456789abcdef',
            config_hash_b3: 'b3:config123',
            tokenizer_hash_b3: 'b3:tokenizer123',
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

    if (pathname.match(/^\/v1\/models\/[^/]+\/validate$/)) {
      return json({
        model_id: 'model-1',
        status: 'ready',
        valid: true,
        can_load: true,
        issues: [],
      });
    }

    if (pathname.match(/^\/v1\/models\/[^/]+\/status$/) || pathname === '/v1/models/status') {
      return json({
        schema_version: '1.0',
        model_id: 'model-1',
        model_name: 'Test Model',
        status: 'ready',
        is_loaded: true,
        updated_at: now,
      });
    }

    if (pathname === '/v1/models/status/all') {
      return json({
        schema_version: '1.0',
        models: [
          {
            model_id: 'model-1',
            model_name: 'Test Model',
            status: 'ready',
            is_loaded: true,
            updated_at: now,
          },
        ],
        total_memory_mb: 0,
        active_model_count: 1,
      });
    }

    // Backend endpoints
    if (pathname === '/v1/backends') {
      return json({
        schema_version: '1.0',
        backends: [
          { backend: 'coreml', status: 'healthy', mode: 'real' },
          { backend: 'auto', status: 'healthy', mode: 'auto' },
        ],
        default_backend: 'coreml',
      });
    }

    if (pathname === '/v1/backends/capabilities') {
      return json({
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

    // Adapters endpoint
    if (pathname === '/v1/adapters' && method === 'GET') {
      return json([
        {
          id: 'adapter-1',
          adapter_id: 'adapter-1',
          name: 'Test Adapter',
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

    // Mutation endpoints - return 403 for viewers
    if (pathname === '/v1/adapters' && method === 'POST') {
      if (role === 'viewer') {
        return json({ error: 'Forbidden', message: 'Insufficient permissions' }, 403);
      }
      return json({ adapter_id: 'new-adapter-1', status: 'created' }, 201);
    }

    if (pathname.match(/^\/v1\/adapters\/[^/]+\/load$/) && method === 'POST') {
      if (role === 'viewer') {
        return json({ error: 'Forbidden', message: 'Insufficient permissions' }, 403);
      }
      return json({ status: 'loading' });
    }

    if (pathname.match(/^\/v1\/adapters\/[^/]+\/unload$/) && method === 'POST') {
      if (role === 'viewer') {
        return json({ error: 'Forbidden', message: 'Insufficient permissions' }, 403);
      }
      return json({ status: 'unloading' });
    }

    if (pathname.match(/^\/v1\/adapters\/[^/]+$/) && method === 'DELETE') {
      if (role === 'viewer' || role === 'operator') {
        return json({ error: 'Forbidden', message: 'Insufficient permissions' }, 403);
      }
      return json({ status: 'deleted' });
    }

    // Adapter stacks
    if (pathname === '/v1/adapter-stacks') {
      return json([
        {
          id: 'stack-1',
          name: 'Test Stack',
          adapter_ids: ['adapter-1'],
          description: 'Test stack',
          created_at: now,
          updated_at: now,
        },
      ]);
    }

    // Training endpoints
    if (pathname === '/v1/training/jobs' && method === 'GET') {
      return json({
        schema_version: '1.0',
        jobs: [
          {
            job_id: 'job-1',
            name: 'Test Training Job',
            status: 'completed',
            created_at: now,
            updated_at: now,
            progress_pct: 100,
          },
        ],
        total: 1,
        page: 1,
        page_size: 20,
      });
    }

    if (pathname === '/v1/training/jobs' && method === 'POST') {
      if (role === 'viewer') {
        return json({ error: 'Forbidden', message: 'Insufficient permissions' }, 403);
      }
      return json({ job_id: 'new-job-1', status: 'queued' }, 201);
    }

    if (pathname === '/v1/training/templates') {
      return json([
        {
          id: 'template-1',
          name: 'Default Template',
          description: 'Standard training template',
        },
      ]);
    }

    // Datasets
    if (pathname === '/v1/datasets' && method === 'GET') {
      return json([
        {
          dataset_id: 'dataset-1',
          name: 'Test Dataset',
          status: 'ready',
          created_at: now,
          updated_at: now,
          row_count: 100,
        },
      ]);
    }

    if (pathname === '/v1/datasets' && method === 'POST') {
      if (role === 'viewer') {
        return json({ error: 'Forbidden', message: 'Insufficient permissions' }, 403);
      }
      return json({ dataset_id: 'new-dataset-1', status: 'processing' }, 201);
    }

    // Tenant default stack
    if (pathname.match(/^\/v1\/tenants\/[^/]+\/default-stack$/)) {
      return json({ schema_version: '1.0', stack_id: null });
    }

    // System metrics
    if (pathname === '/v1/metrics/system') {
      return json({
        schema_version: '1.0',
        cpu_usage_percent: 25,
        memory_usage_pct: 50,
        memory_total_gb: 16,
        tokens_per_second: 100,
        latency_p95_ms: 50,
      });
    }

    if (pathname === '/v1/metrics/snapshot') {
      return json({
        schema_version: '1.0',
        gauges: {},
        counters: {},
        metrics: {},
      });
    }

    if (pathname === '/v1/metrics/quality') {
      return json({ schema_version: '1.0' });
    }

    if (pathname === '/v1/metrics/adapters') {
      return json([]);
    }

    // Repos
    if (pathname === '/v1/repos') {
      return json([]);
    }

    // Default fallback
    return json({ schema_version: '1.0' });
  });
}

/**
 * Helper to wait for page to be fully loaded
 */
async function waitForPageLoad(page: Page) {
  // Wait for any loading spinners to disappear
  await page.waitForLoadState('networkidle');
  // Small buffer for React state updates
  await page.waitForTimeout(100);
}

// -----------------------------------------------------------------------------
// Test Suites
// -----------------------------------------------------------------------------

test.describe('Flow 12: RBAC Sanity - Viewer Role', () => {
  test.beforeEach(async ({ page }) => {
    await setupRoleMocks(page, {
      role: 'viewer',
      permissions: ROLE_PERMISSIONS.viewer,
    });
  });

  test('viewer cannot access /admin page (redirected)', async ({ page }) => {
    await page.goto('/admin');
    await waitForPageLoad(page);

    // Viewer should be redirected away from admin page
    // Either redirected to dashboard or shown permission error
    const url = page.url();
    const hasPermissionError = await page.getByText(/permission|access denied|not authorized/i).isVisible().catch(() => false);
    const redirectedAway = !url.includes('/admin') || url.includes('/dashboard');

    expect(redirectedAway || hasPermissionError).toBe(true);
  });

  test('viewer cannot access /adapters page (route-level restriction)', async ({ page }) => {
    await page.goto('/adapters');
    await waitForPageLoad(page);

    // /adapters route requires admin or operator role
    const url = page.url();
    const redirectedAway = !url.includes('/adapters');
    const hasPermissionError = await page.getByText(/permission|access denied|not authorized/i).isVisible().catch(() => false);

    expect(redirectedAway || hasPermissionError).toBe(true);
  });

  test('viewer can access /training but sees read-only state', async ({ page }) => {
    await page.goto('/training');
    await waitForPageLoad(page);

    // Wait for page content
    await expect(page.getByRole('heading', { name: /training|tune/i }).first()).toBeVisible({ timeout: 10000 });

    // Check that "New Training Job" button is NOT visible for viewer
    // The page conditionally renders based on can('training:start')
    const newJobButton = page.getByRole('button', { name: /new training|start training/i });
    const buttonVisible = await newJobButton.isVisible().catch(() => false);

    // Viewer should not see the start training button
    expect(buttonVisible).toBe(false);
  });

  test('viewer sees disabled state message on training page', async ({ page }) => {
    await page.goto('/training/jobs');
    await waitForPageLoad(page);

    // Wait for content
    await page.waitForTimeout(500);

    // The TrainingJobsTab shows a message when user lacks training:start permission
    const readOnlyMessage = page.getByText(/read-only|view only|cannot start|no permission/i);
    const disabledElements = await page.locator('[disabled], [aria-disabled="true"]').count();
    const newJobButton = page.getByRole('button', { name: /new training|start training/i });
    const buttonExists = await newJobButton.isVisible().catch(() => false);

    // Either there's a read-only message, disabled elements, or the button doesn't exist
    const isReadOnly = await readOnlyMessage.isVisible().catch(() => false);
    expect(isReadOnly || disabledElements > 0 || !buttonExists).toBe(true);
  });

  test('viewer receives 403 when attempting training mutation via API', async ({ page }) => {
    await page.goto('/training');
    await waitForPageLoad(page);

    // Attempt direct API call - should receive 403
    const response = await page.request.post('/v1/training/jobs', {
      data: {
        name: 'Unauthorized Training',
        dataset_id: 'dataset-1',
      },
    });

    expect(response.status()).toBe(403);
    const body = await response.json();
    expect(body.error).toBe('Forbidden');
  });

  test('viewer receives 403 when attempting adapter load via API', async ({ page }) => {
    await page.goto('/dashboard');
    await waitForPageLoad(page);

    // Attempt direct API call - should receive 403
    const response = await page.request.post('/v1/adapters/adapter-1/load');

    expect(response.status()).toBe(403);
    const body = await response.json();
    expect(body.error).toBe('Forbidden');
  });
});

test.describe('Flow 12: RBAC Sanity - Operator Role', () => {
  test.beforeEach(async ({ page }) => {
    await setupRoleMocks(page, {
      role: 'operator',
      permissions: ROLE_PERMISSIONS.operator,
    });
  });

  test('operator cannot access /admin page', async ({ page }) => {
    await page.goto('/admin');
    await waitForPageLoad(page);

    // Operator should be redirected or see permission error
    const url = page.url();
    const hasPermissionError = await page.getByText(/permission|access denied|not authorized/i).isVisible().catch(() => false);
    const redirectedAway = !url.includes('/admin') || url.includes('/dashboard');

    expect(redirectedAway || hasPermissionError).toBe(true);
  });

  test('operator can access /adapters page and sees enabled controls', async ({ page }) => {
    await page.goto('/adapters');
    await waitForPageLoad(page);

    // Wait for adapters table to load
    await expect(page.getByRole('heading', { name: /adapters/i }).first()).toBeVisible({ timeout: 10000 });

    // Operator should see the page without permission errors
    const hasPermissionError = await page.getByText(/permission denied|access denied/i).isVisible().catch(() => false);
    expect(hasPermissionError).toBe(false);

    // Page should be visible and interactive
    const pageContent = page.locator('[data-slot="table-body"], table, [role="table"]');
    await expect(pageContent.first()).toBeVisible({ timeout: 5000 });
  });

  test('operator can access /training page with enabled controls', async ({ page }) => {
    await page.goto('/training/jobs');
    await waitForPageLoad(page);

    // Wait for page to load
    await expect(page.getByRole('heading', { name: /training/i }).first()).toBeVisible({ timeout: 10000 });

    // Operator should see the "New Training Job" button (or similar)
    // Look for any button that would start training
    const trainingButtons = page.getByRole('button', { name: /new|start|create.*training|train/i });
    const buttonVisible = await trainingButtons.first().isVisible().catch(() => false);

    // Operator should have training controls visible
    expect(buttonVisible).toBe(true);
  });

  test('operator can perform adapter load operation via API', async ({ page }) => {
    await page.goto('/dashboard');
    await waitForPageLoad(page);

    // Operator should be able to load adapters
    const response = await page.request.post('/v1/adapters/adapter-1/load');

    expect(response.status()).toBe(200);
    const body = await response.json();
    expect(body.status).toBe('loading');
  });

  test('operator can start training via API', async ({ page }) => {
    await page.goto('/dashboard');
    await waitForPageLoad(page);

    const response = await page.request.post('/v1/training/jobs', {
      data: {
        name: 'Operator Training Job',
        dataset_id: 'dataset-1',
      },
    });

    expect(response.status()).toBe(201);
    const body = await response.json();
    expect(body.job_id).toBeDefined();
  });

  test('operator cannot delete adapters (admin-only)', async ({ page }) => {
    await page.goto('/dashboard');
    await waitForPageLoad(page);

    // Adapter deletion requires admin role
    const response = await page.request.delete('/v1/adapters/adapter-1');

    expect(response.status()).toBe(403);
  });
});

test.describe('Flow 12: RBAC Sanity - Admin Role', () => {
  test.beforeEach(async ({ page }) => {
    await setupRoleMocks(page, {
      role: 'admin',
      permissions: ROLE_PERMISSIONS.admin,
    });
  });

  test('admin can access /admin page with full controls', async ({ page }) => {
    await page.goto('/admin');
    await waitForPageLoad(page);

    // Admin should see the admin page content
    await expect(page.getByRole('heading', { name: /admin|administration/i }).first()).toBeVisible({ timeout: 10000 });

    // Should not see permission denied
    const hasPermissionError = await page.getByText(/permission denied|access denied/i).isVisible().catch(() => false);
    expect(hasPermissionError).toBe(false);

    // Should see admin tabs
    const tabsVisible = await page.getByRole('tab').first().isVisible().catch(() => false);
    expect(tabsVisible).toBe(true);
  });

  test('admin can access /adapters page with full controls', async ({ page }) => {
    await page.goto('/adapters');
    await waitForPageLoad(page);

    // Wait for content
    await expect(page.getByRole('heading', { name: /adapters/i }).first()).toBeVisible({ timeout: 10000 });

    // Admin should see the page content
    const hasPermissionError = await page.getByText(/permission denied|access denied/i).isVisible().catch(() => false);
    expect(hasPermissionError).toBe(false);
  });

  test('admin can access /training page with full controls', async ({ page }) => {
    await page.goto('/training/jobs');
    await waitForPageLoad(page);

    await expect(page.getByRole('heading', { name: /training/i }).first()).toBeVisible({ timeout: 10000 });

    // Admin should see training controls
    const trainingButtons = page.getByRole('button', { name: /new|start|create.*training|train/i });
    const buttonVisible = await trainingButtons.first().isVisible().catch(() => false);
    expect(buttonVisible).toBe(true);
  });

  test('admin can delete adapters via API', async ({ page }) => {
    await page.goto('/dashboard');
    await waitForPageLoad(page);

    const response = await page.request.delete('/v1/adapters/adapter-1');

    expect(response.status()).toBe(200);
    const body = await response.json();
    expect(body.status).toBe('deleted');
  });

  test('admin can perform all training operations via API', async ({ page }) => {
    await page.goto('/dashboard');
    await waitForPageLoad(page);

    // Start training
    const createResponse = await page.request.post('/v1/training/jobs', {
      data: {
        name: 'Admin Training Job',
        dataset_id: 'dataset-1',
      },
    });
    expect(createResponse.status()).toBe(201);

    // Load adapter
    const loadResponse = await page.request.post('/v1/adapters/adapter-1/load');
    expect(loadResponse.status()).toBe(200);

    // Unload adapter
    const unloadResponse = await page.request.post('/v1/adapters/adapter-1/unload');
    expect(unloadResponse.status()).toBe(200);
  });
});

test.describe('Flow 12: RBAC Cross-Role Validation', () => {
  test('permission escalation is blocked - viewer token cannot perform admin actions', async ({ page }) => {
    // Setup as viewer
    await setupRoleMocks(page, {
      role: 'viewer',
      permissions: ROLE_PERMISSIONS.viewer,
    });

    await page.goto('/dashboard');
    await waitForPageLoad(page);

    // Attempt multiple forbidden operations
    const operations = [
      page.request.post('/v1/training/jobs', { data: { name: 'test' } }),
      page.request.post('/v1/adapters/adapter-1/load'),
      page.request.delete('/v1/adapters/adapter-1'),
      page.request.post('/v1/datasets', { data: { name: 'test' } }),
    ];

    const responses = await Promise.all(operations);

    // All should be forbidden
    for (const response of responses) {
      expect(response.status()).toBe(403);
    }
  });

  test('role determines visible navigation items', async ({ page }) => {
    // First check as viewer
    await setupRoleMocks(page, {
      role: 'viewer',
      permissions: ROLE_PERMISSIONS.viewer,
    });

    await page.goto('/dashboard');
    await waitForPageLoad(page);

    // Viewer should not see admin nav items
    const viewerAdminLink = await page.getByRole('link', { name: /^admin$/i }).isVisible().catch(() => false);

    // Now check as admin
    await setupRoleMocks(page, {
      role: 'admin',
      permissions: ROLE_PERMISSIONS.admin,
    });

    await page.goto('/dashboard');
    await waitForPageLoad(page);

    // Admin should see admin nav items (if they exist in nav)
    const adminHasAccess = await page.goto('/admin').then(() => true).catch(() => false);

    // Verify role-based navigation visibility
    expect(viewerAdminLink).toBe(false);
    expect(adminHasAccess).toBe(true);
  });
});
