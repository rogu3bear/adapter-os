import { test, expect, Page } from '@playwright/test';

async function setupTestingMocks(page: Page, options?: { adaptersDelayMs?: number }) {
  const now = new Date().toISOString();
  const adaptersDelayMs = options?.adaptersDelayMs ?? 600;

  await page.route('**/v1/**', async (route) => {
    const url = new URL(route.request().url());
    const { pathname } = url;
    const method = route.request().method();

    const json = (body: unknown, status = 200) =>
      route.fulfill({
        status,
        contentType: 'application/json',
        body: JSON.stringify(body),
      });

    if (method === 'OPTIONS') {
      return route.fulfill({ status: 204 });
    }

    if (pathname === '/v1/auth/me') {
      return json({
        schema_version: '1.0',
        user_id: 'user-1',
        email: 'dev@local',
        role: 'admin',
        created_at: now,
        display_name: 'Dev User',
        tenant_id: 'tenant-1',
        permissions: [
          'testing:execute',
          'golden:view',
          'golden:create',
          'golden:compare',
        ],
        last_login_at: now,
        mfa_enabled: false,
        token_last_rotated_at: now,
        admin_tenants: ['*'],
      });
    }

    if (pathname === '/v1/auth/tenants') {
      return json({
        schema_version: '1.0',
        tenants: [{ id: 'tenant-1', name: 'System', role: 'admin' }],
      });
    }

    if (pathname === '/v1/auth/tenants/switch') {
      return json({
        schema_version: '1.0',
        token: 'mock-token',
        user_id: 'user-1',
        tenant_id: 'tenant-1',
        role: 'admin',
        expires_in: 3600,
        tenants: [{ id: 'tenant-1', name: 'System', role: 'admin' }],
      });
    }

    if (pathname === '/v1/adapters') {
      await new Promise((resolve) => setTimeout(resolve, adaptersDelayMs));
      return json([
        {
          id: 'adapter-1',
          name: 'Test Adapter',
          active: true,
          current_state: 'hot',
          created_at: now,
        },
      ]);
    }

    if (pathname === '/v1/golden/runs') {
      return json([]);
    }

    if (pathname === '/v1/readyz') {
      return json({ status: 'healthy' });
    }

    return json({});
  });
}

test.describe('Testing page loading state', () => {
  test('shows loading marker then adapter row', async ({ page }) => {
    await setupTestingMocks(page, { adaptersDelayMs: 800 });

    await page.goto('/testing');

    const loadingMarker = page.locator('[data-testid="loading-state"]').first();
    await expect(loadingMarker).toBeVisible();
    await expect(loadingMarker).toBeHidden();

    await expect(page.getByRole('cell', { name: 'Test Adapter' })).toBeVisible();
  });
});

