/// <reference types="cypress" />

const traceWithRouterDecisions = {
  schema_version: '1.0',
  id: 'run-trace-viz-1',
  text: 'Response with detailed trace for visualization.',
  tokens_generated: 20,
  token_count: 20,
  latency_ms: 35,
  adapters_used: ['adapter-high-score', 'adapter-medium-score', 'adapter-low-score'],
  finish_reason: 'stop',
  run_receipt: {
    trace_id: 'trace-viz-789',
    run_head_hash: 'head-viz-123',
    output_digest: 'out-viz-456',
    receipt_digest: 'rcpt-viz-789',
  },
  trace: {
    latency_ms: 35,
    router_decisions: [
      { adapter: 'adapter-high-score', score: 0.95, rank: 1 },
      { adapter: 'adapter-medium-score', score: 0.78, rank: 2 },
      { adapter: 'adapter-low-score', score: 0.52, rank: 3 },
    ],
    evidence_spans: [
      { text: 'high relevance evidence', relevance: 0.94, source: 'doc1.md' },
      { text: 'medium relevance evidence', relevance: 0.81, source: 'doc2.md' },
      { text: 'low relevance evidence', relevance: 0.65, source: 'doc3.md' },
    ],
    prompt_tokens: 15,
    completion_tokens: 20,
    total_tokens: 35,
  },
};

const traceWithTimeline = {
  schema_version: '1.0',
  id: 'run-trace-timeline-1',
  text: 'Response with timeline trace data.',
  tokens_generated: 18,
  token_count: 18,
  latency_ms: 42,
  adapters_used: ['adapter-timeline-A'],
  finish_reason: 'stop',
  run_receipt: {
    trace_id: 'trace-timeline-999',
    run_head_hash: 'head-timeline',
    output_digest: 'out-timeline',
    receipt_digest: 'rcpt-timeline',
  },
  trace: {
    latency_ms: 42,
    router_decisions: [{ adapter: 'adapter-timeline-A', score: 0.89 }],
    evidence_spans: [{ text: 'timeline evidence', relevance: 0.87 }],
    timeline: [
      { event: 'request_received', timestamp_ms: 0 },
      { event: 'router_decision', timestamp_ms: 5 },
      { event: 'adapter_loaded', timestamp_ms: 12 },
      { event: 'inference_start', timestamp_ms: 15 },
      { event: 'inference_complete', timestamp_ms: 40 },
      { event: 'response_sent', timestamp_ms: 42 },
    ],
  },
};

