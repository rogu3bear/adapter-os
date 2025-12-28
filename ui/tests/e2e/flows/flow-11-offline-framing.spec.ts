/**
 * Flow 11: Offline Framing Check (No SaaS Language)
 *
 * Content compliance test to ensure the UI uses appropriate
 * "licensed local deployment" / "air-gapped/offline" terminology
 * and avoids SaaS-related language.
 *
 * Preconditions:
 * - UI accessible
 *
 * Steps:
 * 1. Visit key pages: Home, Workspaces, Chat, System, Admin/Settings
 * 2. Get visible text content from each page
 * 3. Check for forbidden SaaS-related terms
 * 4. Optionally verify positive offline terminology
 *
 * Expected outcomes:
 * - No SaaS, trial, subscription, billing, tier, cloud pricing language
 * - Appropriate offline/local deployment language where applicable
 */
import { test, expect, type Page, type Route } from '@playwright/test';

// ============================================================================
// Configuration
// ============================================================================

/**
 * Pages to scan for content compliance.
 * Each entry includes the route path and a human-readable description.
 */
const PAGES_TO_CHECK = [
  { path: '/', name: 'Root/Redirect' },
  { path: '/dashboard', name: 'Dashboard (Home)' },
  { path: '/workspaces', name: 'Workspaces' },
  { path: '/chat', name: 'Chat' },
  { path: '/system', name: 'System Overview' },
  { path: '/admin', name: 'Admin' },
  { path: '/admin/settings', name: 'Admin Settings' },
  { path: '/base-models', name: 'Base Models' },
  { path: '/training', name: 'Training' },
  { path: '/metrics', name: 'Metrics' },
  { path: '/security/policies', name: 'Security Policies (Guardrails)' },
] as const;

/**
 * Forbidden terms that indicate SaaS/cloud service language.
 * These should NOT appear in user-facing UI text.
 *
 * Terms are case-insensitive during matching.
 * Each term includes a regex pattern and description for reporting.
 */
const FORBIDDEN_TERMS = [
  // Subscription/billing language
  { pattern: /\bSaaS\b/i, term: 'SaaS', category: 'cloud-model' },
  { pattern: /\btrial\b/i, term: 'trial', category: 'subscription' },
  { pattern: /\bsubscription\b/i, term: 'subscription', category: 'billing' },
  { pattern: /\bbilling\b/i, term: 'billing', category: 'billing' },
  { pattern: /\btier\b/i, term: 'tier', category: 'pricing' },
  { pattern: /\bpricing\s+tier/i, term: 'pricing tier', category: 'pricing' },
  { pattern: /\bcloud\s+pricing/i, term: 'cloud pricing', category: 'pricing' },
  { pattern: /\bpay[- ]?per[- ]?use/i, term: 'pay-per-use', category: 'billing' },
  { pattern: /\bmonthly\s+fee/i, term: 'monthly fee', category: 'billing' },
  { pattern: /\bannual\s+subscription/i, term: 'annual subscription', category: 'billing' },

  // Cloud service language
  { pattern: /\bcloud[- ]?hosted/i, term: 'cloud-hosted', category: 'cloud-model' },
  { pattern: /\bhosted\s+service/i, term: 'hosted service', category: 'cloud-model' },
  { pattern: /\bmulti[- ]?tenant\s+cloud/i, term: 'multi-tenant cloud', category: 'cloud-model' },

  // Account/upgrade language (context-sensitive - may need refinement)
  { pattern: /\bupgrade\s+your\s+plan/i, term: 'upgrade your plan', category: 'upsell' },
  { pattern: /\bpremium\s+feature/i, term: 'premium feature', category: 'upsell' },
  { pattern: /\bunlock\s+feature/i, term: 'unlock feature', category: 'upsell' },
  { pattern: /\bfree\s+trial/i, term: 'free trial', category: 'subscription' },
  { pattern: /\bpro\s+plan/i, term: 'pro plan', category: 'subscription' },
  { pattern: /\benterprise\s+plan/i, term: 'enterprise plan', category: 'subscription' },

  // Usage limits language (typical of cloud services)
  { pattern: /\brate\s+limit\s+exceeded/i, term: 'rate limit exceeded', category: 'cloud-limits' },
  { pattern: /\bquota\s+exceeded/i, term: 'quota exceeded', category: 'cloud-limits' },
  { pattern: /\busage\s+limit\s+reached/i, term: 'usage limit reached', category: 'cloud-limits' },
] as const;

