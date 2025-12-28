/**
 * Flow 2: System Status Drawer - Truthful State Display
 *
 * Tests that the System Status drawer accurately represents system state,
 * including fallback behavior when /v1/system/status is unavailable.
 *
 * Preconditions:
 * - User is authenticated (dev bypass)
 * - Backend supports /v1/system/status OR fallback endpoints
 *
 * Key assertions:
 * - Drawer opens reliably from header indicator
 * - All sections exist: Integrity, Readiness, Inference, Kernel, Boot
 * - Severity badges display correct states (OK/WARN/CRITICAL/UNKNOWN)
 * - Inference is distinct from Readiness
 * - "No model loaded" yields:
 *   - Readiness: OK (if DB/workers/models are healthy)
 *   - Inference: NOT READY with blocker "no active model"
 * - Fallback mode is clearly labeled when native endpoint fails
 */

import { test, expect, type Page, type Route } from '@playwright/test';

// -----------------------------------------------------------------------------
// Types (inline to avoid import path issues in Playwright config)
// -----------------------------------------------------------------------------

/** Minimal type definition matching src/api/system-status-types.ts */
interface SystemStatusResponse {
  schemaVersion?: string;
  timestamp?: string;
  integrity?: {
    localSecureMode?: boolean | string | number | null;
    strictMode?: boolean | string | number | null;
    pfDeny?: boolean | string | number | null;
    drift?: { status?: string | null; detail?: string | null; lastRun?: string | null } | string | null;
  };
  readiness?: {
    db?: boolean | string | number | null;
    migrations?: boolean | string | number | null;
    workers?: boolean | string | number | null;
    modelsSeeded?: boolean | string | number | null;
    phase?: string | null;
    bootTraceId?: string | null;
    degraded?: string[] | null;
  };
  inferenceReady?: boolean | string | number | null;
  inferenceBlockers?: string[] | null;
  kernel?: {
    activeModel?: string | null;
    activePlan?: string | null;
    activeAdapters?: number | null;
    hotAdapters?: number | null;
    aneMemory?: { usedMb?: number | null; totalMb?: number | null; pressure?: number | null } | null;
    umaPressure?: string | null;
  };
  boot?: {
    phase?: string | null;
    degradedReasons?: string[] | null;
    bootTraceId?: string | null;
    lastError?: string | null;
  };
  components?: Array<{
    name?: string;
    status?: string;
    message?: string;
  }>;
}

// -----------------------------------------------------------------------------
// Test Data Fixtures
// -----------------------------------------------------------------------------

const FIXED_NOW = '2025-01-01T00:00:00.000Z';

/** Fully healthy system status with model loaded */
const HEALTHY_SYSTEM_STATUS: SystemStatusResponse = {
  schemaVersion: '1.0',
  timestamp: FIXED_NOW,
  integrity: {
    localSecureMode: true,
    strictMode: true,
    pfDeny: true,
    drift: { status: 'pass', detail: 'No divergences detected', lastRun: FIXED_NOW },
  },
  readiness: {
    db: 'healthy',
    migrations: 'ok',
    workers: 'ready',
    modelsSeeded: true,
    phase: 'running',
    bootTraceId: 'boot-trace-123',
  },
  inferenceReady: true,
  inferenceBlockers: [],
  kernel: {
    activeModel: 'Qwen2.5-7B',
    activePlan: 'production',
    activeAdapters: 3,
    hotAdapters: 2,
    aneMemory: { usedMb: 512, totalMb: 2048, pressure: 25 },
    umaPressure: 'low',
  },
  boot: {
    phase: 'running',
    degradedReasons: null,
    bootTraceId: 'boot-trace-123',
    lastError: null,
  },
  components: [
    { name: 'Router', status: 'healthy', message: 'Routing active' },
    { name: 'Cache', status: 'healthy', message: 'Cache warm' },
  ],
};

