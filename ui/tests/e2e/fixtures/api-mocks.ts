/**
 * API Mock Helpers for Playwright E2E Tests
 *
 * Provides reusable mock responses for AdapterOS API endpoints.
 * All mocks return production-realistic data structures.
 *
 * Global assumptions:
 * - App served at BASE_URL (from playwright.config.ts)
 * - API at API_URL (usually same host under /api or /v1)
 * - User roles: admin/operator/sre/compliance/viewer
 * - System is offline-capable, no SaaS tier logic
 * - Workspace is UI term, backend uses tenant_id
 */

import type { Page, Route } from '@playwright/test';

// ============================================================================
// Types for Mock Data
// ============================================================================

export type UserRole = 'admin' | 'developer' | 'operator' | 'sre' | 'compliance' | 'auditor' | 'viewer';

export interface MockUserOptions {
  userId?: string;
  email?: string;
  displayName?: string;
  role?: UserRole;
  tenantId?: string;
  permissions?: string[];
  adminTenants?: string[];
  mfaEnabled?: boolean;
}

export interface MockTenantOptions {
  id?: string;
  name?: string;
  role?: UserRole;
}

export interface MockSystemStatusOptions {
  integrity?: {
    localSecureMode?: boolean;
    strictMode?: boolean;
    pfDeny?: boolean;
    drift?: { status: string; detail?: string; lastRun?: string } | null;
  };
  readiness?: {
    db?: boolean;
    migrations?: boolean;
    workers?: boolean;
    modelsSeeded?: boolean;
    phase?: string;
    bootTraceId?: string;
    degraded?: string[];
  };
  inferenceReady?: boolean;
  inferenceBlockers?: string[];
  kernel?: {
    activeModel?: string;
    activePlan?: string;
    activeAdapters?: number;
    hotAdapters?: number;
    aneMemory?: { usedMb?: number; totalMb?: number; pressure?: string };
    umaPressure?: string;
  };
  boot?: {
    phase?: string;
    degradedReasons?: string[];
    bootTraceId?: string;
    lastError?: string;
  };
}

export interface MockWorkspaceOptions {
  id?: string;
  name?: string;
  description?: string;
  memberCount?: number;
}

export interface MockModelOptions {
  id?: string;
  name?: string;
  format?: string;
  backend?: string;
  sizeBytes?: number;
  status?: 'no-model' | 'loading' | 'ready' | 'unloading' | 'error';
  isLoaded?: boolean;
}

export interface MockAdapterOptions {
  id?: string;
  name?: string;
  currentState?: 'unloaded' | 'cold' | 'warm' | 'hot' | 'resident';
  tier?: string;
  scope?: string;
  loraStrength?: number;
}

export interface ApiMockOptions {
  /** Delay in ms before responding (for loading state testing) */
  delayMs?: number;
  /** Fixed timestamp for deterministic tests */
  fixedNow?: string;
  /** User configuration */
  user?: MockUserOptions;
  /** Tenant configuration */
  tenants?: MockTenantOptions[];
  /** System status configuration */
  systemStatus?: MockSystemStatusOptions;
  /** Workspaces configuration */
  workspaces?: MockWorkspaceOptions[];
  /** Models configuration */
  models?: MockModelOptions[];
  /** Adapters configuration */
  adapters?: MockAdapterOptions[];
}

// ============================================================================
// Default Mock Data
// ============================================================================

const DEFAULT_NOW = '2025-01-01T00:00:00.000Z';

const DEFAULT_USER: Required<MockUserOptions> = {
  userId: 'user-1',
  email: 'dev@local',
  displayName: 'Dev User',
  role: 'admin',
  tenantId: 'tenant-1',
  permissions: [
    'inference:execute',
    'metrics:view',
    'training:start',
    'adapter:register',
    'testing:execute',
    'golden:view',
    'golden:create',
    'golden:compare',
  ],
  adminTenants: ['*'],
  mfaEnabled: false,
};

const DEFAULT_TENANT: Required<MockTenantOptions> = {
  id: 'tenant-1',
  name: 'System',
  role: 'admin',
};