/**
 * Positive terms that indicate appropriate offline/local deployment language.
 * These are OPTIONAL - their presence is a good signal but not required.
 */
const POSITIVE_TERMS = [
  { pattern: /\blocal\s+deployment/i, term: 'local deployment' },
  { pattern: /\bair[- ]?gapped/i, term: 'air-gapped' },
  { pattern: /\boffline/i, term: 'offline' },
  { pattern: /\bon[- ]?premise/i, term: 'on-premise' },
  { pattern: /\bself[- ]?hosted/i, term: 'self-hosted' },
  { pattern: /\bprivate\s+deployment/i, term: 'private deployment' },
  { pattern: /\blocal\s+instance/i, term: 'local instance' },
  { pattern: /\blicensed/i, term: 'licensed' },
] as const;

/**
 * Element selectors to exclude from content scanning.
 * These contain technical content that may legitimately use terms
 * that would otherwise be flagged (e.g., code examples, API docs).
 */
const EXCLUDED_SELECTORS = [
  'code',
  'pre',
  '[data-testid="code-block"]',
  '[data-cy="code-block"]',
  '.monaco-editor',
  '[role="log"]',
  '.terminal',
] as const;

// ============================================================================
// Test Utilities
// ============================================================================

interface TermMatch {
  term: string;
  category: string;
  context: string;
  lineNumber?: number;
}

interface PageScanResult {
  path: string;
  name: string;
  forbiddenMatches: TermMatch[];
  positiveMatches: string[];
  textLength: number;
  scanDuration: number;
}

/**
 * Extracts visible text content from a page, excluding technical elements.
 */
async function getVisibleTextContent(page: Page): Promise<string> {
  const excludeSelector = EXCLUDED_SELECTORS.join(', ');

  return page.evaluate((excludeSel) => {
    // Clone the body to avoid modifying the actual DOM
    const clone = document.body.cloneNode(true) as HTMLElement;

    // Remove excluded elements
    const excluded = clone.querySelectorAll(excludeSel);
    excluded.forEach((el) => el.remove());

    // Get text content, normalizing whitespace
    const text = clone.innerText || clone.textContent || '';
    return text.replace(/\s+/g, ' ').trim();
  }, excludeSelector);
}

/**
 * Finds all matches of forbidden terms in text content.
 */
function findForbiddenTerms(text: string): TermMatch[] {
  const matches: TermMatch[] = [];

  for (const term of FORBIDDEN_TERMS) {
    let match: RegExpExecArray | null;
    const regex = new RegExp(term.pattern.source, term.pattern.flags + 'g');

    while ((match = regex.exec(text)) !== null) {
      // Find the line containing this match for context
      const beforeMatch = text.substring(0, match.index);
      const lineNumber = beforeMatch.split(/[.!?\n]/).length;

      // Extract context (surrounding text)
      const contextStart = Math.max(0, match.index - 50);
      const contextEnd = Math.min(text.length, match.index + match[0].length + 50);
      let context = text.substring(contextStart, contextEnd);

      // Trim to word boundaries
      if (contextStart > 0) {
        context = '...' + context.replace(/^\S*\s/, '');
      }
      if (contextEnd < text.length) {
        context = context.replace(/\s\S*$/, '') + '...';
      }

      matches.push({
        term: term.term,
        category: term.category,
        context: context.trim(),
        lineNumber,
      });
    }
  }

  return matches;
}

/**
 * Finds all matches of positive (offline/local) terms in text content.
 */