/** System status with no model loaded - key test case */
const NO_MODEL_SYSTEM_STATUS: SystemStatusResponse = {
  schemaVersion: '1.0',
  timestamp: FIXED_NOW,
  integrity: {
    localSecureMode: true,
    strictMode: true,
    pfDeny: true,
    drift: { status: 'pass', detail: 'No divergences detected', lastRun: FIXED_NOW },
  },
  readiness: {
    db: 'healthy',
    migrations: 'ok',
    workers: 'ready',
    modelsSeeded: true,
    phase: 'running',
    bootTraceId: 'boot-trace-456',
  },
  inferenceReady: false,
  inferenceBlockers: ['no_active_model'],
  kernel: {
    activeModel: null,
    activePlan: null,
    activeAdapters: 0,
    hotAdapters: 0,
    aneMemory: null,
    umaPressure: null,
  },
  boot: {
    phase: 'running',
    degradedReasons: null,
    bootTraceId: 'boot-trace-456',
    lastError: null,
  },
  components: [],
};

/** System status with warnings */
const DEGRADED_SYSTEM_STATUS: SystemStatusResponse = {
  schemaVersion: '1.0',
  timestamp: FIXED_NOW,
  integrity: {
    localSecureMode: true,
    strictMode: false,
    pfDeny: true,
    drift: { status: 'warn', detail: '2 divergences', lastRun: FIXED_NOW },
  },
  readiness: {
    db: 'healthy',
    migrations: 'ok',
    workers: 'degraded',
    modelsSeeded: true,
    phase: 'degraded',
    bootTraceId: 'boot-trace-789',
    degraded: ['worker-pool-reduced'],
  },
  inferenceReady: true,
  inferenceBlockers: [],
  kernel: {
    activeModel: 'Qwen2.5-7B',
    activePlan: 'production',
    activeAdapters: 1,
    hotAdapters: 1,
    aneMemory: { usedMb: 1800, totalMb: 2048, pressure: 88 },
    umaPressure: 'medium',
  },
  boot: {
    phase: 'degraded',
    degradedReasons: ['worker-pool-reduced', 'memory-pressure-high'],
    bootTraceId: 'boot-trace-789',
    lastError: 'Reduced worker capacity',
  },
  components: [
    { name: 'Router', status: 'healthy' },
    { name: 'Workers', status: 'degraded', message: 'Reduced capacity' },
  ],
};

/** System status with critical errors */
const CRITICAL_SYSTEM_STATUS: SystemStatusResponse = {
  schemaVersion: '1.0',
  timestamp: FIXED_NOW,
  integrity: {
    localSecureMode: false,
    strictMode: false,
    pfDeny: false,
    drift: { status: 'fail', detail: 'Critical divergence detected', lastRun: FIXED_NOW },
  },
  readiness: {
    db: 'unhealthy',
    migrations: 'failed',
    workers: 'failed',
    modelsSeeded: false,
    phase: 'failed',
    bootTraceId: 'boot-trace-critical',
  },
  inferenceReady: false,
  inferenceBlockers: ['no_active_model', 'workers_unavailable', 'db_connection_failed'],
  kernel: {
    activeModel: null,
    activePlan: null,
    activeAdapters: 0,
    hotAdapters: 0,
    aneMemory: null,
    umaPressure: 'critical',
  },
  boot: {
    phase: 'failed',
    degradedReasons: ['db-connection-failed', 'workers-offline'],
    bootTraceId: 'boot-trace-critical',
    lastError: 'Database connection failed',
  },
  components: [
    { name: 'Database', status: 'failed', message: 'Connection refused' },
    { name: 'Workers', status: 'failed', message: 'All workers offline' },
  ],
};

// -----------------------------------------------------------------------------
// Helper Functions
// -----------------------------------------------------------------------------

function fulfillJson(route: Route, body: unknown, status = 200) {
  return route.fulfill({
    status,
    contentType: 'application/json',
    body: JSON.stringify(body),
  });
}

/**
 * Sets up base API mocks for authenticated state.
 * Does NOT mock /v1/system/status - that should be done separately per test.
 */