const DEFAULT_MODEL: Required<MockModelOptions> = {
  id: 'model-1',
  name: 'Demo Model',
  format: 'gguf',
  backend: 'coreml',
  sizeBytes: 1_000_000,
  status: 'ready',
  isLoaded: true,
};

const DEFAULT_ADAPTER: Required<MockAdapterOptions> = {
  id: 'adapter-hot',
  name: 'Hot Adapter',
  currentState: 'hot',
  tier: 'prod',
  scope: 'general',
  loraStrength: 1,
};

// ============================================================================
// Mock Response Builders
// ============================================================================

/**
 * Build auth health response
 */
export function buildAuthHealthResponse() {
  return {
    status: 'healthy',
    components: {},
    schema_version: '1.0',
  };
}

/**
 * Build auth config response
 */
export function buildAuthConfigResponse(options?: {
  devBypassAllowed?: boolean;
  productionMode?: boolean;
}) {
  return {
    allow_registration: false,
    require_email_verification: false,
    access_token_ttl_minutes: 60,
    session_timeout_minutes: 480,
    max_login_attempts: 5,
    password_min_length: 8,
    mfa_required: false,
    production_mode: options?.productionMode ?? false,
    dev_token_enabled: true,
    dev_bypass_allowed: options?.devBypassAllowed ?? true,
    jwt_mode: 'hs256',
    token_expiry_hours: 24,
  };
}

/**
 * Build user info response
 */
export function buildUserInfoResponse(options: MockUserOptions = {}, now: string = DEFAULT_NOW) {
  const user = { ...DEFAULT_USER, ...options };
  return {
    schema_version: '1.0',
    user_id: user.userId,
    email: user.email,
    role: user.role,
    created_at: now,
    display_name: user.displayName,
    tenant_id: user.tenantId,
    permissions: user.permissions,
    last_login_at: now,
    mfa_enabled: user.mfaEnabled,
    token_last_rotated_at: now,
    admin_tenants: user.adminTenants,
  };
}

/**
 * Build tenant list response
 */
export function buildTenantListResponse(tenants: MockTenantOptions[] = [DEFAULT_TENANT]) {
  return {
    schema_version: '1.0',
    tenants: tenants.map((t) => ({
      id: t.id ?? DEFAULT_TENANT.id,
      name: t.name ?? DEFAULT_TENANT.name,
      role: t.role ?? DEFAULT_TENANT.role,
    })),
  };
}

/**
 * Build tenant switch response
 */
export function buildTenantSwitchResponse(
  options: MockUserOptions = {},
  tenants: MockTenantOptions[] = [DEFAULT_TENANT]
) {
  const user = { ...DEFAULT_USER, ...options };
  return {
    schema_version: '1.0',
    token: 'mock-token',
    user_id: user.userId,
    tenant_id: user.tenantId,
    role: user.role,
    expires_in: 3600,
    tenants: tenants.map((t) => ({
      id: t.id ?? DEFAULT_TENANT.id,
      name: t.name ?? DEFAULT_TENANT.name,
      role: t.role ?? DEFAULT_TENANT.role,
    })),
    admin_tenants: user.adminTenants,
    session_mode: 'normal',
  };
}

/**
 * Build system status response with all sections
 */
export function buildSystemStatusResponse(
  options: MockSystemStatusOptions = {},
  now: string = DEFAULT_NOW
) {
  return {
    schemaVersion: '1.0',
    timestamp: now,
    integrity: {
      localSecureMode: options.integrity?.localSecureMode ?? true,
      strictMode: options.integrity?.strictMode ?? true,
      pfDeny: options.integrity?.pfDeny ?? false,
      drift: options.integrity?.drift ?? { status: 'ok', detail: null, lastRun: now },
    },
    readiness: {
      db: options.readiness?.db ?? true,
      migrations: options.readiness?.migrations ?? true,
      workers: options.readiness?.workers ?? true,
      modelsSeeded: options.readiness?.modelsSeeded ?? true,
      phase: options.readiness?.phase ?? 'ready',
      bootTraceId: options.readiness?.bootTraceId ?? 'boot-trace-1',
      degraded: options.readiness?.degraded ?? null,
    },
    inferenceReady: options.inferenceReady ?? true,
    inferenceBlockers: options.inferenceBlockers ?? null,
    kernel: {
      activeModel: options.kernel?.activeModel ?? 'model-1',
      activePlan: options.kernel?.activePlan ?? 'plan-1',
      activeAdapters: options.kernel?.activeAdapters ?? 1,
      hotAdapters: options.kernel?.hotAdapters ?? 1,
      aneMemory: options.kernel?.aneMemory ?? { usedMb: 256, totalMb: 1024, pressure: 'low' },
      umaPressure: options.kernel?.umaPressure ?? 'low',
    },
    boot: {
      phase: options.boot?.phase ?? 'ready',
      degradedReasons: options.boot?.degradedReasons ?? null,
      bootTraceId: options.boot?.bootTraceId ?? 'boot-trace-1',
      lastError: options.boot?.lastError ?? null,
    },
    components: [],
  };
}