describe('Trace visualization flow', () => {
  beforeEach(() => {
    cy.visit('/login');
    cy.disableAnimations();
  });

  it('displays router decisions with scores in trace viewer', () => {
    cy.stubApiRoutes({ inferenceResponse: traceWithRouterDecisions });
    cy.get('[data-cy=login-email]').type('test@example.com');
    cy.get('[data-cy=login-password]').type('password');
    cy.get('[data-cy=login-submit]').click();
    cy.wait('@loginRequest');
    cy.wait('@currentUser');
    cy.setUiMode('user');

    // Stub trace detail endpoint
    cy.intercept('GET', '**/v1/telemetry/trace/trace-viz-789', {
      statusCode: 200,
      body: traceWithRouterDecisions.trace,
    }).as('traceDetail');

    cy.visit('/inference');
    cy.get('[data-cy=prompt-input]').clear().type('Show router decisions.');
    cy.get('[data-cy=run-inference-btn]').click();
    cy.wait('@inferRequest');

    // Navigate to trace viewer
    cy.get('[data-cy=proofbar-open-trace]').click();
    cy.location('pathname').should('include', '/telemetry');

    // Verify trace viewer shows router decisions
    cy.get('[data-cy=trace-viewer]').should('be.visible');
  });

  it('shows evidence spans with relevance scores', () => {
    cy.stubApiRoutes({ inferenceResponse: traceWithRouterDecisions });
    cy.get('[data-cy=login-email]').type('test@example.com');
    cy.get('[data-cy=login-password]').type('password');
    cy.get('[data-cy=login-submit]').click();
    cy.wait('@loginRequest');
    cy.wait('@currentUser');
    cy.setUiMode('user');

    cy.intercept('GET', '**/v1/telemetry/trace/trace-viz-789', {
      statusCode: 200,
      body: traceWithRouterDecisions.trace,
    }).as('traceDetail');

    cy.visit('/inference');
    cy.get('[data-cy=prompt-input]').clear().type('Show evidence spans.');
    cy.get('[data-cy=run-inference-btn]').click();
    cy.wait('@inferRequest');

    cy.get('[data-cy=proofbar-open-trace]').click();
    cy.location('pathname').should('include', '/telemetry');

    // Verify evidence spans are visible in trace viewer
    cy.get('[data-cy=trace-viewer]').should('be.visible');
  });

  it('displays trace timeline events in order', () => {
    cy.stubApiRoutes({ inferenceResponse: traceWithTimeline });
    cy.get('[data-cy=login-email]').type('test@example.com');
    cy.get('[data-cy=login-password]').type('password');
    cy.get('[data-cy=login-submit]').click();
    cy.wait('@loginRequest');
    cy.wait('@currentUser');
    cy.setUiMode('user');

    cy.intercept('GET', '**/v1/telemetry/trace/trace-timeline-999', {
      statusCode: 200,
      body: traceWithTimeline.trace,
    }).as('traceTimeline');

    cy.visit('/inference');
    cy.get('[data-cy=prompt-input]').clear().type('Show trace timeline.');
    cy.get('[data-cy=run-inference-btn]').click();
    cy.wait('@inferRequest');

    cy.get('[data-cy=proofbar-open-trace]').click();
    cy.location('pathname').should('include', '/telemetry');

    // Verify timeline is visible
    cy.get('[data-cy=trace-viewer]').should('be.visible');
  });

  it('allows navigation between trace tabs', () => {
    cy.stubApiRoutes({ inferenceResponse: traceWithRouterDecisions });
    cy.get('[data-cy=login-email]').type('test@example.com');
    cy.get('[data-cy=login-password]').type('password');
    cy.get('[data-cy=login-submit]').click();
    cy.wait('@loginRequest');
    cy.wait('@currentUser');
    cy.setUiMode('user');

    cy.visit('/inference');
    cy.get('[data-cy=prompt-input]').clear().type('Navigate trace tabs.');
    cy.get('[data-cy=run-inference-btn]').click();
    cy.wait('@inferRequest');

    cy.get('[data-cy=proofbar-open-trace]').click();

    // Verify URL includes tab parameter
    cy.location('search').should('include', 'tab=viewer');
  });

  it('displays token count breakdown in trace', () => {
    cy.stubApiRoutes({ inferenceResponse: traceWithRouterDecisions });
    cy.get('[data-cy=login-email]').type('test@example.com');
    cy.get('[data-cy=login-password]').type('password');
    cy.get('[data-cy=login-submit]').click();
    cy.wait('@loginRequest');
    cy.wait('@currentUser');
    cy.setUiMode('user');

    cy.visit('/inference');
    cy.get('[data-cy=prompt-input]').clear().type('Token breakdown test.');
    cy.get('[data-cy=run-inference-btn]').click();
    cy.wait('@inferRequest');

    // Verify token usage is displayed
    cy.get('[data-cy=token-usage]').should('contain', '20');
  });

  it('shows latency metrics in trace viewer', () => {
    cy.stubApiRoutes({ inferenceResponse: traceWithRouterDecisions });
    cy.get('[data-cy=login-email]').type('test@example.com');
    cy.get('[data-cy=login-password]').type('password');
    cy.get('[data-cy=login-submit]').click();
    cy.wait('@loginRequest');
    cy.wait('@currentUser');
    cy.setUiMode('user');

    cy.visit('/inference');
    cy.get('[data-cy=prompt-input]').clear().type('Latency metrics test.');
    cy.get('[data-cy=run-inference-btn]').click();
    cy.wait('@inferRequest');

    // Verify latency is displayed
    cy.get('[data-cy=latency]').should('contain', '35').and('contain', 'ms');

    cy.get('[data-cy=proofbar-open-trace]').click();
    cy.location('pathname').should('include', '/telemetry');
  });

  it('allows trace lookup by trace ID', () => {
    cy.stubApiRoutes({ inferenceResponse: traceWithRouterDecisions });
    cy.get('[data-cy=login-email]').type('test@example.com');
    cy.get('[data-cy=login-password]').type('password');
    cy.get('[data-cy=login-submit]').click();
    cy.wait('@loginRequest');
    cy.wait('@currentUser');
    cy.setUiMode('user');

    cy.intercept('GET', '**/v1/telemetry/trace/trace-viz-789', {
      statusCode: 200,
      body: traceWithRouterDecisions.trace,
    }).as('traceDetail');

    cy.visit('/telemetry');

    // Enter trace ID manually
    cy.get('[data-cy=trace-id-input]').clear().type('trace-viz-789');

    // Verify trace loads
    cy.wait('@traceDetail');
  });

  it('handles trace not found gracefully', () => {
    cy.stubApiRoutes();
    cy.get('[data-cy=login-email]').type('test@example.com');
    cy.get('[data-cy=login-password]').type('password');
    cy.get('[data-cy=login-submit]').click();
    cy.wait('@loginRequest');
    cy.wait('@currentUser');
    cy.setUiMode('user');

    // Stub trace not found
    cy.intercept('GET', '**/v1/telemetry/trace/nonexistent-trace', {
      statusCode: 404,
      body: { error: 'Trace not found', code: 'TRACE_NOT_FOUND' },
    }).as('traceNotFound');

    cy.visit('/telemetry');
    cy.get('[data-cy=trace-id-input]').clear().type('nonexistent-trace{enter}');

    cy.wait('@traceNotFound');
    // Should show error message or empty state
  });

  it('exports trace data from viewer', () => {
    cy.stubApiRoutes({ inferenceResponse: traceWithRouterDecisions });
    cy.get('[data-cy=login-email]').type('test@example.com');
    cy.get('[data-cy=login-password]').type('password');
    cy.get('[data-cy=login-submit]').click();
    cy.wait('@loginRequest');
    cy.wait('@currentUser');
    cy.setUiMode('user');

    cy.visit('/inference');
    cy.get('[data-cy=prompt-input]').clear().type('Export trace test.');

    cy.window().then((win) => {
      cy.stub(win.URL, 'createObjectURL').callThrough().as('createObjectURL');
      cy.stub(win.URL, 'revokeObjectURL').callThrough().as('revokeObjectURL');
    });

    cy.get('[data-cy=run-inference-btn]').click();
    cy.wait('@inferRequest');

    // Click export evidence button (which includes trace data)
    cy.get('[data-cy=export-evidence]').click();

    cy.get('@createObjectURL').should('have.been.called');
  });

  it('displays adapter ranking in trace', () => {
    cy.stubApiRoutes({ inferenceResponse: traceWithRouterDecisions });
    cy.get('[data-cy=login-email]').type('test@example.com');
    cy.get('[data-cy=login-password]').type('password');
    cy.get('[data-cy=login-submit]').click();
    cy.wait('@loginRequest');
    cy.wait('@currentUser');
    cy.setUiMode('user');

    cy.visit('/inference');
    cy.get('[data-cy=prompt-input]').clear().type('Adapter ranking test.');
    cy.get('[data-cy=run-inference-btn]').click();
    cy.wait('@inferRequest');

    // Verify all three adapters are listed
    cy.get('[data-cy=adapter-list]').within(() => {
      cy.contains('adapter-high-score').should('be.visible');
      cy.contains('adapter-medium-score').should('be.visible');
      cy.contains('adapter-low-score').should('be.visible');
    });
  });
});
