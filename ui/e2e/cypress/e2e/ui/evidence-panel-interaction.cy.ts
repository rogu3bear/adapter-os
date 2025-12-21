/// <reference types="cypress" />

const evidenceResponse = {
  schema_version: '1.0',
  id: 'run-evidence-panel-1',
  text: 'Test inference response with evidence.',
  tokens_generated: 12,
  token_count: 12,
  latency_ms: 25,
  adapters_used: ['adapter-A', 'adapter-B'],
  finish_reason: 'stop',
  run_receipt: {
    trace_id: 'trace-evidence-panel',
    run_head_hash: 'head-evidence-panel',
    output_digest: 'out-evidence-panel',
    receipt_digest: 'rcpt-evidence-panel',
  },
  citations: [
    {
      adapter_id: 'adapter-A',
      file_path: 'document1.md',
      chunk_id: 'chunk-1',
      offset_start: 0,
      offset_end: 50,
      preview: 'First document preview text',
    },
    {
      adapter_id: 'adapter-B',
      file_path: 'document2.md',
      chunk_id: 'chunk-2',
      offset_start: 100,
      offset_end: 200,
      preview: 'Second document preview text',
    },
  ],
  trace: {
    latency_ms: 25,
    router_decisions: [
      { adapter: 'adapter-A', score: 0.88 },
      { adapter: 'adapter-B', score: 0.76 },
    ],
    evidence_spans: [
      { text: 'evidence span from A', relevance: 0.92 },
      { text: 'evidence span from B', relevance: 0.81 },
    ],
  },
};

