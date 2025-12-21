/// <reference types="cypress" />

const receiptResponse = {
  schema_version: '1.0',
  id: 'run-receipt-1',
  text: 'Stubbed answer body for receipt validation.',
  tokens_generated: 8,
  token_count: 8,
  latency_ms: 18,
  adapters_used: ['adapter-A', 'adapter-B'],
  finish_reason: 'stop',
  run_receipt: {
    trace_id: 'trace-777',
    run_head_hash: 'head-777',
    output_digest: 'out-777',
    receipt_digest: 'rcpt-stub-777',
  },
  trace: {
    latency_ms: 18,
    router_decisions: [{ adapter: 'adapter-A', score: 0.91 }],
    evidence_spans: [{ text: 'source snippet', relevance: 0.87 }],
  },
};

describe('Inference receipt flow', () => {
  beforeEach(() => {
    cy.stubApiRoutes({ inferenceResponse: receiptResponse });
    cy.visit('/login');
    cy.disableAnimations();
    cy.get('[data-cy=login-email]').type('test@example.com');
    cy.get('[data-cy=login-password]').type('password');
    cy.get('[data-cy=login-submit]').click();
    cy.wait('@loginRequest');
    cy.wait('@currentUser');
    cy.visit('/inference');
  });

  it('submits a prompt and renders receipt with digests', () => {
    cy.get('[data-cy=prompt-input]').clear().type('Generate a short answer about adapters.');
    cy.get('[data-cy=run-inference-btn]').click();
    cy.wait('@inferRequest');

    cy.boundedPoll(
      () => cy.get('[data-cy=receipt-digest]').should('contain', 'rcpt-stub-777'),
      { timeout: 10000, interval: 500 }
    );

    cy.get('[data-cy=receipt-trace-meta]').should('contain', 'trace-777').and('contain', 'out-777');
    cy.get('[data-cy=adapter-list]').within(() => {
      cy.contains('adapter-A');
      cy.contains('adapter-B');
    });
    cy.get('[data-cy=inference-output]').should('contain', receiptResponse.text);
  });
});
