import { login, authenticatedRequest, getApiBaseUrl, computeRequestId, shouldRefreshToken, clearAuthToken } from './api-helpers';
import { cleanupTrackedResources, clearResourceTracking, trackResource } from './resource-cleanup';

declare global {
  namespace Cypress {
    interface Chainable {
      /**
       * Login and cache authentication token
       * Automatically refreshes token if expired or near expiry
       * @example cy.login()
       */
      login(): Chainable<string>;

      /**
       * Make an authenticated API request
       * @example cy.apiRequest({ method: 'GET', url: '/v1/adapters' })
       */
      apiRequest<T = any>(options: {
        method: string;
        url: string;
        body?: any;
        token?: string;
        failOnStatusCode?: boolean;
      }): Chainable<Cypress.Response<T>>;

      /**
       * Clear authentication token
       * @example cy.clearAuth()
       */
      clearAuth(): Chainable<void>;

      /**
       * Seed test data (placeholder for future implementation)
       * @example cy.seedTestData()
       */
      seedTestData(options?: { skipReset?: boolean; chat?: boolean }): Chainable<void>;

      /**
       * Track a created resource for cleanup
       * @example cy.trackResource('adapter', adapterId, `/v1/adapters/${adapterId}`)
       */
      trackResource(type: string, id: string, endpoint: string, method?: string): Chainable<void>;

      /**
       * Cleanup all tracked test resources
       * @example cy.cleanupTestData()
       */
      cleanupTestData(): Chainable<void>;

      /**
       * Disable CSS animations and transitions for stability
       * @example cy.disableAnimations()
       */
      disableAnimations(): Chainable<void>;

      /**
       * Set UI mode via header toggle
       * @example cy.setUiMode('user')
       */
      setUiMode(mode?: 'user' | 'builder' | 'audit'): Chainable<void>;

      /**
       * Poll a condition with a bounded timeout
       * @example cy.boundedPoll(() => cy.get('[data-cy=status]').should('contain', 'ready'))
       */
      boundedPoll<T>(
        fn: () => Cypress.Chainable<T> | T,
        options?: { timeout?: number; interval?: number; description?: string }
      ): Chainable<T>;

      /**
       * Stub core AdapterOS API routes for UI E2E flows
       * @example cy.stubApiRoutes()
       */
      stubApiRoutes(options?: {
        inferenceResponse?: Record<string, any>;
        tenants?: Array<{ id: string; name: string; schema_version?: string }>;
        user?: Record<string, any>;
        models?: Array<Record<string, any>>;
        policies?: Array<Record<string, any>>;
      }): Chainable<void>;
    }
  }
}

// Login command - authenticates and caches token with automatic refresh
Cypress.Commands.add('login', () => {
  const staticToken = Cypress.env('AUTH_TOKEN');
  if (staticToken && typeof staticToken === 'string') {
    Cypress.env('authToken', staticToken);
    return cy.wrap(staticToken);
  }

  const existingToken = Cypress.env('authToken');
  
  // Check if existing token is still valid
  if (existingToken && typeof existingToken === 'string') {
    if (!shouldRefreshToken(existingToken)) {
      // Token is still valid, return it
      return cy.wrap(existingToken);
    }
  }
  
  // Need to login (either no token or token expired)
  return login().then((token: string) => {
    Cypress.env('authToken', token);
    return token;
  });
});

// Authenticated API request wrapper
Cypress.Commands.add('apiRequest', <T = any>(options: {
  method: string;
  url: string;
  body?: any;
  token?: string;
  failOnStatusCode?: boolean;
}) => {
  const apiBase = getApiBaseUrl();
  const fullUrl = options.url.startsWith('http') ? options.url : `${apiBase}${options.url}`;
  
  return authenticatedRequest<T>({
    ...options,
    url: fullUrl,
  });
});

// Clear authentication token command
Cypress.Commands.add('clearAuth', () => {
  clearAuthToken();
  return cy.wrap(undefined);
});

// Track resource for cleanup
Cypress.Commands.add('trackResource', (type: string, id: string, endpoint: string, method: string = 'DELETE') => {
  trackResource(type, id, endpoint, method);
  return cy.wrap(undefined);
});