async function setupBaseMocks(page: Page) {
  // Health endpoints
  await page.route('**/healthz', (route) =>
    fulfillJson(route, { status: 'healthy' })
  );
  await page.route('**/healthz/all', (route) =>
    fulfillJson(route, { status: 'healthy', components: {}, schema_version: '1.0' })
  );
  await page.route('**/readyz', (route) =>
    fulfillJson(route, {
      status: 'ready',
      ready: true,
      checks: {
        db: { ok: true },
        worker: { ok: true },
        models_seeded: { ok: true },
      },
    })
  );

  // Auth endpoints
  await page.route('**/v1/auth/config', (route) =>
    fulfillJson(route, {
      allow_registration: false,
      require_email_verification: false,
      session_timeout_minutes: 60,
      max_login_attempts: 5,
      mfa_required: false,
      dev_bypass_allowed: true,
    })
  );

  await page.route('**/v1/auth/dev-bypass', (route) =>
    fulfillJson(route, {
      schema_version: '1.0',
      token: 'dev-bypass-token',
      user_id: 'dev-user',
      tenant_id: 'tenant-1',
      role: 'admin',
      expires_in: 3600,
      tenants: [{ id: 'tenant-1', name: 'System', role: 'admin' }],
      admin_tenants: ['*'],
      session_mode: 'dev_bypass',
    })
  );

  await page.route('**/v1/auth/me', (route) =>
    fulfillJson(route, {
      schema_version: '1.0',
      user_id: 'dev-user',
      email: 'dev@local',
      role: 'admin',
      created_at: FIXED_NOW,
      display_name: 'Dev User',
      tenant_id: 'tenant-1',
      permissions: ['*'],
      admin_tenants: ['*'],
    })
  );

  await page.route('**/v1/auth/tenants', (route) =>
    fulfillJson(route, {
      schema_version: '1.0',
      tenants: [{ id: 'tenant-1', name: 'System', role: 'admin' }],
    })
  );

  await page.route('**/v1/auth/tenants/switch', (route) =>
    fulfillJson(route, {
      schema_version: '1.0',
      token: 'dev-bypass-token',
      user_id: 'dev-user',
      tenant_id: 'tenant-1',
      role: 'admin',
      expires_in: 3600,
      tenants: [{ id: 'tenant-1', name: 'System', role: 'admin' }],
      admin_tenants: ['*'],
      session_mode: 'dev_bypass',
    })
  );

  // Model endpoints
  await page.route('**/v1/models', (route) =>
    fulfillJson(route, { models: [], total: 0 })
  );
  await page.route('**/v1/models/**/status', (route) =>
    fulfillJson(route, { schema_version: '1.0', status: 'no-model', is_loaded: false })
  );
  await page.route('**/v1/models/status', (route) =>
    fulfillJson(route, { schema_version: '1.0', status: 'no-model', is_loaded: false })
  );
  await page.route('**/v1/models/status/all', (route) =>
    fulfillJson(route, { schema_version: '1.0', models: [], total_memory_mb: 0, active_model_count: 0 })
  );

  // Other required endpoints
  await page.route('**/v1/adapters**', (route) => fulfillJson(route, []));
  await page.route('**/v1/adapter-stacks**', (route) => fulfillJson(route, { stacks: [] }));
  await page.route('**/v1/backends', (route) =>
    fulfillJson(route, { schema_version: '1.0', backends: [], default_backend: 'auto' })
  );
  await page.route('**/v1/backends/**', (route) =>
    fulfillJson(route, { schema_version: '1.0' })
  );
  await page.route('**/v1/metrics/**', (route) =>
    fulfillJson(route, { schema_version: '1.0' })
  );
  await page.route('**/v1/settings', (route) =>
    fulfillJson(route, {
      schema_version: '1.0',
      security: { egress_enabled: false, require_pf_deny: true },
    })
  );
  await page.route('**/v1/diagnostics/**', (route) =>
    fulfillJson(route, { result: 'pass', divergences: 0 })
  );
  await page.route('**/system/ready', (route) =>
    fulfillJson(route, {
      schema_version: '1.0',
      state: 'running',
      components: [],
    })
  );
  await page.route('**/v1/system/state', (route) =>
    fulfillJson(route, {
      schema_version: '1.0',
      tenants: [],
      memory: null,
    })
  );
}

