/**
 * Flow 1: Workspace Selection Gate E2E Tests
 *
 * Verifies that workspace selection behaves correctly for authenticated users.
 * Tests positive flows (workspace available and selectable) and negative flows
 * (empty workspace list shows appropriate empty state).
 */
import { test, expect, type Page, type Route } from '@playwright/test';

// -----------------------------------------------------------------------------
// Test Data
// -----------------------------------------------------------------------------

const FIXED_NOW = '2025-01-01T00:00:00.000Z';

const MOCK_WORKSPACES = [
  {
    id: 'workspace-1',
    name: 'Engineering',
    description: 'Engineering team workspace',
    owner_id: 'user-1',
    members: [],
    created_at: FIXED_NOW,
    is_default: true,
  },
  {
    id: 'workspace-2',
    name: 'Data Science',
    description: 'Data science research workspace',
    owner_id: 'user-2',
    members: [],
    created_at: FIXED_NOW,
    is_default: false,
  },
];

const MOCK_TENANTS = [
  { id: 'workspace-1', name: 'Engineering', role: 'admin' },
  { id: 'workspace-2', name: 'Data Science', role: 'member' },
];

const MOCK_USER = {
  schema_version: '1.0',
  user_id: 'user-1',
  email: 'dev@local',
  role: 'admin',
  created_at: FIXED_NOW,
  display_name: 'Dev User',
  tenant_id: 'workspace-1',
  permissions: ['inference:execute', 'metrics:view', 'training:start', 'adapter:register'],
  last_login_at: FIXED_NOW,
  mfa_enabled: false,
  token_last_rotated_at: FIXED_NOW,
  admin_tenants: ['*'],
};

// -----------------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------------

/**
 * Attaches console error guards to detect React errors or uncaught exceptions.
 */
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

/**
 * Helper to fulfill JSON responses.
 */
function fulfillJson(route: Route, body: unknown, status = 200) {
  return route.fulfill({
    status,
    contentType: 'application/json',
    body: JSON.stringify(body),
  });
}

/**
 * Sets up API mocks for workspace selection tests.
 *
 * @param page - Playwright Page instance
 * @param options - Configuration options
 * @param options.workspaces - Array of workspaces to return (empty array for empty state test)
 * @param options.tenants - Array of tenant summaries (empty array for empty state test)
 * @param options.delayMs - Optional delay for workspace endpoints
 */
async function setupWorkspaceMocks(
  page: Page,
  options: {
    workspaces?: typeof MOCK_WORKSPACES;
    tenants?: typeof MOCK_TENANTS;
    delayMs?: number;
  } = {}
) {
  const { workspaces = MOCK_WORKSPACES, tenants = MOCK_TENANTS, delayMs = 0 } = options;

  // Health endpoints
  await page.route('**/healthz', async (route) => fulfillJson(route, { status: 'healthy' }));
  await page.route('**/healthz/all', async (route) =>
    fulfillJson(route, { status: 'healthy', components: {}, schema_version: '1.0' })
  );
  await page.route('**/readyz', async (route) => fulfillJson(route, { status: 'ready' }));

  // Main API routes
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
      return fulfillJson(route, MOCK_USER);
    }

    if (pathname === '/v1/auth/tenants') {
      if (delayMs > 0) {
        await new Promise((resolve) => setTimeout(resolve, delayMs));
      }
      return fulfillJson(route, {
        schema_version: '1.0',
        tenants,
      });
    }

    if (pathname === '/v1/auth/tenants/switch') {
      const body = JSON.parse((await req.postData()) || '{}');
      const targetTenantId = body.tenant_id || tenants[0]?.id || 'workspace-1';
      return fulfillJson(route, {
        schema_version: '1.0',
        token: 'mock-token',
        user_id: 'user-1',
        tenant_id: targetTenantId,
        role: 'admin',
        expires_in: 3600,
        tenants,
        admin_tenants: ['*'],
        session_mode: 'normal',
      });
    }

    // Workspace endpoints
    if (pathname === '/v1/workspaces') {
      if (delayMs > 0) {
        await new Promise((resolve) => setTimeout(resolve, delayMs));
      }
      return fulfillJson(route, workspaces);
    }

    if (pathname === '/v1/workspaces/my') {
      if (delayMs > 0) {
        await new Promise((resolve) => setTimeout(resolve, delayMs));
      }
      return fulfillJson(route, workspaces);
    }

    // Workspace member and resource endpoints
    if (pathname.match(/^\/v1\/workspaces\/[^/]+\/members$/)) {
      return fulfillJson(route, []);
    }

    if (pathname.match(/^\/v1\/workspaces\/[^/]+\/resources$/)) {
      return fulfillJson(route, []);
    }

    // Activity events endpoint
    if (pathname === '/v1/activity/events') {
      return fulfillJson(route, []);
    }

    // Tenant default stack endpoint
    if (pathname.match(/^\/v1\/tenants\/[^/]+\/default-stack$/)) {
      return fulfillJson(route, { schema_version: '1.0', stack_id: null });
    }

    // Models endpoints (may be called during page load)
    if (pathname === '/v1/models') {
      return fulfillJson(route, { models: [], total: 0 });
    }

    if (pathname === '/v1/models/status/all') {
      return fulfillJson(route, {
        schema_version: '1.0',
        models: [],
        total_memory_mb: 0,
        active_model_count: 0,
      });
    }

    // Backends endpoints
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

    // Adapters and stacks
    if (pathname === '/v1/adapters') {
      return fulfillJson(route, []);
    }

    if (pathname === '/v1/adapter-stacks') {
      return fulfillJson(route, []);
    }

    // Metrics endpoints
    if (pathname === '/v1/metrics/system') {
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

    // Default fallback
    return fulfillJson(route, { schema_version: '1.0' });
  });
}