/**
 * Build workspace list response (tenant list in backend terms)
 */
export function buildWorkspaceListResponse(workspaces: MockWorkspaceOptions[] = []) {
  const defaultWorkspace: MockWorkspaceOptions = {
    id: 'workspace-1',
    name: 'Default Workspace',
    description: 'Default workspace for testing',
    memberCount: 1,
  };

  const items = workspaces.length > 0 ? workspaces : [defaultWorkspace];

  return {
    schema_version: '1.0',
    workspaces: items.map((w) => ({
      id: w.id ?? defaultWorkspace.id,
      name: w.name ?? defaultWorkspace.name,
      description: w.description ?? defaultWorkspace.description,
      member_count: w.memberCount ?? defaultWorkspace.memberCount,
    })),
    total: items.length,
  };
}

/**
 * Build models list response
 */
export function buildModelsListResponse(models: MockModelOptions[] = [DEFAULT_MODEL], now: string = DEFAULT_NOW) {
  return {
    models: models.map((m) => ({
      id: m.id ?? DEFAULT_MODEL.id,
      name: m.name ?? DEFAULT_MODEL.name,
      hash_b3: 'b3:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef',
      config_hash_b3: 'b3:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef',
      tokenizer_hash_b3: 'b3:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef',
      format: m.format ?? DEFAULT_MODEL.format,
      backend: m.backend ?? DEFAULT_MODEL.backend,
      size_bytes: m.sizeBytes ?? DEFAULT_MODEL.sizeBytes,
      adapter_count: 1,
      training_job_count: 0,
      imported_at: now,
      updated_at: now,
      architecture: { architecture: 'decoder' },
    })),
    total: models.length,
  };
}

/**
 * Build model status response
 */
export function buildModelStatusResponse(model: MockModelOptions = DEFAULT_MODEL, now: string = DEFAULT_NOW) {
  return {
    schema_version: '1.0',
    model_id: model.id ?? DEFAULT_MODEL.id,
    model_name: model.name ?? DEFAULT_MODEL.name,
    status: model.status ?? DEFAULT_MODEL.status,
    is_loaded: model.isLoaded ?? DEFAULT_MODEL.isLoaded,
    updated_at: now,
  };
}

/**
 * Build base model status response (all models)
 */
export function buildAllModelsStatusResponse(
  models: MockModelOptions[] = [DEFAULT_MODEL],
  now: string = DEFAULT_NOW
) {
  return {
    schema_version: '1.0',
    models: models.map((m) => ({
      model_id: m.id ?? DEFAULT_MODEL.id,
      model_name: m.name ?? DEFAULT_MODEL.name,
      status: m.status ?? DEFAULT_MODEL.status,
      is_loaded: m.isLoaded ?? DEFAULT_MODEL.isLoaded,
      updated_at: now,
    })),
    total_memory_mb: 0,
    active_model_count: models.filter((m) => m.isLoaded ?? true).length,
  };
}

/**
 * Build adapters list response
 */