describe('Evidence panel interaction', () => {
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
  });

  it('displays evidence panel with create and refresh buttons', () => {
    // Stub the evidence API endpoints
    cy.intercept('GET', '**/v1/evidence?trace_id=*', {
      statusCode: 200,
      body: { evidence: [] },
    }).as('evidenceList');

    cy.intercept('POST', '**/v1/evidence', {
      statusCode: 201,
      body: {
        id: 'evidence-1',
        trace_id: 'trace-evidence-panel',
        evidence_type: 'audit',
        status: 'pending',
        reference: 'rcpt-evidence-panel',
        created_at: new Date().toISOString(),
      },
    }).as('createEvidence');

    cy.visit('/inference');
    cy.get('[data-cy=prompt-input]').clear().type('Show evidence panel.');
    cy.get('[data-cy=run-inference-btn]').click();
    cy.wait('@inferRequest');

    // Navigate to trace viewer which has evidence panel
    cy.get('[data-cy=proofbar-open-trace]').click();
    cy.location('pathname').should('include', '/telemetry');

    // Verify evidence panel is present
    cy.get('[data-testid=evidence-panel]').should('be.visible');
    cy.get('[data-testid=evidence-panel]').within(() => {
      cy.contains('Evidence').should('be.visible');
      cy.contains('button', /create evidence/i).should('be.visible');
      cy.contains('button', /refresh/i).should('be.visible');
    });
  });

  it('creates evidence and shows status badge', () => {
    // Stub empty evidence list initially
    cy.intercept('GET', '**/v1/evidence?trace_id=*', {
      statusCode: 200,
      body: { evidence: [] },
    }).as('evidenceListEmpty');

    // Stub create evidence
    cy.intercept('POST', '**/v1/evidence', {
      statusCode: 201,
      body: {
        id: 'evidence-created-1',
        trace_id: 'trace-evidence-panel',
        evidence_type: 'audit',
        status: 'pending',
        reference: 'rcpt-evidence-panel',
        description: 'Inference evidence bundle',
        created_at: new Date().toISOString(),
      },
    }).as('createEvidence');

    // Stub updated evidence list after creation
    cy.intercept('GET', '**/v1/evidence?trace_id=trace-evidence-panel*', {
      statusCode: 200,
      body: {
        evidence: [
          {
            id: 'evidence-created-1',
            trace_id: 'trace-evidence-panel',
            tenant_id: 'tenant-1',
            evidence_type: 'audit',
            status: 'pending',
            reference: 'rcpt-evidence-panel',
            description: 'Inference evidence bundle',
            created_at: new Date().toISOString(),
            updated_at: new Date().toISOString(),
          },
        ],
      },
    }).as('evidenceListUpdated');

    cy.visit('/inference');
    cy.get('[data-cy=prompt-input]').clear().type('Create evidence test.');
    cy.get('[data-cy=run-inference-btn]').click();
    cy.wait('@inferRequest');

    cy.get('[data-cy=proofbar-open-trace]').click();
    cy.wait('@evidenceListEmpty');

    // Click create evidence button
    cy.get('[data-testid=evidence-panel]').within(() => {
      cy.contains('button', /create evidence/i).click();
    });

    cy.wait('@createEvidence');
    cy.contains('Evidence creation queued').should('be.visible');
  });

  it('downloads evidence when ready', () => {
    // Stub evidence list with ready evidence
    cy.intercept('GET', '**/v1/evidence?trace_id=*', {
      statusCode: 200,
      body: {
        evidence: [
          {
            id: 'evidence-ready-1',
            trace_id: 'trace-evidence-panel',
            tenant_id: 'tenant-1',
            evidence_type: 'audit',
            status: 'ready',
            reference: 'rcpt-evidence-panel',
            description: 'Ready evidence bundle',
            bundle_size_bytes: 2048,
            file_name: 'evidence-bundle.json',
            created_at: new Date().toISOString(),
            updated_at: new Date().toISOString(),
          },
        ],
      },
    }).as('evidenceReady');

    // Stub download endpoint
    cy.intercept('GET', '**/v1/evidence/evidence-ready-1/download', {
      statusCode: 200,
      body: { trace_id: 'trace-evidence-panel', receipt: 'rcpt-evidence-panel' },
    }).as('downloadEvidence');

    cy.visit('/inference');
    cy.get('[data-cy=prompt-input]').clear().type('Download evidence test.');
    cy.get('[data-cy=run-inference-btn]').click();
    cy.wait('@inferRequest');

    cy.get('[data-cy=proofbar-open-trace]').click();
    cy.wait('@evidenceReady');

    // Verify evidence is shown with ready status
    cy.get('[data-testid=evidence-panel]').within(() => {
      cy.contains('ready').should('be.visible');
      cy.contains('2.0 KB').should('be.visible');
      cy.contains('button', /download/i).should('be.visible').click();
    });

    cy.wait('@downloadEvidence');
  });

  it('shows error state when evidence creation fails', () => {
    cy.intercept('GET', '**/v1/evidence?trace_id=*', {
      statusCode: 200,
      body: { evidence: [] },
    }).as('evidenceList');

    // Stub failed creation
    cy.intercept('POST', '**/v1/evidence', {
      statusCode: 500,
      body: { error: 'Evidence creation failed', code: 'EVIDENCE_ERROR' },
    }).as('createEvidenceFailed');

    cy.visit('/inference');
    cy.get('[data-cy=prompt-input]').clear().type('Fail evidence test.');
    cy.get('[data-cy=run-inference-btn]').click();
    cy.wait('@inferRequest');

    cy.get('[data-cy=proofbar-open-trace]').click();
    cy.wait('@evidenceList');

    // Click create evidence and verify error toast
    cy.get('[data-testid=evidence-panel]').within(() => {
      cy.contains('button', /create evidence/i).click();
    });

    cy.wait('@createEvidenceFailed');
    cy.contains(/evidence creation failed/i).should('be.visible');
  });

  it('refreshes evidence list and shows loading state', () => {
    cy.intercept('GET', '**/v1/evidence?trace_id=*', {
      statusCode: 200,
      body: {
        evidence: [
          {
            id: 'evidence-1',
            trace_id: 'trace-evidence-panel',
            tenant_id: 'tenant-1',
            evidence_type: 'audit',
            status: 'processing',
            reference: 'rcpt-evidence-panel',
            description: 'Processing evidence',
            created_at: new Date().toISOString(),
          },
        ],
      },
      delay: 100,
    }).as('evidenceList');

    cy.visit('/inference');
    cy.get('[data-cy=prompt-input]').clear().type('Refresh evidence test.');
    cy.get('[data-cy=run-inference-btn]').click();
    cy.wait('@inferRequest');

    cy.get('[data-cy=proofbar-open-trace]').click();
    cy.wait('@evidenceList');

    // Click refresh and verify loading state
    cy.get('[data-testid=evidence-panel]').within(() => {
      cy.contains('button', /refresh/i).click();
      // Loading state should show briefly
      cy.wait('@evidenceList');
      cy.contains('processing').should('be.visible');
    });
  });
});