function findPositiveTerms(text: string): string[] {
  const matches: string[] = [];

  for (const term of POSITIVE_TERMS) {
    if (term.pattern.test(text)) {
      matches.push(term.term);
    }
  }

  return Array.from(new Set(matches)); // Deduplicate
}

/**
 * Sets up API mocks for pages to load without a real backend.
 */
async function setupMocks(page: Page) {
  const now = new Date().toISOString();

  const fulfillJson = (route: Route, body: unknown, status = 200) =>
    route.fulfill({
      status,
      contentType: 'application/json',
      body: JSON.stringify(body),
    });

  // Health endpoints
  await page.route('**/healthz', async (route) => fulfillJson(route, { status: 'healthy' }));
  await page.route('**/healthz/all', async (route) =>
    fulfillJson(route, { status: 'healthy', components: {}, schema_version: '1.0' })
  );
  await page.route('**/readyz', async (route) => fulfillJson(route, { status: 'ready' }));

  // API endpoints
  await page.route('**/v1/**', async (route) => {
    const req = route.request();
    const url = new URL(req.url());
    const { pathname } = url;
    const method = req.method();

    if (method === 'OPTIONS') {
      return route.fulfill({ status: 204 });
    }

    // Auth endpoints
    if (pathname === '/v1/auth/me' || pathname.endsWith('/auth/me')) {
      return fulfillJson(route, {
        schema_version: '1.0',
        user_id: 'user-1',
        email: 'admin@local.dev',
        role: 'admin',
        created_at: now,
        display_name: 'Admin User',
        tenant_id: 'tenant-1',
        permissions: [
          'inference:execute',
          'metrics:view',
          'training:start',
          'adapter:register',
          'audit:view',
        ],
        last_login_at: now,
        mfa_enabled: false,
        token_last_rotated_at: now,
        admin_tenants: ['*'],
      });
    }

    if (pathname === '/v1/auth/tenants' || pathname.endsWith('/auth/tenants')) {
      return fulfillJson(route, {
        schema_version: '1.0',
        tenants: [{ id: 'tenant-1', name: 'Local Workspace', role: 'admin' }],
      });
    }

    if (pathname.includes('/auth/tenants/switch')) {
      return fulfillJson(route, {
        schema_version: '1.0',
        token: 'mock-token',
        user_id: 'user-1',
        tenant_id: 'tenant-1',
        role: 'admin',
        expires_in: 3600,
        tenants: [{ id: 'tenant-1', name: 'Local Workspace', role: 'admin' }],
        admin_tenants: ['*'],
        session_mode: 'normal',
      });
    }

    // Models
    if (pathname === '/v1/models' || pathname.endsWith('/models')) {
      return fulfillJson(route, {
        models: [
          {
            id: 'model-1',
            name: 'Local Model',
            hash_b3: 'b3:mock-hash',
            config_hash_b3: 'b3:mock-config',
            tokenizer_hash_b3: 'b3:mock-tokenizer',
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

    if (pathname.includes('/models/status')) {
      return fulfillJson(route, {
        schema_version: '1.0',
        models: [
          {
            model_id: 'model-1',
            model_name: 'Local Model',
            status: 'ready',
            is_loaded: true,
            updated_at: now,
          },
        ],
        total_memory_mb: 0,
        active_model_count: 1,
      });
    }

    // Backends
    if (pathname === '/v1/backends' || pathname.endsWith('/backends')) {
      return fulfillJson(route, {
        schema_version: '1.0',
        backends: [
          { backend: 'coreml', status: 'healthy', mode: 'real' },
          { backend: 'auto', status: 'healthy', mode: 'auto' },
        ],
        default_backend: 'coreml',
      });
    }

    if (pathname.includes('/backends/capabilities')) {
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

    // Adapters
    if (pathname === '/v1/adapters' || pathname.endsWith('/adapters')) {
      return fulfillJson(route, [
        {
          id: 'adapter-1',
          adapter_id: 'adapter-1',
          name: 'Local Adapter',
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

    if (pathname === '/v1/adapter-stacks' || pathname.endsWith('/adapter-stacks')) {
      return fulfillJson(route, [
        {
          id: 'stack-1',
          name: 'Default Stack',
          adapter_ids: ['adapter-1'],
          description: 'Default adapter stack',
          created_at: now,
          updated_at: now,
        },
      ]);
    }

    // Metrics
    if (pathname.includes('/metrics/system')) {
      return fulfillJson(route, {
        schema_version: '1.0',
        cpu_usage_percent: 15,
        memory_usage_pct: 45,
        memory_total_gb: 16,
        tokens_per_second: 25,
        latency_p95_ms: 120,
      });
    }

    if (pathname.includes('/metrics/snapshot')) {
      return fulfillJson(route, {
        schema_version: '1.0',
        gauges: {},
        counters: {},
        metrics: {},
      });
    }

    if (pathname.includes('/metrics/quality')) {
      return fulfillJson(route, { schema_version: '1.0' });
    }

    if (pathname.includes('/metrics/adapters')) {
      return fulfillJson(route, []);
    }

    // Training
    if (pathname.includes('/training/jobs')) {
      return fulfillJson(route, {
        schema_version: '1.0',
        jobs: [],
        total: 0,
        page: 1,
        page_size: 20,
      });
    }

    if (pathname.includes('/training/templates')) {
      return fulfillJson(route, []);
    }

    // Datasets
    if (pathname === '/v1/datasets' || pathname.endsWith('/datasets')) {
      return fulfillJson(route, []);
    }

    // Repos
    if (pathname === '/v1/repos' || pathname.endsWith('/repos')) {
      return fulfillJson(route, []);
    }

    // Policies
    if (pathname.includes('/policies')) {
      return fulfillJson(route, {
        schema_version: '1.0',
        policies: [],
        total: 0,
      });
    }

    // Tenants
    if (pathname.includes('/tenants/tenant-1/default-stack')) {
      return fulfillJson(route, { schema_version: '1.0', stack_id: null });
    }

    // System info
    if (pathname.includes('/system/info')) {
      return fulfillJson(route, {
        schema_version: '1.0',
        version: '1.0.0',
        deployment_mode: 'local',
        license_type: 'enterprise',
      });
    }

    // Chat sessions
    if (pathname.includes('/chat/sessions')) {
      return fulfillJson(route, {
        schema_version: '1.0',
        sessions: [],
        total: 0,
      });
    }

    // Default response
    return fulfillJson(route, { schema_version: '1.0' });
  });
}

/**
 * Scans a single page for content compliance.
 */
async function scanPage(
  page: Page,
  path: string,
  name: string
): Promise<PageScanResult> {
  const startTime = Date.now();

  await page.goto(path, { waitUntil: 'networkidle' });

  // Wait for any dynamic content to load
  await page.waitForTimeout(1000);

  const textContent = await getVisibleTextContent(page);
  const forbiddenMatches = findForbiddenTerms(textContent);
  const positiveMatches = findPositiveTerms(textContent);

  return {
    path,
    name,
    forbiddenMatches,
    positiveMatches,
    textLength: textContent.length,
    scanDuration: Date.now() - startTime,
  };
}

/**
 * Formats scan results for test output.
 */
function formatScanResults(results: PageScanResult[]): string {
  const lines: string[] = [];

  lines.push('='.repeat(80));
  lines.push('OFFLINE FRAMING COMPLIANCE SCAN RESULTS');
  lines.push('='.repeat(80));
  lines.push('');

  const totalForbidden = results.reduce((sum, r) => sum + r.forbiddenMatches.length, 0);
  const pagesWithViolations = results.filter((r) => r.forbiddenMatches.length > 0);

  lines.push(`Pages scanned: ${results.length}`);
  lines.push(`Total forbidden term matches: ${totalForbidden}`);
  lines.push(`Pages with violations: ${pagesWithViolations.length}`);
  lines.push('');

  for (const result of results) {
    lines.push('-'.repeat(80));
    lines.push(`Page: ${result.name} (${result.path})`);
    lines.push(`  Text length: ${result.textLength} chars`);
    lines.push(`  Scan time: ${result.scanDuration}ms`);

    if (result.forbiddenMatches.length > 0) {
      lines.push(`  VIOLATIONS (${result.forbiddenMatches.length}):`);
      for (const match of result.forbiddenMatches) {
        lines.push(`    - "${match.term}" [${match.category}]`);
        lines.push(`      Context: "${match.context}"`);
      }
    } else {
      lines.push('  No violations found');
    }

    if (result.positiveMatches.length > 0) {
      lines.push(`  Positive terms found: ${result.positiveMatches.join(', ')}`);
    }

    lines.push('');
  }

  return lines.join('\n');
}

// ============================================================================
// Tests
// ============================================================================

test.describe('Flow 11: Offline Framing Check', () => {
  test.beforeEach(async ({ page }) => {
    await setupMocks(page);
  });

  test('should not contain SaaS-related terminology on key pages', async ({ page }) => {
    const results: PageScanResult[] = [];

    for (const pageConfig of PAGES_TO_CHECK) {
      const result = await scanPage(page, pageConfig.path, pageConfig.name);
      results.push(result);
    }

    // Output detailed results for debugging
    console.log(formatScanResults(results));

    // Aggregate all violations
    const allViolations = results.flatMap((r) =>
      r.forbiddenMatches.map((m) => ({
        page: r.name,
        path: r.path,
        ...m,
      }))
    );

    // Build assertion message
    if (allViolations.length > 0) {
      const violationSummary = allViolations
        .map(
          (v) =>
            `  - "${v.term}" on ${v.page} (${v.path})\n    Category: ${v.category}\n    Context: "${v.context}"`
        )
        .join('\n\n');

      expect.soft(
        allViolations,
        `Found ${allViolations.length} SaaS-related term(s) in UI:\n\n${violationSummary}`
      ).toHaveLength(0);
    }

    // Hard assertion
    expect(allViolations).toHaveLength(0);
  });

  test('individual page scans for detailed reporting', async ({ page }) => {
    // Test each page individually for better error isolation
    for (const pageConfig of PAGES_TO_CHECK) {
      await test.step(`Scanning ${pageConfig.name}`, async () => {
        const result = await scanPage(page, pageConfig.path, pageConfig.name);

        if (result.forbiddenMatches.length > 0) {
          const violations = result.forbiddenMatches
            .map((m) => `"${m.term}" [${m.category}]: ${m.context}`)
            .join('\n');

          expect.soft(
            result.forbiddenMatches,
            `${pageConfig.name} contains forbidden terms:\n${violations}`
          ).toHaveLength(0);
        }
      });
    }
  });

  test('should use appropriate offline/local deployment terminology', async ({ page }) => {
    const results: PageScanResult[] = [];

    for (const pageConfig of PAGES_TO_CHECK) {
      const result = await scanPage(page, pageConfig.path, pageConfig.name);
      results.push(result);
    }

    // Collect all positive terms found across pages
    const allPositiveTerms = Array.from(new Set(results.flatMap((r) => r.positiveMatches)));

    console.log('\nPositive offline/local terminology found:');
    if (allPositiveTerms.length > 0) {
      console.log(`  ${allPositiveTerms.join(', ')}`);
    } else {
      console.log('  (none)');
    }

    // This is informational - we don't fail if no positive terms are found
    // since not all pages need to explicitly mention deployment type
    console.log(
      '\nNote: Positive terminology check is informational. ' +
        'Not all pages require explicit offline/local language.'
    );
  });

  test('System Status drawer should not contain SaaS language', async ({ page }) => {
    // Navigate to a page with the system status drawer
    await page.goto('/dashboard', { waitUntil: 'networkidle' });
    await page.waitForTimeout(500);

    // Look for system status trigger (could be a button, icon, or header element)
    const statusTriggers = [
      '[data-cy="system-status-trigger"]',
      '[data-testid="system-status-trigger"]',
      '[aria-label*="system status"]',
      '[aria-label*="System Status"]',
      'button:has-text("System Status")',
      '[data-cy="status-drawer-trigger"]',
    ];

    let drawerOpened = false;

    for (const selector of statusTriggers) {
      const trigger = page.locator(selector).first();
      if ((await trigger.count()) > 0) {
        await trigger.click();
        drawerOpened = true;
        await page.waitForTimeout(500);
        break;
      }
    }

    // If drawer opened, scan its content
    if (drawerOpened) {
      const drawerSelectors = [
        '[role="dialog"]',
        '[data-cy="system-status-drawer"]',
        '[data-testid="status-drawer"]',
        '.drawer',
        '[class*="drawer"]',
      ];

      let drawerText = '';

      for (const selector of drawerSelectors) {
        const drawer = page.locator(selector).first();
        if ((await drawer.count()) > 0) {
          drawerText = (await drawer.textContent()) || '';
          break;
        }
      }

      if (drawerText) {
        const violations = findForbiddenTerms(drawerText);

        if (violations.length > 0) {
          const summary = violations
            .map((v) => `"${v.term}" [${v.category}]: ${v.context}`)
            .join('\n');

          expect.soft(
            violations,
            `System Status drawer contains forbidden terms:\n${summary}`
          ).toHaveLength(0);
        }

        expect(violations).toHaveLength(0);
      }
    } else {
      // If no drawer trigger found, this is informational
      console.log('Note: System Status drawer trigger not found. Skipping drawer scan.');
    }
  });

  test('forbidden term patterns are correctly defined', async () => {
    // Validation test to ensure our patterns work correctly

    // Test that patterns match expected strings
    const testCases = [
      { input: 'This is a SaaS product', shouldMatch: 'SaaS' },
      { input: 'Start your free trial today', shouldMatch: 'trial' },
      { input: 'Manage your subscription', shouldMatch: 'subscription' },
      { input: 'View billing details', shouldMatch: 'billing' },
      { input: 'Upgrade to premium tier', shouldMatch: 'tier' },
      { input: 'Cloud pricing available', shouldMatch: 'cloud pricing' },
      { input: 'Pay-per-use model', shouldMatch: 'pay-per-use' },
    ];

    for (const tc of testCases) {
      const matches = findForbiddenTerms(tc.input);
      expect(
        matches.some((m) => m.term.toLowerCase() === tc.shouldMatch.toLowerCase()),
        `Pattern should match "${tc.shouldMatch}" in "${tc.input}"`
      ).toBe(true);
    }

    // Test that patterns don't match inappropriate strings
    const negativeTestCases = [
      'Local deployment ready',
      'Air-gapped installation',
      'Self-hosted instance',
      'On-premise setup complete',
      'Licensed for offline use',
    ];

    for (const input of negativeTestCases) {
      const matches = findForbiddenTerms(input);
      expect(
        matches,
        `"${input}" should not match any forbidden patterns`
      ).toHaveLength(0);
    }
  });

  test('positive term patterns detect offline terminology', async () => {
    // Validation test for positive term detection

    const testCases = [
      { input: 'Local deployment supported', shouldMatch: 'local deployment' },
      { input: 'Works in air-gapped environments', shouldMatch: 'air-gapped' },
      { input: 'Offline mode available', shouldMatch: 'offline' },
      { input: 'On-premise installation', shouldMatch: 'on-premise' },
      { input: 'Self-hosted solution', shouldMatch: 'self-hosted' },
    ];

    for (const tc of testCases) {
      const matches = findPositiveTerms(tc.input);
      expect(
        matches.some((m) => m.toLowerCase() === tc.shouldMatch.toLowerCase()),
        `Should detect "${tc.shouldMatch}" in "${tc.input}"`
      ).toBe(true);
    }
  });
});

// ============================================================================
// Export utilities for use in other tests
// ============================================================================

export {
  FORBIDDEN_TERMS,
  POSITIVE_TERMS,
  findForbiddenTerms,
  findPositiveTerms,
  getVisibleTextContent,
  type TermMatch,
  type PageScanResult,
};
