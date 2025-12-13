/// <reference types="cypress" />

const evidenceResponse = {
  schema_version: '1.0',
  id: 'run-evidence-1',
  text: 'Evidence rich response body.',
  tokens_generated: 10,
  token_count: 10,
  latency_ms: 20,
  adapters_used: ['adapter-A'],
  finish_reason: 'stop',
  run_receipt: {
    trace_id: 'trace-evidence',
    run_head_hash: 'head-evidence',
    output_digest: 'out-evidence',
    receipt_digest: 'rcpt-evidence',
  },
  citations: [{ adapter_id: 'adapter-A', file_path: 'doc.md', chunk_id: 'chunk-1', offset_start: 0, offset_end: 10, preview: 'doc preview' }],
  trace: {
    latency_ms: 20,
    router_decisions: [{ adapter: 'adapter-A', score: 0.9 }],
    evidence_spans: [{ text: 'evidence span', relevance: 0.95 }],
  },
};

describe('Evidence export', () => {
  beforeEach(() => {
    cy.stubApiRoutes({ inferenceResponse: evidenceResponse });
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

  it('exports evidence bundle with receipt digest filename', () => {
    cy.get('[data-cy=prompt-input]').clear().type('Export evidence bundle.');
    cy.window().then((win) => {
      cy.stub(win.URL, 'createObjectURL').callThrough().as('createObjectURL');
      cy.stub(win.URL, 'revokeObjectURL').callThrough().as('revokeObjectURL');
    });

    cy.get('[data-cy=run-inference-btn]').click();
    cy.wait('@inferRequest');
    cy.get('[data-cy=proofbar-receipt-digest]').should('contain', 'rcpt-evidence');
    cy.get('[data-cy=export-evidence]').click();

    cy.get('@createObjectURL').should('have.been.called');
    cy.get('@revokeObjectURL').should('have.been.called');
  });
});
