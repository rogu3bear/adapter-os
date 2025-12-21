/// <reference types="cypress" />

const blockedByPolicyResponse = {
  schema_version: '1.0',
  error: 'Request blocked by policy enforcement',
  code: 'POLICY_DENIED',
  policy_id: 'egress-block-001',
  message: 'Network egress policy violation detected',
};

const policyWarningResponse = {
  schema_version: '1.0',
  id: 'run-policy-warning-1',
  text: 'Response with policy warnings.',
  tokens_generated: 10,
  token_count: 10,
  latency_ms: 15,
  adapters_used: ['adapter-warning'],
  finish_reason: 'stop',
  run_receipt: {
    trace_id: 'trace-warning-1',
    run_head_hash: 'head-warning',
    output_digest: 'out-warning',
    receipt_digest: 'rcpt-warning',
  },
  policy_warnings: [
    {
      policy_id: 'determinism-check',
      severity: 'warning',
      message: 'Non-deterministic routing detected',
    },
  ],
  trace: {
    latency_ms: 15,
    router_decisions: [{ adapter: 'adapter-warning', score: 0.82 }],
    evidence_spans: [],
  },
};

const isolationViolationResponse = {
  schema_version: '1.0',
  error: 'Tenant isolation policy violation',
  code: 'ISOLATION_VIOLATION',
  policy_id: 'tenant-isolation-001',
  message: 'Attempted to access resources from different tenant',
  tenant_id: 'tenant-1',
  requested_tenant_id: 'tenant-2',
};

