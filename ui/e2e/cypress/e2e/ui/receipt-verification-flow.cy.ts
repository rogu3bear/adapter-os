/// <reference types="cypress" />

const verifiedReceiptResponse = {
  schema_version: '1.0',
  id: 'run-verified-1',
  text: 'Verified inference response with complete receipt.',
  tokens_generated: 15,
  token_count: 15,
  latency_ms: 30,
  adapters_used: ['adapter-verified-A', 'adapter-verified-B'],
  finish_reason: 'stop',
  run_receipt: {
    trace_id: 'trace-verified-123',
    run_head_hash: 'head-verified-abc123',
    output_digest: 'out-verified-xyz789',
    receipt_digest: 'rcpt-verified-final-999',
  },
  trace: {
    latency_ms: 30,
    router_decisions: [
      { adapter: 'adapter-verified-A', score: 0.95 },
      { adapter: 'adapter-verified-B', score: 0.87 },
    ],
    evidence_spans: [
      { text: 'verified evidence A', relevance: 0.93 },
      { text: 'verified evidence B', relevance: 0.89 },
    ],
  },
};

const invalidReceiptResponse = {
  schema_version: '1.0',
  id: 'run-invalid-1',
  text: 'Response with tampered receipt.',
  tokens_generated: 10,
  token_count: 10,
  latency_ms: 20,
  adapters_used: ['adapter-invalid'],
  finish_reason: 'stop',
  run_receipt: {
    trace_id: 'trace-invalid-456',
    run_head_hash: 'head-tampered',
    output_digest: 'out-tampered',
    receipt_digest: 'rcpt-invalid-tampered',
  },
  trace: {
    latency_ms: 20,
    router_decisions: [{ adapter: 'adapter-invalid', score: 0.75 }],
    evidence_spans: [],
  },
};