export function buildAdaptersListResponse(
  adapters: MockAdapterOptions[] = [DEFAULT_ADAPTER],
  now: string = DEFAULT_NOW
) {
  return adapters.map((a) => ({
    id: a.id ?? DEFAULT_ADAPTER.id,
    adapter_id: a.id ?? DEFAULT_ADAPTER.id,
    name: a.name ?? DEFAULT_ADAPTER.name,
    current_state: a.currentState ?? DEFAULT_ADAPTER.currentState,
    runtime_state: a.currentState ?? DEFAULT_ADAPTER.currentState,
    created_at: now,
    updated_at: now,
    lora_tier: a.tier ?? DEFAULT_ADAPTER.tier,
    lora_scope: a.scope ?? DEFAULT_ADAPTER.scope,
    lora_strength: a.loraStrength ?? DEFAULT_ADAPTER.loraStrength,
  }));
}

/**
 * Build backends list response
 */
export function buildBackendsResponse() {
  return {
    schema_version: '1.0',
    backends: [
      { backend: 'coreml', status: 'healthy', mode: 'real' },
      { backend: 'auto', status: 'healthy', mode: 'auto' },
    ],
    default_backend: 'coreml',
  };
}

/**
 * Build backends capabilities response
 */
export function buildBackendsCapabilitiesResponse() {
  return {
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
  };
}

/**
 * Build system metrics response
 */
export function buildSystemMetricsResponse() {
  return {
    schema_version: '1.0',
    cpu_usage_percent: 1,
    memory_usage_pct: 1,
    memory_total_gb: 16,
    tokens_per_second: 0,
    latency_p95_ms: 0,
  };
}

/**
 * Build metrics snapshot response
 */
export function buildMetricsSnapshotResponse() {
  return {
    schema_version: '1.0',
    gauges: {},
    counters: {},
    metrics: {},
  };
}

// ============================================================================
// Route Handler Utilities
// ============================================================================

/**
 * Helper to fulfill a route with JSON response
 */
export function fulfillJson(route: Route, body: unknown, status = 200) {
  return route.fulfill({
    status,
    contentType: 'application/json',
    body: JSON.stringify(body),
  });
}

/**
 * Helper to add delay before responding
 */
export async function withDelay<T>(delayMs: number, fn: () => T): Promise<T> {
  if (delayMs > 0) {
    await new Promise((resolve) => setTimeout(resolve, delayMs));
  }
  return fn();
}

// ============================================================================
// Main Mock Setup Functions
// ============================================================================

/**
 * Setup all API mocks for a test page
 *
 * @example
 * ```ts
 * await setupApiMocks(page, {
 *   user: { role: 'viewer' },
 *   systemStatus: { inferenceReady: false, inferenceBlockers: ['No model loaded'] },
 * });
 * ```
 */