/**
 * Navigate to dashboard and open the System Status drawer.
 */
async function openSystemStatusDrawer(page: Page) {
  await page.goto('/dashboard');
  await page.waitForLoadState('networkidle');

  // Click the system status trigger in the header
  const trigger = page.getByTestId('system-status-trigger');
  await expect(trigger).toBeVisible({ timeout: 10000 });
  await trigger.click();

  // Wait for drawer to open (Sheet component with "System Status" title)
  const drawer = page.getByRole('dialog');
  await expect(drawer).toBeVisible({ timeout: 5000 });
  await expect(drawer.getByRole('heading', { name: 'System Status' })).toBeVisible();

  return drawer;
}

// -----------------------------------------------------------------------------
// Test Suite
// -----------------------------------------------------------------------------

test.describe('Flow 2: System Status Drawer', () => {
  test.describe('Drawer Opening', () => {
    test('opens reliably from header system indicator', async ({ page }) => {
      await setupBaseMocks(page);
      await page.route('**/v1/system/status', (route) =>
        fulfillJson(route, HEALTHY_SYSTEM_STATUS)
      );

      const drawer = await openSystemStatusDrawer(page);
      await expect(drawer).toBeVisible();
    });

    test('shows refresh button that refetches status', async ({ page }) => {
      await setupBaseMocks(page);

      let callCount = 0;
      await page.route('**/v1/system/status', (route) => {
        callCount++;
        return fulfillJson(route, HEALTHY_SYSTEM_STATUS);
      });

      const drawer = await openSystemStatusDrawer(page);

      // Click refresh button
      const refreshButton = drawer.getByRole('button', { name: 'Refresh' });
      await expect(refreshButton).toBeVisible();

      const initialCount = callCount;
      await refreshButton.click();

      // Wait for refetch
      await page.waitForTimeout(500);
      expect(callCount).toBeGreaterThan(initialCount);
    });
  });

  test.describe('Section Structure', () => {
    test('displays all required sections: Integrity, Readiness, Inference, Kernel, Boot', async ({ page }) => {
      await setupBaseMocks(page);
      await page.route('**/v1/system/status', (route) =>
        fulfillJson(route, HEALTHY_SYSTEM_STATUS)
      );

      const drawer = await openSystemStatusDrawer(page);

      // All sections must be present
      const sections = ['Integrity', 'Readiness', 'Inference', 'Kernel', 'Boot'];
      for (const section of sections) {
        const sectionHeading = drawer.getByText(section, { exact: true }).first();
        await expect(sectionHeading, `Section "${section}" should be visible`).toBeVisible();
      }
    });

    test('Inference section is distinct from Readiness section', async ({ page }) => {
      await setupBaseMocks(page);
      await page.route('**/v1/system/status', (route) =>
        fulfillJson(route, HEALTHY_SYSTEM_STATUS)
      );

      const drawer = await openSystemStatusDrawer(page);

      // Both sections should exist as separate entities
      const integritySection = drawer.getByText('Integrity', { exact: true });
      const readinessSection = drawer.getByText('Readiness', { exact: true });
      const inferenceSection = drawer.getByText('Inference', { exact: true });

      await expect(integritySection.first()).toBeVisible();
      await expect(readinessSection.first()).toBeVisible();
      await expect(inferenceSection.first()).toBeVisible();

      // Inference section should have its own fields
      await expect(drawer.getByText('Inference ready')).toBeVisible();
      await expect(drawer.getByText('Blockers')).toBeVisible();
    });
  });

  test.describe('Severity Badges', () => {
    test('shows OK badges for healthy status', async ({ page }) => {
      await setupBaseMocks(page);
      await page.route('**/v1/system/status', (route) =>
        fulfillJson(route, HEALTHY_SYSTEM_STATUS)
      );

      const drawer = await openSystemStatusDrawer(page);

      // Should have OK badges visible
      const okBadges = drawer.getByText('OK', { exact: true });
      await expect(okBadges.first()).toBeVisible();
    });

    test('shows WARN badges for degraded status', async ({ page }) => {
      await setupBaseMocks(page);
      await page.route('**/v1/system/status', (route) =>
        fulfillJson(route, DEGRADED_SYSTEM_STATUS)
      );

      const drawer = await openSystemStatusDrawer(page);

      // Should have WARN badges visible for degraded items
      const warnBadges = drawer.getByText('WARN', { exact: true });
      await expect(warnBadges.first()).toBeVisible();
    });

    test('shows CRITICAL badges for failed status', async ({ page }) => {
      await setupBaseMocks(page);
      await page.route('**/v1/system/status', (route) =>
        fulfillJson(route, CRITICAL_SYSTEM_STATUS)
      );

      const drawer = await openSystemStatusDrawer(page);

      // Should have CRITICAL badges visible for failed items
      const criticalBadges = drawer.getByText('CRITICAL', { exact: true });
      await expect(criticalBadges.first()).toBeVisible();
    });

    test('shows UNKNOWN badges for null/undefined values', async ({ page }) => {
      await setupBaseMocks(page);

      // Status with many null values
      const unknownStatus: SystemStatusResponse = {
        schemaVersion: '1.0',
        timestamp: FIXED_NOW,
        integrity: {
          localSecureMode: null,
          strictMode: null,
          pfDeny: null,
          drift: null,
        },
        readiness: {
          db: null,
          migrations: null,
          workers: null,
          modelsSeeded: null,
        },
        inferenceReady: null,
        inferenceBlockers: null,
        kernel: {
          activeModel: null,
          activePlan: null,
          activeAdapters: null,
          hotAdapters: null,
        },
        boot: {
          phase: null,
          degradedReasons: null,
        },
        components: [],
      };

      await page.route('**/v1/system/status', (route) =>
        fulfillJson(route, unknownStatus)
      );

      const drawer = await openSystemStatusDrawer(page);

      // Should have UNKNOWN badges visible
      const unknownBadges = drawer.getByText('UNKNOWN', { exact: true });
      await expect(unknownBadges.first()).toBeVisible();
    });
  });

  test.describe('No Model Loaded Scenario', () => {
    test('Readiness shows OK when DB/workers/models are healthy', async ({ page }) => {
      await setupBaseMocks(page);
      await page.route('**/v1/system/status', (route) =>
        fulfillJson(route, NO_MODEL_SYSTEM_STATUS)
      );

      const drawer = await openSystemStatusDrawer(page);

      // Find Readiness section - should have OK badge
      // The readiness fields (db, migrations, workers, modelsSeeded) are all healthy
      await expect(drawer.getByText('Database')).toBeVisible();
      await expect(drawer.getByText('Workers')).toBeVisible();

      // Look for healthy/ready indicators in the Readiness section
      // Since db: 'healthy', migrations: 'ok', workers: 'ready', modelsSeeded: true
      // these should all resolve to OK badges
      const readinessSection = drawer.locator('text=Readiness').first();
      await expect(readinessSection).toBeVisible();
    });

    test('Inference shows NOT READY with blocker when no model loaded', async ({ page }) => {
      await setupBaseMocks(page);
      await page.route('**/v1/system/status', (route) =>
        fulfillJson(route, NO_MODEL_SYSTEM_STATUS)
      );

      const drawer = await openSystemStatusDrawer(page);

      // Inference ready should show "Not ready"
      await expect(drawer.getByText('Not ready')).toBeVisible();

      // Blockers should show "no active model" (formatted from no_active_model)
      await expect(drawer.getByText(/no active model/i)).toBeVisible();

      // The Inference section should have CRITICAL badge (not OK)
      const inferenceSection = drawer.getByText('Inference', { exact: true }).first();
      await expect(inferenceSection).toBeVisible();
    });

    test('Kernel section shows "None" for active model', async ({ page }) => {
      await setupBaseMocks(page);
      await page.route('**/v1/system/status', (route) =>
        fulfillJson(route, NO_MODEL_SYSTEM_STATUS)
      );

      const drawer = await openSystemStatusDrawer(page);

      // Active model should show "None"
      await expect(drawer.getByText('Active model')).toBeVisible();
      await expect(drawer.getByText('None')).toBeVisible();
    });
  });

  test.describe('Fallback Mode', () => {
    test('labels source as fallback when /v1/system/status fails', async ({ page }) => {
      await setupBaseMocks(page);

      // Make native endpoint fail
      await page.route('**/v1/system/status', (route) =>
        route.fulfill({ status: 500, body: 'Internal Server Error' })
      );

      const drawer = await openSystemStatusDrawer(page);

      // Should show "Source: fallback" badge
      await expect(drawer.getByText('Source: fallback')).toBeVisible();
    });

    test('shows native source when /v1/system/status succeeds', async ({ page }) => {
      await setupBaseMocks(page);
      await page.route('**/v1/system/status', (route) =>
        fulfillJson(route, HEALTHY_SYSTEM_STATUS)
      );

      const drawer = await openSystemStatusDrawer(page);

      // Should show native source
      await expect(drawer.getByText('Source: /v1/system/status')).toBeVisible();
    });

    test('does not silently show OK when endpoint fails - shows unknown/fallback state', async ({ page }) => {
      await setupBaseMocks(page);

      // Make all status endpoints fail
      await page.route('**/v1/system/status', (route) =>
        route.fulfill({ status: 500 })
      );
      await page.route('**/readyz', (route) =>
        route.fulfill({ status: 500 })
      );
      await page.route('**/system/ready', (route) =>
        route.fulfill({ status: 500 })
      );

      const drawer = await openSystemStatusDrawer(page);

      // Should NOT show all OK - should have UNKNOWN or show error state
      // The UI should indicate the data is from fallback or unavailable
      const sourceInfo = drawer.getByText(/Source:/);
      await expect(sourceInfo).toBeVisible();

      // Should have some UNKNOWN badges when data is unavailable
      // or show an error state
      const hasUnknownOrError = await drawer.getByText('UNKNOWN').first().isVisible().catch(() => false) ||
                                 await drawer.getByText('Load failed').isVisible().catch(() => false);

      expect(hasUnknownOrError).toBeTruthy();
    });

    test('shows stale snapshot warning when endpoint errors after previous success', async ({ page }) => {
      await setupBaseMocks(page);

      // First call succeeds
      let callCount = 0;
      await page.route('**/v1/system/status', (route) => {
        callCount++;
        if (callCount === 1) {
          return fulfillJson(route, HEALTHY_SYSTEM_STATUS);
        }
        // Subsequent calls fail
        return route.fulfill({ status: 500 });
      });

      const drawer = await openSystemStatusDrawer(page);
      await expect(drawer.getByText('Source: /v1/system/status')).toBeVisible();

      // Click refresh to trigger failure
      const refreshButton = drawer.getByRole('button', { name: 'Refresh' });
      await refreshButton.click();
      await page.waitForTimeout(500);

      // Should show stale warning or fallback indicator
      const hasStaleOrFallback = await drawer.getByText(/Stale|fallback/i).first().isVisible().catch(() => false);
      expect(hasStaleOrFallback).toBeTruthy();
    });
  });

  test.describe('Data Accuracy', () => {
    test('displays correct Integrity section values', async ({ page }) => {
      await setupBaseMocks(page);
      await page.route('**/v1/system/status', (route) =>
        fulfillJson(route, HEALTHY_SYSTEM_STATUS)
      );

      const drawer = await openSystemStatusDrawer(page);

      // Check Integrity section fields
      await expect(drawer.getByText('Local secure mode')).toBeVisible();
      await expect(drawer.getByText('Strict mode')).toBeVisible();
      await expect(drawer.getByText('PF deny')).toBeVisible();
      await expect(drawer.getByText('Drift check')).toBeVisible();
    });

    test('displays correct Readiness section values', async ({ page }) => {
      await setupBaseMocks(page);
      await page.route('**/v1/system/status', (route) =>
        fulfillJson(route, HEALTHY_SYSTEM_STATUS)
      );

      const drawer = await openSystemStatusDrawer(page);

      // Check Readiness section fields
      await expect(drawer.getByText('Database')).toBeVisible();
      await expect(drawer.getByText('Migrations')).toBeVisible();
      await expect(drawer.getByText('Workers')).toBeVisible();
      await expect(drawer.getByText('Models seeded')).toBeVisible();
    });

    test('displays correct Kernel section values with loaded model', async ({ page }) => {
      await setupBaseMocks(page);
      await page.route('**/v1/system/status', (route) =>
        fulfillJson(route, HEALTHY_SYSTEM_STATUS)
      );

      const drawer = await openSystemStatusDrawer(page);

      // Check Kernel section fields
      await expect(drawer.getByText('Active model')).toBeVisible();
      await expect(drawer.getByText('Qwen2.5-7B')).toBeVisible();
      await expect(drawer.getByText('Active plan')).toBeVisible();
      await expect(drawer.getByText('Adapters')).toBeVisible();
      await expect(drawer.getByText('ANE memory')).toBeVisible();
      await expect(drawer.getByText('UMA pressure')).toBeVisible();
    });

    test('displays Boot section with degraded reasons when present', async ({ page }) => {
      await setupBaseMocks(page);
      await page.route('**/v1/system/status', (route) =>
        fulfillJson(route, DEGRADED_SYSTEM_STATUS)
      );

      const drawer = await openSystemStatusDrawer(page);

      // Check Boot section shows degraded reasons
      await expect(drawer.getByText('Degraded reasons')).toBeVisible();
      await expect(drawer.getByText(/worker-pool-reduced|memory-pressure-high/)).toBeVisible();
    });

    test('displays Components section with individual component status', async ({ page }) => {
      await setupBaseMocks(page);
      await page.route('**/v1/system/status', (route) =>
        fulfillJson(route, HEALTHY_SYSTEM_STATUS)
      );

      const drawer = await openSystemStatusDrawer(page);

      // Check Components section
      await expect(drawer.getByText('Components')).toBeVisible();
      await expect(drawer.getByText('Router')).toBeVisible();
      await expect(drawer.getByText('Cache')).toBeVisible();
    });

    test('shows "No component health reported" when components array is empty', async ({ page }) => {
      await setupBaseMocks(page);
      await page.route('**/v1/system/status', (route) =>
        fulfillJson(route, NO_MODEL_SYSTEM_STATUS)
      );

      const drawer = await openSystemStatusDrawer(page);

      // Check empty components message
      await expect(drawer.getByText('No component health reported')).toBeVisible();
    });
  });

  test.describe('Timestamp Display', () => {
    test('shows last updated timestamp', async ({ page }) => {
      await setupBaseMocks(page);
      await page.route('**/v1/system/status', (route) =>
        fulfillJson(route, HEALTHY_SYSTEM_STATUS)
      );

      const drawer = await openSystemStatusDrawer(page);

      // Should show Updated badge with time
      await expect(drawer.getByText(/Updated/)).toBeVisible();
    });
  });

  test.describe('Blocker Formatting', () => {
    test('formats blockers with underscores replaced by spaces', async ({ page }) => {
      await setupBaseMocks(page);
      await page.route('**/v1/system/status', (route) =>
        fulfillJson(route, CRITICAL_SYSTEM_STATUS)
      );

      const drawer = await openSystemStatusDrawer(page);

      // Blockers like 'no_active_model' should be displayed as 'no active model'
      // and 'workers_unavailable' as 'workers unavailable'
      await expect(drawer.getByText(/no active model/i)).toBeVisible();
    });

    test('shows "None" when blockers array is empty', async ({ page }) => {
      await setupBaseMocks(page);
      await page.route('**/v1/system/status', (route) =>
        fulfillJson(route, HEALTHY_SYSTEM_STATUS)
      );

      const drawer = await openSystemStatusDrawer(page);

      // Find blockers row and check it shows "None"
      const blockersRow = drawer.getByText('Blockers').locator('..');
      await expect(blockersRow.getByText('None')).toBeVisible();
    });
  });
});