// Cleanup all tracked test resources
Cypress.Commands.add('cleanupTestData', () => {
  return cleanupTrackedResources();
});

// Disable CSS animations/transitions to reduce flake
Cypress.Commands.add('disableAnimations', () => {
  cy.document().then((doc) => {
    const style = doc.createElement('style');
    style.setAttribute('data-cy', 'disable-animations');
    style.innerHTML = `
      * , *::before, *::after {
        animation-duration: 0ms !important;
        transition-duration: 0ms !important;
        scroll-behavior: auto !important;
      }
    `;
    doc.head.appendChild(style);
  });
});

// UI mode toggle helper to ensure consistent navigation availability
Cypress.Commands.add('setUiMode', (mode: 'user' | 'builder' | 'audit' = 'user') => {
  cy.window({ log: false }).then((win) => {
    try {
      win.localStorage.setItem('aos_ui_mode', mode);
    } catch {
      // ignore storage failures in tests
    }
  });

  cy.get('[data-cy=ui-mode-toggle]', { timeout: 10000 }).click({ force: true });
  cy.get(`[data-cy=ui-mode-option-${mode}]`, { timeout: 10000 }).click({ force: true });
});

// Bounded polling helper for status pages
Cypress.Commands.add('boundedPoll', (fn: () => Cypress.Chainable<any> | any, options?: { timeout?: number; interval?: number; description?: string }) => {
  const timeout = options?.timeout ?? 10000;
  const interval = options?.interval ?? 500;
  const started = Date.now();

  const attempt = (): any => {
    return cy.wrap(null, { log: Boolean(options?.description) }).then(() => fn()).then((result) => result).catch((err) => {
      if (Date.now() - started >= timeout) {
        throw err;
      }
      return cy.wait(interval, { log: false }).then(attempt);
    });
  };

  return attempt();
});

// Default stubbed fixtures
const defaultTenants = [
  { schema_version: '1.0', id: 'tenant-1', name: 'Tenant One' },
  { schema_version: '1.0', id: 'tenant-2', name: 'Tenant Two' },
];

const defaultModel = {
  id: 'model-1',
  name: 'Qwen2.5-7B',
  hash_b3: 'model-hash',
  config_hash_b3: 'cfg-hash',
  tokenizer_hash_b3: 'tok-hash',
  quantization: 'q4',
  adapter_count: 1,
  training_job_count: 0,
};

const defaultPolicy = {
  cpid: 'cp-001',
  schema_hash: 'policy-hash',
  created_at: new Date().toISOString(),
  created_by: 'admin@example.com',
};

const defaultInferenceResponse = {
  schema_version: '1.0',
  id: 'run-1',
  text: 'Stubbed inference response',
  tokens_generated: 6,
  token_count: 6,
  latency_ms: 12,
  adapters_used: ['adapter-A'],
  finish_reason: 'stop',
  run_receipt: {
    trace_id: 'trace-1',
    run_head_hash: 'head-abc',
    output_digest: 'out-xyz',
    receipt_digest: 'rcpt-123',
  },
  trace: {
    latency_ms: 12,
    router_decisions: [{ adapter: 'adapter-A', score: 0.92 }],
    evidence_spans: [{ text: 'evidence snippet', relevance: 0.91 }],
  },
};