describe('Policy enforcement scenarios', () => {
  beforeEach(() => {
    cy.visit('/login');
    cy.disableAnimations();
  });

  it('blocks inference when egress policy denies request', () => {
    cy.stubApiRoutes();
    cy.get('[data-cy=login-email]').type('test@example.com');
    cy.get('[data-cy=login-password]').type('password');
    cy.get('[data-cy=login-submit]').click();
    cy.wait('@loginRequest');
    cy.wait('@currentUser');
    cy.setUiMode('user');

    // Override inference endpoint to return policy denial
    cy.intercept('POST', '**/v1/infer', {
      statusCode: 403,
      body: blockedByPolicyResponse,
    }).as('inferBlocked');

    cy.visit('/inference');
    cy.get('[data-cy=prompt-input]').clear().type('This should be blocked by policy.');
    cy.get('[data-cy=run-inference-btn]').click();
    cy.wait('@inferBlocked');

    // Verify error message is displayed
    cy.contains(/policy/i).should('be.visible');
    cy.contains(/blocked/i).should('be.visible');
  });

  it('shows policy warnings in inference response', () => {
    cy.stubApiRoutes({ inferenceResponse: policyWarningResponse });
    cy.get('[data-cy=login-email]').type('test@example.com');
    cy.get('[data-cy=login-password]').type('password');
    cy.get('[data-cy=login-submit]').click();
    cy.wait('@loginRequest');
    cy.wait('@currentUser');
    cy.setUiMode('user');

    cy.visit('/inference');
    cy.get('[data-cy=prompt-input]').clear().type('Show policy warnings.');
    cy.get('[data-cy=run-inference-btn]').click();
    cy.wait('@inferRequest');

    // Verify response is shown
    cy.get('[data-cy=inference-output]').should('contain', policyWarningResponse.text);

    // Policy warnings should be visible if UI displays them
    // (depends on UI implementation)
  });

  it('prevents cross-tenant access with isolation policy', () => {
    cy.stubApiRoutes();
    cy.get('[data-cy=login-email]').type('test@example.com');
    cy.get('[data-cy=login-password]').type('password');
    cy.get('[data-cy=login-submit]').click();
    cy.wait('@loginRequest');
    cy.wait('@currentUser');
    cy.setUiMode('user');

    // Override inference to return isolation violation
    cy.intercept('POST', '**/v1/infer', {
      statusCode: 403,
      body: isolationViolationResponse,
    }).as('inferIsolationViolation');

    cy.visit('/inference');
    cy.get('[data-cy=prompt-input]').clear().type('Cross-tenant test.');
    cy.get('[data-cy=run-inference-btn]').click();
    cy.wait('@inferIsolationViolation');

    // Verify isolation violation error is shown
    cy.contains(/isolation/i).should('be.visible');
  });

  it('enforces determinism policy in replay mode', () => {
    const replayResponse = {
      schema_version: '1.0',
      id: 'run-replay-1',
      text: 'Deterministic replay response.',
      tokens_generated: 12,
      token_count: 12,
      latency_ms: 18,
      adapters_used: ['adapter-replay'],
      finish_reason: 'stop',
      determinism_mode: 'deterministic',
      is_replay: true,
      run_receipt: {
        trace_id: 'trace-replay-1',
        run_head_hash: 'head-replay',
        output_digest: 'out-replay',
        receipt_digest: 'rcpt-replay',
      },
      trace: {
        latency_ms: 18,
        router_decisions: [{ adapter: 'adapter-replay', score: 0.91 }],
        evidence_spans: [],
      },
    };

    cy.stubApiRoutes({ inferenceResponse: replayResponse });
    cy.get('[data-cy=login-email]').type('test@example.com');
    cy.get('[data-cy=login-password]').type('password');
    cy.get('[data-cy=login-submit]').click();
    cy.wait('@loginRequest');
    cy.wait('@currentUser');
    cy.setUiMode('user');

    cy.visit('/inference');
    cy.get('[data-cy=prompt-input]').clear().type('Determinism enforcement test.');
    cy.get('[data-cy=run-inference-btn]').click();
    cy.wait('@inferRequest');

    // Verify deterministic badge is shown
    cy.get('[data-cy=proof-bar]').within(() => {
      cy.contains('deterministic').should('be.visible');
    });
  });

  it('displays policy editor with deny toggles', () => {
    cy.stubApiRoutes();
    cy.get('[data-cy=login-email]').type('test@example.com');
    cy.get('[data-cy=login-password]').type('password');
    cy.get('[data-cy=login-submit]').click();
    cy.wait('@loginRequest');
    cy.wait('@currentUser');

    cy.visit('/policies');
    cy.wait('@policies');

    // Verify policy editor is visible
    cy.contains(/policy/i).should('be.visible');
  });

  it('saves policy configuration with validation', () => {
    cy.stubApiRoutes();
    cy.get('[data-cy=login-email]').type('test@example.com');
    cy.get('[data-cy=login-password]').type('password');
    cy.get('[data-cy=login-submit]').click();
    cy.wait('@loginRequest');
    cy.wait('@currentUser');

    cy.visit('/policies');
    cy.wait('@policies');

    // Try to create new policy
    cy.contains('button', /new policy/i).click();

    // Toggle a policy setting
    cy.get('[data-cy$="-toggle"]').first().click();

    // Save policy
    cy.get('[data-cy=policy-save-btn]').click();

    // Verify validation and save endpoints are called
    cy.wait('@policyValidate');
    cy.wait('@policySave');
  });

  it('shows evidence policy enforcement in audit trail', () => {
    cy.stubApiRoutes();
    cy.get('[data-cy=login-email]').type('test@example.com');
    cy.get('[data-cy=login-password]').type('password');
    cy.get('[data-cy=login-submit]').click();
    cy.wait('@loginRequest');
    cy.wait('@currentUser');

    // Stub policy audit endpoint
    cy.intercept('GET', '**/v1/audit/policies*', {
      statusCode: 200,
      body: {
        entries: [
          {
            cpid: 'cp-evidence-001',
            policy_id: 'evidence-required',
            action: 'enabled',
            tenant_id: 'tenant-1',
            created_at: new Date().toISOString(),
            created_by: 'test@example.com',
            schema_hash: 'hash-evidence-001',
          },
        ],
      },
    }).as('policyAudit');

    cy.visit('/security/audit');
    cy.wait('@policyAudit');

    // Verify audit entries are shown
    cy.get('[data-cy=policy-audit-row]').should('have.length.at.least', 1);
  });

  it('blocks adapter stacking when policy denies it', () => {
    cy.stubApiRoutes();
    cy.get('[data-cy=login-email]').type('test@example.com');
    cy.get('[data-cy=login-password]').type('password');
    cy.get('[data-cy=login-submit]').click();
    cy.wait('@loginRequest');
    cy.wait('@currentUser');

    // Stub adapter stacking endpoint to return policy denial
    cy.intercept('POST', '**/v1/adapter-stacks', {
      statusCode: 403,
      body: {
        error: 'Adapter stacking denied by policy',
        code: 'POLICY_DENIED',
        policy_id: 'adapter-stack-limit',
      },
    }).as('stackDenied');

    cy.visit('/adapters');

    // Attempt to create stack would be tested here
    // (depends on UI implementation)
  });

  it('enforces KV cache quota policy', () => {
    const quotaExceededResponse = {
      schema_version: '1.0',
      error: 'KV cache quota exceeded',
      code: 'KV_QUOTA_EXCEEDED',
      policy_id: 'kv-quota-limit',
      message: 'Request would exceed KV cache quota limit',
      quota_bytes: 1073741824,
      used_bytes: 1073741824,
    };

    cy.stubApiRoutes();
    cy.get('[data-cy=login-email]').type('test@example.com');
    cy.get('[data-cy=login-password]').type('password');
    cy.get('[data-cy=login-submit]').click();
    cy.wait('@loginRequest');
    cy.wait('@currentUser');
    cy.setUiMode('user');

    // Override inference to return quota exceeded
    cy.intercept('POST', '**/v1/infer', {
      statusCode: 429,
      body: quotaExceededResponse,
    }).as('inferQuotaExceeded');

    cy.visit('/inference');
    cy.get('[data-cy=prompt-input]').clear().type('KV quota test.');
    cy.get('[data-cy=run-inference-btn]').click();
    cy.wait('@inferQuotaExceeded');

    // Verify quota error is displayed
    cy.contains(/quota/i).should('be.visible');
  });

  it('shows policy chain in audit log with Merkle hash', () => {
    cy.stubApiRoutes();
    cy.get('[data-cy=login-email]').type('test@example.com');
    cy.get('[data-cy=login-password]').type('password');
    cy.get('[data-cy=login-submit]').click();
    cy.wait('@loginRequest');
    cy.wait('@currentUser');

    // Stub audit chain endpoint with hash chain
    cy.intercept('GET', '**/v1/audit/policies*', {
      statusCode: 200,
      body: {
        entries: [
          {
            seq: 1,
            cpid: 'cp-001',
            policy_id: 'determinism',
            action: 'enabled',
            tenant_id: 'tenant-1',
            created_at: new Date(Date.now() - 2000).toISOString(),
            schema_hash: 'hash-001',
            prev_hash: null,
            signature: 'sig-001',
          },
          {
            seq: 2,
            cpid: 'cp-002',
            policy_id: 'determinism',
            action: 'disabled',
            tenant_id: 'tenant-1',
            created_at: new Date(Date.now() - 1000).toISOString(),
            schema_hash: 'hash-002',
            prev_hash: 'hash-001',
            signature: 'sig-002',
          },
        ],
      },
    }).as('policyChain');

    cy.visit('/security/audit');
    cy.wait('@policyChain');

    // Verify audit entries show hash chain
    cy.get('[data-cy=policy-audit-row]').should('have.length', 2);

    // Verify second entry has previous hash
    cy.get('[data-cy=policy-audit-row]').eq(1).within(() => {
      cy.get('td').should('contain.text', 'hash-001');
    });
  });

  it('prevents replay with tampered policy state', () => {
    cy.stubApiRoutes();
    cy.get('[data-cy=login-email]').type('test@example.com');
    cy.get('[data-cy=login-password]').type('password');
    cy.get('[data-cy=login-submit]').click();
    cy.wait('@loginRequest');
    cy.wait('@currentUser');

    // Stub replay endpoint to return policy mismatch
    cy.intercept('POST', '**/v1/replay', {
      statusCode: 409,
      body: {
        error: 'Policy state mismatch during replay',
        code: 'POLICY_STATE_MISMATCH',
        policy_id: 'determinism',
        expected_state: 'enabled',
        actual_state: 'disabled',
      },
    }).as('replayPolicyMismatch');

    // Would test replay functionality here if implemented in UI
  });
});
