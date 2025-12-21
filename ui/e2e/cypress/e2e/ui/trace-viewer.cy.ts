/// <reference types="cypress" />

const traceResponse = {
  schema_version: '1.0',
  id: 'run-trace-1',
  text: 'Trace-heavy response',
  tokens_generated: 5,
  token_count: 5,
  latency_ms: 15,
  adapters_used: ['adapter-A'],
  finish_reason: 'stop',
  run_receipt: {
    trace_id: 'trace-open',
    run_head_hash: 'head-open',
    output_digest: 'out-open',
    receipt_digest: 'rcpt-open',
  },
  trace: {
    latency_ms: 15,
    router_decisions: [{ adapter: 'adapter-A', score: 0.93 }],
    evidence_spans: [{ text: 'trace evidence', relevance: 0.8 }],
  },
};

describe('Trace viewer', () => {
  beforeEach(() => {
    cy.stubApiRoutes({ inferenceResponse: traceResponse });
    cy.visit('/login');
    cy.disableAnimations();
    cy.get('[data-cy=login-email]').type('test@example.com');
    cy.get('[data-cy=login-password]').type('password');
    cy.get('[data-cy=login-submit]').click();
    cy.wait('@loginRequest');
    cy.wait('@currentUser');
    cy.setUiMode('user');
    cy.visit('/inference');
  });

  it('opens trace viewer from receipt card', () => {
    cy.get('[data-cy=prompt-input]').clear().type('Show trace output.');
    cy.get('[data-cy=run-inference-btn]').click();
    cy.wait('@inferRequest');

    cy.get('[data-cy=proofbar-trace-id]').should('contain', 'trace-open');
    cy.get('[data-cy=proofbar-receipt-digest]').should('contain', 'rcpt-open');
    cy.get('[data-cy=proofbar-open-trace]').should('be.visible').click();

    cy.location('pathname').should('include', '/telemetry');
    cy.location('search').should('include', 'requestId=trace-open');
    cy.location('search').should('include', 'tab=viewer');
  });
});