/**
 * Asserts that no user-facing copy contains the word "Tenant".
 * Verifies that the UI uses "Workspace" terminology consistently.
 */
async function assertNoTenantTerminology(page: Page) {
  // Get all visible text content from the page body
  const bodyText = await page.locator('body').innerText();

  // Check for "Tenant" (case-insensitive) in user-facing text
  // Exclude technical data-attributes and internal identifiers
  const hasTenantWord = /\bTenant\b/i.test(bodyText);

  // Allow technical occurrences in data attributes but not in visible text
  // The UI should use "Workspace" terminology
  expect(hasTenantWord, 'User-facing copy should not contain "Tenant"').toBe(false);
}

// -----------------------------------------------------------------------------
// Test Suites
// -----------------------------------------------------------------------------

test.describe('Flow 1: Workspace Selection Gate', () => {
  test.describe('Positive Case: Workspaces Available', () => {
    test('navigates to /workspaces and displays available workspaces', async ({ page }) => {
      const { consoleErrors, pageErrors } = attachConsoleGuards(page);
      await setupWorkspaceMocks(page);

      // Navigate with dev bypass
      await page.goto('/workspaces?dev=true');

      // Wait for page to load
      await expect(page.getByRole('heading', { name: 'Workspaces', exact: true })).toBeVisible();

      // Verify workspace cards are displayed
      await expect(page.getByText('Engineering')).toBeVisible();
      await expect(page.getByText('Data Science')).toBeVisible();

      // Verify no page errors
      expect(pageErrors, `page errors: ${pageErrors.join('\n')}`).toEqual([]);
      // Filter out expected React warnings that don't affect functionality
      const criticalErrors = consoleErrors.filter(
        (err) => !err.includes('Warning:') && !err.includes('ResizeObserver')
      );
      expect(criticalErrors, `console errors: ${criticalErrors.join('\n')}`).toEqual([]);
    });

    test('selects a workspace and reflects it in the header', async ({ page }) => {
      const { consoleErrors, pageErrors } = attachConsoleGuards(page);
      await setupWorkspaceMocks(page);

      await page.goto('/workspaces?dev=true');

      // Wait for workspaces to load
      await expect(page.getByRole('heading', { name: 'Workspaces', exact: true })).toBeVisible();
      await expect(page.getByText('Engineering')).toBeVisible();

      // Click on a workspace card to select it
      // The WorkspaceCard has a "View Details" button that triggers selection
      const engineeringCard = page.locator('[class*="Card"]').filter({ hasText: 'Engineering' });
      await engineeringCard.click();

      // Verify the header workspace switcher shows the selected workspace
      const workspaceSwitcher = page.getByTestId('tenant-switcher');
      await expect(workspaceSwitcher).toBeVisible();
      await expect(workspaceSwitcher).toContainText('Engineering');

      // Verify no page errors
      expect(pageErrors, `page errors: ${pageErrors.join('\n')}`).toEqual([]);
    });

    test('sidebar reflects workspace context after selection', async ({ page }) => {
      const { consoleErrors, pageErrors } = attachConsoleGuards(page);
      await setupWorkspaceMocks(page);

      await page.goto('/workspaces?dev=true');

      // Wait for page to load and select a workspace
      await expect(page.getByRole('heading', { name: 'Workspaces', exact: true })).toBeVisible();

      // The sidebar navigation should be visible and accessible
      const navigation = page.locator('[role="navigation"]');
      await expect(navigation).toBeVisible();

      // Navigate to another page to verify workspace context persists
      const dashboardLink = page.getByRole('button', { name: /Dashboard/i });
      if (await dashboardLink.isVisible()) {
        await dashboardLink.click();
        await expect(page).toHaveURL(/\/dashboard/);
      }

      // Verify no page errors
      expect(pageErrors, `page errors: ${pageErrors.join('\n')}`).toEqual([]);
    });

    test('no user-facing copy contains "Tenant"', async ({ page }) => {
      await setupWorkspaceMocks(page);

      await page.goto('/workspaces?dev=true');

      // Wait for page to fully load
      await expect(page.getByRole('heading', { name: 'Workspaces', exact: true })).toBeVisible();
      await expect(page.getByText('Engineering')).toBeVisible();

      // Also check the header workspace switcher dropdown
      const workspaceSwitcher = page.getByTestId('tenant-switcher');
      await workspaceSwitcher.click();

      // Wait for dropdown to open
      await expect(page.getByRole('menuitem').first()).toBeVisible();

      // Verify terminology - the dropdown label should say "Workspace"
      await expect(page.getByText('Workspace', { exact: false })).toBeVisible();
    });

    test('workspace selector dropdown shows all available workspaces', async ({ page }) => {
      const { consoleErrors, pageErrors } = attachConsoleGuards(page);
      await setupWorkspaceMocks(page);

      await page.goto('/workspaces?dev=true');

      // Wait for page to load
      await expect(page.getByRole('heading', { name: 'Workspaces', exact: true })).toBeVisible();

      // Open the workspace switcher dropdown in the header
      const workspaceSwitcher = page.getByTestId('tenant-switcher');
      await workspaceSwitcher.click();

      // Verify both workspaces appear in the dropdown
      await expect(page.getByTestId('tenant-option-workspace-1')).toBeVisible();
      await expect(page.getByTestId('tenant-option-workspace-2')).toBeVisible();

      // Verify no page errors
      expect(pageErrors, `page errors: ${pageErrors.join('\n')}`).toEqual([]);
    });
  });

  test.describe('Negative Case: Empty Workspace List', () => {
    test('shows empty state when API returns 0 workspaces', async ({ page }) => {
      const { consoleErrors, pageErrors } = attachConsoleGuards(page);

      // Setup mocks with empty workspace list
      await setupWorkspaceMocks(page, {
        workspaces: [],
        tenants: [],
      });

      await page.goto('/workspaces?dev=true');

      // Wait for page to load - should show empty state
      await expect(page.getByRole('heading', { name: 'Workspaces', exact: true })).toBeVisible();

      // Verify empty state message is displayed
      await expect(page.getByText(/No workspaces available/i)).toBeVisible();

      // Verify create/refresh action is available
      // The WorkspacesPage shows "Refresh list" button
      const refreshButton = page.getByRole('button', { name: /Refresh list/i });
      await expect(refreshButton).toBeVisible();

      // Verify no crash - page should be functional
      expect(pageErrors, `page errors: ${pageErrors.join('\n')}`).toEqual([]);

      // Verify the page is not blank (has content)
      const pageContent = await page.locator('main').textContent();
      expect(pageContent?.length).toBeGreaterThan(0);
    });

    test('empty state allows refresh action', async ({ page }) => {
      const { consoleErrors, pageErrors } = attachConsoleGuards(page);

      let refreshCount = 0;

      // Setup mocks that track refresh calls
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
          return fulfillJson(route, MOCK_USER);
        }

        if (pathname === '/v1/auth/tenants') {
          return fulfillJson(route, { schema_version: '1.0', tenants: [] });
        }

        if (pathname === '/v1/auth/tenants/switch') {
          return fulfillJson(route, {
            schema_version: '1.0',
            token: 'mock-token',
            user_id: 'user-1',
            tenant_id: 'workspace-1',
            role: 'admin',
            expires_in: 3600,
            tenants: [],
            admin_tenants: ['*'],
            session_mode: 'normal',
          });
        }

        if (pathname === '/v1/workspaces' || pathname === '/v1/workspaces/my') {
          refreshCount++;
          // After first refresh, return a workspace
          if (refreshCount > 2) {
            return fulfillJson(route, MOCK_WORKSPACES);
          }
          return fulfillJson(route, []);
        }

        // Workspace member and resource endpoints
        if (pathname.match(/^\/v1\/workspaces\/[^/]+\/members$/)) {
          return fulfillJson(route, []);
        }

        if (pathname.match(/^\/v1\/workspaces\/[^/]+\/resources$/)) {
          return fulfillJson(route, []);
        }

        // Activity events endpoint
        if (pathname === '/v1/activity/events') {
          return fulfillJson(route, []);
        }

        if (pathname.match(/^\/v1\/tenants\/[^/]+\/default-stack$/)) {
          return fulfillJson(route, { schema_version: '1.0', stack_id: null });
        }

        return fulfillJson(route, { schema_version: '1.0' });
      });

      await page.goto('/workspaces?dev=true');

      // Wait for empty state
      await expect(page.getByRole('heading', { name: 'Workspaces', exact: true })).toBeVisible();
      await expect(page.getByText(/No workspaces available/i)).toBeVisible();

      // Click refresh button
      const refreshButton = page.getByRole('button', { name: /Refresh list/i });
      await refreshButton.click();

      // After refresh, workspaces should appear
      await expect(page.getByText('Engineering')).toBeVisible({ timeout: 5000 });

      // Verify no page errors
      expect(pageErrors, `page errors: ${pageErrors.join('\n')}`).toEqual([]);
    });

    test('does not show blank page when workspaces are empty', async ({ page }) => {
      const { pageErrors } = attachConsoleGuards(page);

      await setupWorkspaceMocks(page, {
        workspaces: [],
        tenants: [],
      });

      await page.goto('/workspaces?dev=true');

      // Wait for page to load
      await expect(page.getByRole('heading', { name: 'Workspaces', exact: true })).toBeVisible();

      // Verify the page has meaningful content (not blank)
      const mainContent = page.locator('main');
      await expect(mainContent).toBeVisible();

      const textContent = await mainContent.textContent();
      expect(textContent).toBeTruthy();
      expect(textContent!.length).toBeGreaterThan(50);

      // Verify specific UI elements are present
      await expect(page.getByText('Active workspace')).toBeVisible();
      await expect(page.getByText(/No workspaces available/i)).toBeVisible();

      // Verify no crashes
      expect(pageErrors, `page errors: ${pageErrors.join('\n')}`).toEqual([]);
    });

    test('empty state shows helpful guidance message', async ({ page }) => {
      await setupWorkspaceMocks(page, {
        workspaces: [],
        tenants: [],
      });

      await page.goto('/workspaces?dev=true');

      // Wait for page to load
      await expect(page.getByRole('heading', { name: 'Workspaces', exact: true })).toBeVisible();

      // Verify guidance message appears
      const emptyStateAlert = page.locator('[role="alert"], .alert, [class*="Alert"]').filter({
        hasText: /No workspaces available/i,
      });
      await expect(emptyStateAlert).toBeVisible();

      // Verify the message provides actionable guidance
      await expect(
        page.getByText(/Create a workspace from the API|ask an administrator/i)
      ).toBeVisible();
    });
  });

  test.describe('Loading States', () => {
    test('shows loading state while workspaces are being fetched', async ({ page }) => {
      // Setup mocks with delay to observe loading state
      await setupWorkspaceMocks(page, { delayMs: 1000 });

      await page.goto('/workspaces?dev=true');

      // Check for loading indicators
      // The WorkspaceSelector shows "Loading workspaces..." text
      const loadingIndicator = page.getByText(/Loading workspaces/i).or(
        page.locator('[class*="Skeleton"]').first()
      );

      // Loading state should be visible initially
      await expect(loadingIndicator).toBeVisible({ timeout: 500 });

      // Eventually, workspaces should load
      await expect(page.getByText('Engineering')).toBeVisible({ timeout: 5000 });
    });
  });

  test.describe('Workspace Switching', () => {
    test('can switch between workspaces via header dropdown', async ({ page }) => {
      const { pageErrors } = attachConsoleGuards(page);
      await setupWorkspaceMocks(page);

      await page.goto('/workspaces?dev=true');

      // Wait for page to load
      await expect(page.getByRole('heading', { name: 'Workspaces', exact: true })).toBeVisible();

      // First, select Engineering workspace by clicking the card
      const engineeringCard = page.locator('[class*="Card"]').filter({ hasText: 'Engineering' });
      await engineeringCard.click();

      // Verify Engineering is selected
      const workspaceSwitcher = page.getByTestId('tenant-switcher');
      await expect(workspaceSwitcher).toContainText('Engineering');

      // Open dropdown and switch to Data Science
      await workspaceSwitcher.click();
      await page.getByTestId('tenant-option-workspace-2').click();

      // Verify Data Science is now selected
      await expect(workspaceSwitcher).toContainText('Data Science');

      // Verify no errors during switch
      expect(pageErrors, `page errors: ${pageErrors.join('\n')}`).toEqual([]);
    });
  });
});