// Stub core API routes for deterministic UI flows
Cypress.Commands.add('stubApiRoutes', (options: {
  inferenceResponse?: Record<string, any>;
  tenants?: Array<{ id: string; name: string; schema_version?: string }>;
  user?: Record<string, any>;
  models?: Array<Record<string, any>>;
  policies?: Array<Record<string, any>>;
} = {}) => {
  const tenants = options.tenants ?? defaultTenants;
  const models = options.models ?? [defaultModel];
  const userInfo = options.user ?? {
    schema_version: '1.0',
    user_id: 'user-1',
    email: 'test@example.com',
    role: 'admin',
    created_at: new Date().toISOString(),
    tenant_id: tenants[0].id,
    permissions: ['policy:apply', 'policy:sign', '*'],
    admin_tenants: tenants.map((t) => t.id),
  };
  const loginResponse = {
    schema_version: '1.0',
    token: 'stub-token',
    user_id: userInfo.user_id,
    tenant_id: userInfo.tenant_id,
    role: userInfo.role,
    expires_in: 3600,
    tenants,
    admin_tenants: userInfo.admin_tenants,
  };
  const inferenceResponse = options.inferenceResponse ?? defaultInferenceResponse;
  const policies = options.policies ?? [defaultPolicy];

  cy.intercept('GET', '**/healthz', {
    status: 'healthy',
    components: {},
  }).as('healthz');

  cy.intercept('GET', '**/healthz/all', {
    status: 'healthy',
    components: {},
    timestamp: new Date().toISOString(),
  }).as('healthzAll');

  cy.intercept('GET', '**/v1/auth/config', {
    allow_registration: false,
    require_email_verification: false,
    session_timeout_minutes: 60,
    max_login_attempts: 5,
    mfa_required: false,
    dev_bypass_allowed: true,
  }).as('authConfig');

  cy.intercept('POST', '**/v1/auth/login', (req) => {
    req.reply({ statusCode: 200, body: loginResponse });
  }).as('loginRequest');

  cy.intercept('GET', '**/v1/auth/me', { body: userInfo }).as('currentUser');
  cy.intercept('GET', '**/v1/auth/tenants', { body: { schema_version: '1.0', tenants } }).as('tenantList');
  cy.intercept('POST', '**/v1/auth/refresh', {
    body: { token: loginResponse.token, expires_at: Date.now() + 3600_000 },
  }).as('refreshSession');
  cy.intercept('POST', '**/v1/auth/tenants/switch', (req) => {
    const tenantId = (req.body as { tenant_id?: string })?.tenant_id ?? tenants[0].id;
    req.reply({
      statusCode: 200,
      body: { ...loginResponse, tenant_id: tenantId },
    });
  }).as('switchTenant');

  cy.intercept('GET', '**/v1/models', {
    body: { models, total: models.length },
  }).as('models');

  cy.intercept('GET', '**/v1/models/**/status', {
    body: {
      schema_version: '1.0',
      model_id: models[0]?.id ?? 'model-1',
      status: 'ready',
      is_loaded: true,
    },
  }).as('modelStatus');

  cy.intercept('GET', '**/v1/models/**/validate', {
    body: {
      model_id: models[0]?.id ?? 'model-1',
      status: 'ready',
      valid: true,
      can_load: true,
      issues: [],
    },
  }).as('modelValidate');

  cy.intercept('POST', '**/v1/models/**/load', {
    body: {
      schema_version: '1.0',
      model_id: models[0]?.id ?? 'model-1',
      status: 'ready',
      is_loaded: true,
    },
  }).as('modelLoad');

  cy.intercept('GET', '**/v1/adapters**', { body: [] }).as('adapters');
  cy.intercept('GET', '**/v1/adapter-stacks**', { body: { stacks: [] } }).as('adapterStacks');
  cy.intercept('GET', '**/v1/backends/**', { body: { backends: [] } }).as('backends');
  cy.intercept('GET', '**/v1/routing/**', { body: { schema_version: '1.0', routes: [] } }).as('routing');

  cy.intercept('POST', '**/v1/infer', (req) => {
    req.reply({ statusCode: 200, body: inferenceResponse });
  }).as('inferRequest');

  cy.intercept('GET', '**/v1/policies', { body: policies }).as('policies');
  cy.intercept('POST', '**/v1/policies/validate', { body: { valid: true, errors: [] } }).as('policyValidate');
  cy.intercept('POST', '**/v1/policies', { statusCode: 200, body: { status: 'ok' } }).as('policySave');

  cy.intercept('GET', '**/v1/audit/**', { body: {} }).as('audit');
  cy.intercept('GET', '**/v1/telemetry/**', { body: {} }).as('telemetry');

  return cy.wrap(undefined);
});

// Deterministic seed helper (uses aosctl db seed-fixtures via cypress task)
Cypress.Commands.add('seedTestData', (options: { skipReset?: boolean; chat?: boolean } = {}) => {
  return cy.task('db:seed-fixtures', options);
});
