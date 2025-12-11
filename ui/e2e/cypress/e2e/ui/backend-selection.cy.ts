/// <reference types="cypress" />
import '../../support/commands';

function stubCommonEndpoints() {
  cy.intercept('GET', '**/api/v1/auth/me', {
    statusCode: 200,
    body: {
      user_id: 'user-test',
      email: 'dev@local',
      role: 'admin',
      tenant_id: 'tenant-core',
      permissions: ['inference:execute'],
      admin_tenants: ['*'],
    },
  }).as('authMe');

  cy.intercept('GET', '**/api/v1/auth/tenants', {
    statusCode: 200,
    body: {
      tenants: [
        { id: 'tenant-core', name: 'Core Tenant' },
      ],
    },
  }).as('tenants');

  cy.intercept('POST', '**/api/v1/auth/tenants/switch', {
    statusCode: 200,
    body: {
      tenant_id: 'tenant-core',
      tenants: [
        { id: 'tenant-core', name: 'Core Tenant' },
      ],
    },
  }).as('switchTenant');

  cy.intercept('GET', '**/api/v1/adapters', {
    statusCode: 200,
    body: [
      {
        id: 'adapter-1',
        adapter_id: 'adapter-1',
        name: 'Adapter One',
        hash_b3: 'hash-adapter-1',
        rank: 1,
        tier: 'persistent',
        current_state: 'hot',
        created_at: '2024-01-01T00:00:00Z',
        coreml_export_available: true,
        coreml_export_status: 'ready',
        coreml_export_verified: true,
        coreml_verification_status: 'passed',
      },
    ],
  }).as('adapters');

  cy.intercept('GET', '**/api/v1/adapter-stacks', {
    statusCode: 200,
    body: [],
  }).as('stacks');

  cy.intercept('GET', '**/api/v1/models', {
    statusCode: 200,
    body: [
      {
        id: 'model-a',
        name: 'Model A',
        hash_b3: 'hash-a',
        config_hash_b3: 'cfg-a',
        tokenizer_hash_b3: 'tok-a',
        adapter_count: 0,
        training_job_count: 0,
      },
      {
        id: 'model-b',
        name: 'Model B',
        hash_b3: 'hash-b',
        config_hash_b3: 'cfg-b',
        tokenizer_hash_b3: 'tok-b',
        adapter_count: 0,
        training_job_count: 0,
      },
    ],
  }).as('models');

  cy.intercept('GET', '**/api/v1/models/*/validate', (req) => {
    const parts = req.url.split('/');
    const modelId = parts[parts.length - 2] || 'model-a';
    req.reply({
      statusCode: 200,
      body: {
        model_id: modelId,
        valid: true,
        can_load: true,
        issues: [],
      },
    });
  }).as('modelValidate');
}

function stubBackends(options: {
  coremlAvailable: boolean;
  backendUsed?: string;
  coremlStatusOverride?: Record<string, unknown>;
  exportStatusCode?: number;
  exportBody?: Record<string, unknown>;
  verifyStatusCode?: number;
  verifyBody?: Record<string, unknown>;
}) {
  cy.intercept('GET', '**/api/v1/backends', {
    statusCode: 200,
    body: {
      schema_version: '1.0',
      backends: [
        { backend: 'coreml', status: options.coremlAvailable ? 'healthy' : 'unavailable', mode: 'real' },
        { backend: 'mlx', status: 'healthy', mode: 'real' },
        { backend: 'metal', status: 'healthy', mode: 'real' },
      ],
    },
  }).as('backendList');

  cy.intercept('GET', '**/api/v1/backends/capabilities', {
    statusCode: 200,
    body: {
      schema_version: '1.0',
      hardware: {
        ane_available: options.coremlAvailable,
        gpu_available: true,
        gpu_type: 'M-Series',
        cpu_model: 'Apple Silicon',
      },
      backends: [
        {
          backend: 'coreml',
          capabilities: [
            { name: 'coreml', available: options.coremlAvailable },
          ],
        },
        {
          backend: 'mlx',
          capabilities: [
            { name: 'mlx', available: true },
          ],
        },
        {
          backend: 'metal',
          capabilities: [
            { name: 'metal', available: true },
          ],
        },
      ],
    },
  }).as('backendCaps');

  const coremlStatusBody =
    options.coremlStatusOverride || {
      schema_version: '1.0',
      status: {
        supported: options.coremlAvailable,
        export_available: options.coremlAvailable,
        export_status: options.coremlAvailable ? 'ready' : 'unavailable',
        verification_status: options.coremlAvailable ? 'passed' : 'unsupported',
        verified: options.coremlAvailable,
        coreml_package_hash: options.coremlAvailable ? 'pkg-hash-live' : undefined,
        coreml_expected_package_hash: options.coremlAvailable ? 'pkg-hash-live' : undefined,
        coreml_hash_mismatch: false,
        notes: options.coremlAvailable ? [] : ['CoreML unavailable on host'],
      },
      message: options.coremlAvailable ? 'ready' : 'CoreML disabled',
    };

  cy.intercept('GET', '**/api/v1/adapters/*/coreml/status', {
    statusCode: 200,
    body: coremlStatusBody,
  }).as('coremlStatus');

  cy.intercept('POST', '**/api/v1/adapters/*/coreml/export', {
    statusCode: options.exportStatusCode ?? (options.coremlAvailable ? 200 : 501),
    body: options.exportBody || coremlStatusBody,
  }).as('coremlExport');

  cy.intercept('POST', '**/api/v1/adapters/*/coreml/verify', {
    statusCode: options.verifyStatusCode ?? (options.coremlAvailable ? 200 : 501),
    body: options.verifyBody || coremlStatusBody,
  }).as('coremlVerify');

  cy.intercept('POST', '**/api/v1/infer', (req) => {
    req.reply({
      statusCode: 200,
      body: {
        schema_version: '1.0',
        id: 'infer-1',
        text: `Hello from ${options.backendUsed || 'auto'}`,
        tokens_generated: 12,
        token_count: 12,
        latency_ms: 42,
        adapters_used: [],
        finish_reason: 'stop',
        backend_used: options.backendUsed || 'auto',
      },
    });
  }).as('infer');
}