describe('Receipt verification flow', () => {
  beforeEach(() => {
    cy.visit('/login');
    cy.disableAnimations();
  });

  it('verifies valid receipt and shows verification badge', () => {
    cy.stubApiRoutes({ inferenceResponse: verifiedReceiptResponse });
    cy.get('[data-cy=login-email]').type('test@example.com');
    cy.get('[data-cy=login-password]').type('password');
    cy.get('[data-cy=login-submit]').click();
    cy.wait('@loginRequest');
    cy.wait('@currentUser');
    cy.setUiMode('user');

    // Stub receipt verification endpoint
    cy.intercept('POST', '**/v1/receipts/verify', {
      statusCode: 200,
      body: {
        valid: true,
        receipt_digest: 'rcpt-verified-final-999',
        trace_id: 'trace-verified-123',
        verified_at: new Date().toISOString(),
        signature_valid: true,
      },
    }).as('verifyReceipt');

    cy.visit('/inference');
    cy.get('[data-cy=prompt-input]').clear().type('Verify this receipt.');
    cy.get('[data-cy=run-inference-btn]').click();
    cy.wait('@inferRequest');

    // Verify receipt digest is displayed
    cy.get('[data-cy=proofbar-receipt-digest]').should('contain', 'rcpt-verified-final-999');
    cy.get('[data-cy=proofbar-trace-id]').should('contain', 'trace-verified-123');

    // Verify all receipt metadata is shown
    cy.get('[data-cy=receipt-digest]').within(() => {
      cy.contains('rcpt-verified-final-999').should('be.visible');
    });

    cy.get('[data-cy=receipt-trace-meta]').within(() => {
      cy.contains('trace-verified-123').should('be.visible');
      cy.contains('out-verified-xyz789').should('be.visible');
    });
  });

  it('copies receipt digest to clipboard', () => {
    cy.stubApiRoutes({ inferenceResponse: verifiedReceiptResponse });
    cy.get('[data-cy=login-email]').type('test@example.com');
    cy.get('[data-cy=login-password]').type('password');
    cy.get('[data-cy=login-submit]').click();
    cy.wait('@loginRequest');
    cy.wait('@currentUser');
    cy.setUiMode('user');

    cy.visit('/inference');
    cy.get('[data-cy=prompt-input]').clear().type('Copy receipt test.');
    cy.get('[data-cy=run-inference-btn]').click();
    cy.wait('@inferRequest');

    // Click copy button on receipt digest
    cy.get('[data-cy=proofbar-receipt-digest]').within(() => {
      cy.get('button[aria-label*="Copy"]').click();
    });

    // Verify copy success toast
    cy.contains(/copied/i).should('be.visible');
  });

  it('copies trace ID to clipboard', () => {
    cy.stubApiRoutes({ inferenceResponse: verifiedReceiptResponse });
    cy.get('[data-cy=login-email]').type('test@example.com');
    cy.get('[data-cy=login-password]').type('password');
    cy.get('[data-cy=login-submit]').click();
    cy.wait('@loginRequest');
    cy.wait('@currentUser');
    cy.setUiMode('user');

    cy.visit('/inference');
    cy.get('[data-cy=prompt-input]').clear().type('Copy trace ID test.');
    cy.get('[data-cy=run-inference-btn]').click();
    cy.wait('@inferRequest');

    // Click copy button on trace ID
    cy.get('[data-cy=proofbar-trace-id]').within(() => {
      cy.get('button[aria-label*="Copy"]').click();
    });

    // Verify copy success toast
    cy.contains(/copied/i).should('be.visible');
  });

  it('shows all adapters used in receipt', () => {
    cy.stubApiRoutes({ inferenceResponse: verifiedReceiptResponse });
    cy.get('[data-cy=login-email]').type('test@example.com');
    cy.get('[data-cy=login-password]').type('password');
    cy.get('[data-cy=login-submit]').click();
    cy.wait('@loginRequest');
    cy.wait('@currentUser');
    cy.setUiMode('user');

    cy.visit('/inference');
    cy.get('[data-cy=prompt-input]').clear().type('Show all adapters.');
    cy.get('[data-cy=run-inference-btn]').click();
    cy.wait('@inferRequest');

    // Verify both adapters are listed
    cy.get('[data-cy=adapter-list]').within(() => {
      cy.contains('adapter-verified-A').should('be.visible');
      cy.contains('adapter-verified-B').should('be.visible');
    });
  });

  it('displays determinism badge correctly', () => {
    const deterministicResponse = {
      ...verifiedReceiptResponse,
      determinism_mode: 'deterministic',
    };

    cy.stubApiRoutes({ inferenceResponse: deterministicResponse });
    cy.get('[data-cy=login-email]').type('test@example.com');
    cy.get('[data-cy=login-password]').type('password');
    cy.get('[data-cy=login-submit]').click();
    cy.wait('@loginRequest');
    cy.wait('@currentUser');
    cy.setUiMode('user');

    cy.visit('/inference');
    cy.get('[data-cy=prompt-input]').clear().type('Determinism badge test.');
    cy.get('[data-cy=run-inference-btn]').click();
    cy.wait('@inferRequest');

    // Verify determinism badge is shown
    cy.get('[data-cy=proof-bar]').within(() => {
      cy.contains('deterministic').should('be.visible');
    });
  });

  it('shows backend used in receipt', () => {
    const backendResponse = {
      ...verifiedReceiptResponse,
      backend: 'coreml',
    };

    cy.stubApiRoutes({ inferenceResponse: backendResponse });
    cy.get('[data-cy=login-email]').type('test@example.com');
    cy.get('[data-cy=login-password]').type('password');
    cy.get('[data-cy=login-submit]').click();
    cy.wait('@loginRequest');
    cy.wait('@currentUser');
    cy.setUiMode('user');

    cy.visit('/inference');
    cy.get('[data-cy=prompt-input]').clear().type('Backend display test.');
    cy.get('[data-cy=run-inference-btn]').click();
    cy.wait('@inferRequest');

    // Verify backend is displayed
    cy.get('[data-cy=proof-bar]').within(() => {
      cy.contains('Backend:').should('be.visible');
      cy.contains('coreml').should('be.visible');
    });
  });

  it('handles receipt verification failure gracefully', () => {
    cy.stubApiRoutes({ inferenceResponse: invalidReceiptResponse });
    cy.get('[data-cy=login-email]').type('test@example.com');
    cy.get('[data-cy=login-password]').type('password');
    cy.get('[data-cy=login-submit]').click();
    cy.wait('@loginRequest');
    cy.wait('@currentUser');
    cy.setUiMode('user');

    // Stub failed verification
    cy.intercept('POST', '**/v1/receipts/verify', {
      statusCode: 400,
      body: {
        valid: false,
        receipt_digest: 'rcpt-invalid-tampered',
        error: 'Receipt signature verification failed',
        signature_valid: false,
      },
    }).as('verifyReceiptFailed');

    cy.visit('/inference');
    cy.get('[data-cy=prompt-input]').clear().type('Invalid receipt test.');
    cy.get('[data-cy=run-inference-btn]').click();
    cy.wait('@inferRequest');

    // Receipt should still be displayed even if verification fails
    cy.get('[data-cy=proofbar-receipt-digest]').should('contain', 'rcpt-invalid-tampered');
    cy.get('[data-cy=proofbar-trace-id]').should('contain', 'trace-invalid-456');
  });

  it('displays latency and token metrics in receipt', () => {
    cy.stubApiRoutes({ inferenceResponse: verifiedReceiptResponse });
    cy.get('[data-cy=login-email]').type('test@example.com');
    cy.get('[data-cy=login-password]').type('password');
    cy.get('[data-cy=login-submit]').click();
    cy.wait('@loginRequest');
    cy.wait('@currentUser');
    cy.setUiMode('user');

    cy.visit('/inference');
    cy.get('[data-cy=prompt-input]').clear().type('Metrics display test.');
    cy.get('[data-cy=run-inference-btn]').click();
    cy.wait('@inferRequest');

    // Verify metrics are displayed
    cy.get('[data-cy=latency]').should('contain', '30').and('contain', 'ms');
    cy.get('[data-cy=token-usage]').should('contain', '15');
  });

  it('navigates to trace viewer from receipt', () => {
    cy.stubApiRoutes({ inferenceResponse: verifiedReceiptResponse });
    cy.get('[data-cy=login-email]').type('test@example.com');
    cy.get('[data-cy=login-password]').type('password');
    cy.get('[data-cy=login-submit]').click();
    cy.wait('@loginRequest');
    cy.wait('@currentUser');
    cy.setUiMode('user');

    cy.visit('/inference');
    cy.get('[data-cy=prompt-input]').clear().type('Navigate to trace.');
    cy.get('[data-cy=run-inference-btn]').click();
    cy.wait('@inferRequest');

    // Click open trace button
    cy.get('[data-cy=proofbar-open-trace]').should('be.visible').click();

    // Verify navigation to telemetry page with correct params
    cy.location('pathname').should('include', '/telemetry');
    cy.location('search').should('include', 'requestId=trace-verified-123');
    cy.location('search').should('include', 'tab=viewer');
  });
});