export async function setupApiMocks(page: Page, options: ApiMockOptions = {}) {
  const now = options.fixedNow ?? DEFAULT_NOW;
  const delayMs = options.delayMs ?? 0;

  // Health endpoints
  await page.route('**/healthz', (route) => fulfillJson(route, { status: 'healthy' }));
  await page.route('**/healthz/all', (route) => fulfillJson(route, buildAuthHealthResponse()));
  await page.route('**/readyz', (route) => fulfillJson(route, { status: 'ready' }));

  // Main API routes
  await page.route('**/v1/**', async (route) => {
    const req = route.request();
    const url = new URL(req.url());
    const { pathname } = url;
    const method = req.method();

    // Handle OPTIONS (CORS preflight)
    if (method === 'OPTIONS') {
      return route.fulfill({ status: 204 });
    }

    // Strip /api prefix if present
    const normalizedPath = pathname.startsWith('/api/') ? pathname.slice(4) : pathname;

    // Auth endpoints
    if (normalizedPath === '/v1/auth/health') {
      return fulfillJson(route, buildAuthHealthResponse());
    }

    if (normalizedPath === '/v1/auth/config') {
      return fulfillJson(route, buildAuthConfigResponse());
    }

    if (normalizedPath === '/v1/auth/me') {
      return fulfillJson(route, buildUserInfoResponse(options.user, now));
    }

    if (normalizedPath === '/v1/auth/tenants') {
      return fulfillJson(route, buildTenantListResponse(options.tenants));
    }

    if (normalizedPath === '/v1/auth/tenants/switch') {
      return fulfillJson(route, buildTenantSwitchResponse(options.user, options.tenants));
    }

    // System status endpoint
    if (normalizedPath === '/v1/system/status') {
      return withDelay(delayMs, () =>
        fulfillJson(route, buildSystemStatusResponse(options.systemStatus, now))
      );
    }

    // Workspaces (maps to tenants in backend)
    if (normalizedPath === '/v1/workspaces') {
      return fulfillJson(route, buildWorkspaceListResponse(options.workspaces));
    }

    // Tenants
    if (normalizedPath === '/v1/tenants') {
      return fulfillJson(route, buildTenantListResponse(options.tenants));
    }

    // Models endpoints
    if (normalizedPath === '/v1/models') {
      return withDelay(delayMs, () => fulfillJson(route, buildModelsListResponse(options.models, now)));
    }

    if (normalizedPath.match(/^\/v1\/models\/[^/]+\/validate$/)) {
      const modelId = normalizedPath.split('/')[3];
      return fulfillJson(route, {
        model_id: modelId,
        status: 'ready',
        valid: true,
        can_load: true,
        issues: [],
      });
    }

    if (normalizedPath.match(/^\/v1\/models\/[^/]+\/status$/)) {
      const modelId = normalizedPath.split('/')[3];
      const model = options.models?.find((m) => m.id === modelId) ?? { ...DEFAULT_MODEL, id: modelId };
      return fulfillJson(route, buildModelStatusResponse(model, now));
    }

    if (normalizedPath === '/v1/models/status') {
      const model = options.models?.[0] ?? DEFAULT_MODEL;
      return fulfillJson(route, buildModelStatusResponse(model, now));
    }

    if (normalizedPath === '/v1/models/status/all' || normalizedPath === '/v1/base-model/status') {
      return fulfillJson(route, buildAllModelsStatusResponse(options.models, now));
    }

    // Backends endpoints
    if (normalizedPath === '/v1/backends') {
      return withDelay(delayMs, () => fulfillJson(route, buildBackendsResponse()));
    }

    if (normalizedPath === '/v1/backends/capabilities') {
      return withDelay(delayMs, () => fulfillJson(route, buildBackendsCapabilitiesResponse()));
    }

    // Adapters endpoints
    if (normalizedPath === '/v1/adapters') {
      return withDelay(delayMs, () => fulfillJson(route, buildAdaptersListResponse(options.adapters, now)));
    }

    // Adapter stacks
    if (normalizedPath === '/v1/adapter-stacks') {
      return fulfillJson(route, [
        {
          id: 'stack-1',
          name: 'Demo Stack',
          adapter_ids: options.adapters?.map((a) => a.id ?? DEFAULT_ADAPTER.id) ?? [DEFAULT_ADAPTER.id],
          description: 'Demo stack',
          created_at: now,
          updated_at: now,
        },
      ]);
    }

    // Default stack for tenant
    if (normalizedPath.match(/^\/v1\/tenants\/[^/]+\/default-stack$/)) {
      return fulfillJson(route, { schema_version: '1.0', stack_id: null });
    }

    // Metrics endpoints
    if (normalizedPath === '/v1/metrics/system') {
      return withDelay(delayMs, () => fulfillJson(route, buildSystemMetricsResponse()));
    }

    if (normalizedPath === '/v1/metrics/snapshot') {
      return fulfillJson(route, buildMetricsSnapshotResponse());
    }

    if (normalizedPath === '/v1/metrics/quality') {
      return fulfillJson(route, { schema_version: '1.0' });
    }

    if (normalizedPath === '/v1/metrics/adapters') {
      return fulfillJson(route, []);
    }

    // Training endpoints
    if (normalizedPath === '/v1/training/jobs') {
      return withDelay(delayMs, () =>
        fulfillJson(route, {
          schema_version: '1.0',
          jobs: [],
          total: 0,
          page: 1,
          page_size: 20,
        })
      );
    }

    if (normalizedPath === '/v1/training/templates') {
      return fulfillJson(route, []);
    }

    // Datasets
    if (normalizedPath === '/v1/datasets') {
      return fulfillJson(route, []);
    }

    // Repos
    if (normalizedPath === '/v1/repos') {
      return fulfillJson(route, []);
    }

    // Golden runs
    if (normalizedPath === '/v1/golden/runs') {
      return fulfillJson(route, []);
    }

    // Readyz
    if (normalizedPath === '/v1/readyz') {
      return fulfillJson(route, {
        ready: true,
        checks: {
          db: { ok: true },
          worker: { ok: true },
          models_seeded: { ok: true },
        },
      });
    }

    // Default fallback: return empty object
    return fulfillJson(route, { schema_version: '1.0' });
  });
}