const COREML_FLAG_ENABLED =
  Cypress.env('VITE_COREML_EXPORT_UI') === 'true' || Cypress.env('COREML_EXPORT_UI') === true;

describe('Backend selection & CoreML fallback', () => {
  beforeEach(() => {
    Cypress.env('AUTH_TOKEN', 'dev-token');
    stubCommonEndpoints();
  });

  const itFlagOn = COREML_FLAG_ENABLED ? it : it.skip;
  const itFlagOff = COREML_FLAG_ENABLED ? it.skip : it;

  it('selects CoreML when available and shows active tag', () => {
    stubBackends({ coremlAvailable: true, backendUsed: 'coreml' });

    cy.visit('/inference');
    cy.wait(['@models', '@backendList', '@backendCaps']);
    cy.get('[data-cy=model-selector]').click();
    cy.get('[data-cy=model-option]').first().click();

    cy.get('[data-cy=prompt-input]').clear().type('Hello CoreML backend');
    cy.get('[data-cy=run-inference-btn]').click();

    cy.wait('@infer');
    cy.get('[data-cy=active-backend-tag]').should('contain', 'CoreML');
    cy.get('[data-cy=backend-fallback-alert]').should('not.exist');
    cy.get('[data-cy=inference-output]').should('contain', 'Hello from coreml');
  });

  it('falls back gracefully when CoreML is unavailable', () => {
    stubBackends({ coremlAvailable: false, backendUsed: 'mlx' });

    cy.visit('/inference');
    cy.wait(['@models', '@backendList', '@backendCaps']);
    cy.get('[data-cy=model-selector]').click();
    cy.get('[data-cy=model-option]').first().click();

    cy.get('[data-cy=prompt-input]').click({ force: true }).clear({ force: true }).type('Fallback please', { force: true });
    cy.get('[data-cy=run-inference-btn]').click();

    cy.wait('@infer');
    cy.get('[data-cy=active-backend-tag]').should('contain', 'MLX');
    cy.get('[data-cy=backend-fallback-alert]').should('contain', 'Fell back from CoreML');
    cy.get('[data-cy=inference-output]').should('contain', 'Hello from mlx');
  });

  it('remembers backend choice per model', () => {
    stubBackends({ coremlAvailable: true, backendUsed: 'coreml' });

    cy.visit('/inference');
    cy.wait(['@models', '@backendList', '@backendCaps']);
    cy.get('[data-cy=model-selector]').click();
    cy.get('[data-cy=model-option]').first().click();
    cy.get('[data-cy=backend-selector]').click();
    cy.get('[data-cy=backend-option-coreml]').click();
    cy.get('[data-cy=active-backend-tag]').should('contain', 'CoreML');

    cy.get('[data-cy=model-selector]').click();
    cy.get('[data-cy=model-option]').eq(1).click();
    cy.get('[data-cy=backend-selector]').click();
    cy.get('[data-cy=backend-option-metal]').click();
    cy.get('[data-cy=active-backend-tag]').should('contain', 'Metal');

    cy.get('[data-cy=model-selector]').click();
    cy.get('[data-cy=model-option]').first().click();
    cy.get('[data-cy=backend-selector]').click();
    cy.get('[data-cy=backend-option-coreml]').click({ force: true });
    cy.get('[data-cy=active-backend-tag]').should(($tag) => {
      const text = $tag.text();
      expect(text).to.match(/CoreML|Metal/);
    });
  });

  itFlagOn('shows CoreML status badges when feature flag is on', () => {
    stubBackends({
      coremlAvailable: true,
      backendUsed: 'coreml',
      coremlStatusOverride: {
        schema_version: '1.0',
        status: {
          supported: true,
          export_available: true,
          export_status: 'ready',
          verification_status: 'passed',
          verified: true,
          coreml_package_hash: 'hash-actual-123',
          coreml_expected_package_hash: 'hash-expected-123',
          coreml_hash_mismatch: false,
        },
        message: 'ready',
      },
    });

    cy.visit('/inference');
    cy.wait(['@models', '@backendList', '@backendCaps', '@coremlStatus']);
    cy.get('[data-cy=model-selector]').click();
    cy.get('[data-cy=model-option]').first().click();

    cy.get('[data-cy=coreml-export-badge]').should('contain', 'Export: Ready');
    cy.get('[data-cy=coreml-verification-badge]').should('contain', 'Verification: Passed');
    cy.get('[data-cy=coreml-hash-info]')
      .should('contain', 'Expected: hash-expected-123')
      .and('contain', 'Actual: hash-actual-123');
  });

  itFlagOn('handles CoreML export and verification success and failure states', () => {
    stubBackends({ coremlAvailable: true, backendUsed: 'coreml' });

    cy.visit('/inference');
    cy.wait(['@models', '@backendList', '@backendCaps', '@coremlStatus']);
    cy.get('[data-cy=model-selector]').click();
    cy.get('[data-cy=model-option]').first().click();

    cy.intercept('POST', '**/api/v1/adapters/*/coreml/export', {
      statusCode: 200,
      body: {
        status: {
          supported: true,
          export_available: true,
          export_status: 'pending',
          verification_status: 'pending',
        },
        message: 'CoreML export started',
      },
    }).as('coremlExportSuccess');

    cy.get('[data-cy=coreml-export-trigger]').click();
    cy.wait('@coremlExportSuccess');
    cy.contains('CoreML export started');

    cy.intercept('POST', '**/api/v1/adapters/*/coreml/verify', {
      statusCode: 503,
      body: { message: 'registry unavailable' },
    }).as('coremlVerifyFailure');

    cy.get('[data-cy=coreml-verify-trigger]').click();
    cy.wait('@coremlVerifyFailure');
    cy.contains('registry unavailable');
  });

  itFlagOn('highlights CoreML verification mismatch', () => {
    stubBackends({
      coremlAvailable: true,
      backendUsed: 'coreml',
      coremlStatusOverride: {
        schema_version: '1.0',
        status: {
          supported: true,
          export_available: true,
          export_status: 'ready',
          verification_status: 'failed',
          verified: false,
          coreml_hash_mismatch: true,
          coreml_expected_package_hash: 'expected-hash-1',
          coreml_package_hash: 'actual-hash-1',
        },
        message: 'mismatch',
      },
    });

    cy.visit('/inference');
    cy.wait(['@models', '@backendList', '@backendCaps', '@coremlStatus']);

    cy.get('[data-cy=coreml-mismatch-badge]').should('exist');
    cy.get('[data-cy=coreml-mismatch-alert]').should('contain', 'hash mismatch');
    cy.get('[data-cy=coreml-hash-info]')
      .should('contain', 'Expected: expected-hash-1')
      .and('contain', 'Actual: actual-hash-1');
  });

  itFlagOff('hides CoreML export/verify controls when feature flag is off', () => {
    stubBackends({ coremlAvailable: true, backendUsed: 'coreml' });

    cy.visit('/inference');
    cy.wait(['@models', '@backendList', '@backendCaps']);
    cy.get('[data-cy=model-selector]').click();
    cy.get('[data-cy=model-option]').first().click();

    cy.get('[data-cy=coreml-disabled-note]').should('exist');
    cy.get('[data-cy=coreml-export-trigger]').should('not.exist');
    cy.get('[data-cy=coreml-verify-trigger]').should('not.exist');
    cy.get('[data-cy=coreml-export-badge]').should('not.exist');
    cy.get('[data-cy=coreml-verification-badge]').should('not.exist');
  });
});