/**
 * Setup mock for a specific endpoint with custom response
 *
 * @example
 * ```ts
 * await mockEndpoint(page, '/v1/models', { models: [], total: 0 });
 * ```
 */
export async function mockEndpoint(
  page: Page,
  endpoint: string,
  response: unknown,
  options?: { status?: number; delayMs?: number }
) {
  await page.route(`**${endpoint}`, async (route) => {
    if (options?.delayMs) {
      await new Promise((resolve) => setTimeout(resolve, options.delayMs));
    }
    return fulfillJson(route, response, options?.status ?? 200);
  });
}

/**
 * Setup mock for an endpoint that should return an error
 *
 * @example
 * ```ts
 * await mockEndpointError(page, '/v1/models', 500, 'Internal server error');
 * ```
 */
export async function mockEndpointError(
  page: Page,
  endpoint: string,
  status: number,
  message: string
) {
  await page.route(`**${endpoint}`, (route) =>
    fulfillJson(
      route,
      {
        error: message,
        status_code: status,
      },
      status
    )
  );
}

/**
 * Setup mock for inference endpoint
 *
 * @example
 * ```ts
 * await mockInferenceEndpoint(page, {
 *   text: 'Hello world!',
 *   tokensGenerated: 5,
 *   latencyMs: 42,
 * });
 * ```
 */
export async function mockInferenceEndpoint(
  page: Page,
  options?: {
    text?: string;
    tokensGenerated?: number;
    latencyMs?: number;
    adaptersUsed?: string[];
    receiptDigest?: string;
    error?: string;
  }
) {
  const response = options?.error
    ? { error: options.error }
    : {
        schema_version: '1.0',
        id: 'resp-1',
        text: options?.text ?? 'Mock inference response',
        tokens_generated: options?.tokensGenerated ?? 10,
        token_count: options?.tokensGenerated ?? 10,
        latency_ms: options?.latencyMs ?? 50,
        adapters_used: options?.adaptersUsed ?? ['adapter-hot'],
        finish_reason: 'stop',
        backend: 'coreml',
        backend_used: 'coreml',
        run_receipt: {
          trace_id: 'trace-abc123',
          run_head_hash: 'head-hash-mock',
          output_digest: 'output-digest-mock',
          receipt_digest: options?.receiptDigest ?? 'b3-mock-receipt-digest',
        },
        trace: {
          latency_ms: options?.latencyMs ?? 50,
          adapters_used: options?.adaptersUsed ?? ['adapter-hot'],
          router_decisions: [],
          evidence_spans: [],
        },
      };

  await mockEndpoint(page, '/v1/infer', response);
}

/**
 * Setup mock for streaming inference using EventSource stub
 *
 * Must be called before page.goto()
 */
export async function installSseStub(page: Page) {
  await page.addInitScript(() => {
    class MockEventSource {
      url: string;
      withCredentials: boolean;
      readyState = 1;
      onopen: ((event: Event) => void) | null = null;
      onmessage: ((event: MessageEvent) => void) | null = null;
      onerror: ((event: Event) => void) | null = null;

      constructor(url: string, options?: EventSourceInit) {
        this.url = url;
        this.withCredentials = Boolean(options?.withCredentials);
        setTimeout(() => this.onopen?.(new Event('open')), 0);
      }

      addEventListener() {}
      removeEventListener() {}
      close() {
        this.readyState = 2;
      }
    }

    window.EventSource = MockEventSource as unknown as typeof EventSource;
  });
}

export { DEFAULT_NOW, DEFAULT_USER, DEFAULT_TENANT, DEFAULT_MODEL, DEFAULT_ADAPTER };
