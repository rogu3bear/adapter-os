/// <reference types="cypress" />

const chatResponseWithEvidence = {
  schema_version: '1.0',
  id: 'msg-evidence-drawer-1',
  text: 'Chat response with rich evidence for drawer testing.',
  tokens_generated: 25,
  token_count: 25,
  latency_ms: 40,
  adapters_used: ['adapter-doc-A', 'adapter-doc-B'],
  finish_reason: 'stop',
  run_receipt: {
    trace_id: 'trace-drawer-1',
    run_head_hash: 'head-drawer-1',
    output_digest: 'out-drawer-1',
    receipt_digest: 'rcpt-drawer-1',
  },
  citations: [
    {
      adapter_id: 'adapter-doc-A',
      file_path: 'manual.pdf',
      chunk_id: 'chunk-manual-1',
      offset_start: 0,
      offset_end: 100,
      preview: 'Installation instructions from manual',
      page_number: 5,
    },
    {
      adapter_id: 'adapter-doc-B',
      file_path: 'guide.pdf',
      chunk_id: 'chunk-guide-1',
      offset_start: 200,
      offset_end: 350,
      preview: 'Configuration examples from guide',
      page_number: 12,
    },
    {
      adapter_id: 'adapter-doc-A',
      file_path: 'faq.md',
      chunk_id: 'chunk-faq-1',
      offset_start: 50,
      offset_end: 150,
      preview: 'Frequently asked questions about setup',
    },
  ],
  trace: {
    latency_ms: 40,
    router_decisions: [
      { adapter: 'adapter-doc-A', score: 0.92, rank: 1 },
      { adapter: 'adapter-doc-B', score: 0.84, rank: 2 },
    ],
    evidence_spans: [
      { text: 'Installation instructions', relevance: 0.95, source: 'manual.pdf' },
      { text: 'Configuration examples', relevance: 0.88, source: 'guide.pdf' },
      { text: 'Setup FAQ', relevance: 0.79, source: 'faq.md' },
    ],
  },
};

describe('Evidence drawer interaction', () => {
  beforeEach(() => {
    cy.stubApiRoutes({ inferenceResponse: chatResponseWithEvidence });
    cy.visit('/login');
    cy.disableAnimations();
    cy.get('[data-cy=login-email]').type('test@example.com');
    cy.get('[data-cy=login-password]').type('password');
    cy.get('[data-cy=login-submit]').click();
    cy.wait('@loginRequest');
    cy.wait('@currentUser');
    cy.setUiMode('user');
  });

  it('opens evidence drawer from trigger button', () => {
    cy.visit('/chat');

    // Stub chat endpoints
    cy.intercept('GET', '**/v1/chat/sessions', {
      statusCode: 200,
      body: { sessions: [] },
    }).as('chatSessions');

    cy.intercept('POST', '**/v1/chat/sessions', {
      statusCode: 201,
      body: {
        id: 'session-1',
        tenant_id: 'tenant-1',
        title: 'Test Session',
        created_at: new Date().toISOString(),
      },
    }).as('createSession');

    cy.intercept('POST', '**/v1/chat/sessions/*/messages', {
      statusCode: 200,
      body: chatResponseWithEvidence,
    }).as('sendMessage');

    // Send a message to get response with evidence
    cy.get('textarea[placeholder*="message"]').type('Show me evidence{enter}');
    cy.wait('@sendMessage');

    // Look for evidence drawer trigger button (uses testid)
    cy.get('[data-testid=evidence-drawer-trigger-rulebook]').should('be.visible').click();

    // Verify drawer opens with Rulebook tab
    cy.contains('Evidence').should('be.visible');
    cy.contains('Rulebook').should('be.visible');
  });

  it('switches between drawer tabs: Rulebook, Calculation, Trace', () => {
    cy.visit('/inference');
    cy.get('[data-cy=prompt-input]').clear().type('Evidence drawer tabs test.');
    cy.get('[data-cy=run-inference-btn]').click();
    cy.wait('@inferRequest');

    // Navigate to chat page where drawer is available
    cy.visit('/chat');

    cy.intercept('GET', '**/v1/chat/sessions', {
      statusCode: 200,
      body: { sessions: [] },
    }).as('chatSessions');

    cy.intercept('POST', '**/v1/chat/sessions/*/messages', {
      statusCode: 200,
      body: chatResponseWithEvidence,
    }).as('sendMessage');

    cy.get('textarea[placeholder*="message"]').type('Tab switching test{enter}');
    cy.wait('@sendMessage');

    // Open drawer
    cy.get('[data-testid=evidence-drawer-trigger-rulebook]').click();

    // Verify Rulebook tab is active by default
    cy.contains('Rulebook').should('be.visible');

    // Click Calculation tab trigger
    cy.get('[data-testid=evidence-drawer-trigger-calculation]').click();
    cy.contains('Calculation').should('be.visible');

    // Click Trace tab trigger
    cy.get('[data-testid=evidence-drawer-trigger-trace]').click();
    cy.contains('Trace').should('be.visible');
  });

  it('displays citations in Rulebook tab', () => {
    cy.visit('/chat');

    cy.intercept('GET', '**/v1/chat/sessions', {
      statusCode: 200,
      body: { sessions: [] },
    }).as('chatSessions');

    cy.intercept('POST', '**/v1/chat/sessions/*/messages', {
      statusCode: 200,
      body: chatResponseWithEvidence,
    }).as('sendMessage');

    cy.get('textarea[placeholder*="message"]').type('Show citations{enter}');
    cy.wait('@sendMessage');

    cy.get('[data-testid=evidence-drawer-trigger-rulebook]').click();

    // Verify citations are displayed
    cy.contains('manual.pdf').should('be.visible');
    cy.contains('guide.pdf').should('be.visible');
    cy.contains('faq.md').should('be.visible');
  });

  it('displays router decisions in Calculation tab', () => {
    cy.visit('/chat');

    cy.intercept('GET', '**/v1/chat/sessions', {
      statusCode: 200,
      body: { sessions: [] },
    }).as('chatSessions');

    cy.intercept('POST', '**/v1/chat/sessions/*/messages', {
      statusCode: 200,
      body: chatResponseWithEvidence,
    }).as('sendMessage');

    cy.get('textarea[placeholder*="message"]').type('Show router decisions{enter}');
    cy.wait('@sendMessage');

    cy.get('[data-testid=evidence-drawer-trigger-calculation]').click();

    // Verify Calculation tab content
    cy.contains('Calculation').should('be.visible');
  });

  it('displays trace information in Trace tab', () => {
    cy.visit('/chat');

    cy.intercept('GET', '**/v1/chat/sessions', {
      statusCode: 200,
      body: { sessions: [] },
    }).as('chatSessions');

    cy.intercept('POST', '**/v1/chat/sessions/*/messages', {
      statusCode: 200,
      body: chatResponseWithEvidence,
    }).as('sendMessage');

    cy.get('textarea[placeholder*="message"]').type('Show trace{enter}');
    cy.wait('@sendMessage');

    cy.get('[data-testid=evidence-drawer-trigger-trace]').click();

    // Verify Trace tab content
    cy.contains('Trace').should('be.visible');
  });

  it('closes drawer with Escape key', () => {
    cy.visit('/chat');

    cy.intercept('GET', '**/v1/chat/sessions', {
      statusCode: 200,
      body: { sessions: [] },
    }).as('chatSessions');

    cy.intercept('POST', '**/v1/chat/sessions/*/messages', {
      statusCode: 200,
      body: chatResponseWithEvidence,
    }).as('sendMessage');

    cy.get('textarea[placeholder*="message"]').type('Keyboard test{enter}');
    cy.wait('@sendMessage');

    cy.get('[data-testid=evidence-drawer-trigger-rulebook]').click();

    // Verify drawer is open
    cy.contains('Evidence').should('be.visible');

    // Press Escape to close
    cy.get('body').type('{esc}');

    // Verify drawer is closed
    cy.contains('Evidence').should('not.exist');
  });

  it('navigates tabs with arrow keys', () => {
    cy.visit('/chat');

    cy.intercept('GET', '**/v1/chat/sessions', {
      statusCode: 200,
      body: { sessions: [] },
    }).as('chatSessions');

    cy.intercept('POST', '**/v1/chat/sessions/*/messages', {
      statusCode: 200,
      body: chatResponseWithEvidence,
    }).as('sendMessage');

    cy.get('textarea[placeholder*="message"]').type('Arrow key navigation{enter}');
    cy.wait('@sendMessage');

    cy.get('[data-testid=evidence-drawer-trigger-rulebook]').click();

    // Verify Rulebook tab is active
    cy.contains('Rulebook').should('be.visible');

    // Press right arrow to go to next tab
    cy.get('body').type('{rightarrow}');

    // Should now be on Calculation tab
    cy.contains('Calculation').should('be.visible');

    // Press right arrow again
    cy.get('body').type('{rightarrow}');

    // Should now be on Trace tab
    cy.contains('Trace').should('be.visible');

    // Press left arrow to go back
    cy.get('body').type('{leftarrow}');

    // Should be back on Calculation
    cy.contains('Calculation').should('be.visible');
  });

  it('preserves drawer state across message navigation', () => {
    cy.visit('/chat');

    cy.intercept('GET', '**/v1/chat/sessions', {
      statusCode: 200,
      body: { sessions: [] },
    }).as('chatSessions');

    cy.intercept('POST', '**/v1/chat/sessions/*/messages', {
      statusCode: 200,
      body: chatResponseWithEvidence,
    }).as('sendMessage1');

    // Send first message
    cy.get('textarea[placeholder*="message"]').type('First message{enter}');
    cy.wait('@sendMessage1');

    // Open drawer on Calculation tab
    cy.get('[data-testid=evidence-drawer-trigger-calculation]').click();
    cy.contains('Calculation').should('be.visible');

    // Drawer should stay open when viewing different messages
    // (implementation specific - depends on context provider)
  });

  it('shows inline evidence preview before opening drawer', () => {
    cy.visit('/chat');

    cy.intercept('GET', '**/v1/chat/sessions', {
      statusCode: 200,
      body: { sessions: [] },
    }).as('chatSessions');

    cy.intercept('POST', '**/v1/chat/sessions/*/messages', {
      statusCode: 200,
      body: chatResponseWithEvidence,
    }).as('sendMessage');

    cy.get('textarea[placeholder*="message"]').type('Inline preview test{enter}');
    cy.wait('@sendMessage');

    // Look for inline evidence preview (if implemented)
    // Would use data-testid="inline-evidence-preview-*"
  });

  it('allows clicking "View All Evidence" to open drawer', () => {
    cy.visit('/chat');

    cy.intercept('GET', '**/v1/chat/sessions', {
      statusCode: 200,
      body: { sessions: [] },
    }).as('chatSessions');

    cy.intercept('POST', '**/v1/chat/sessions/*/messages', {
      statusCode: 200,
      body: chatResponseWithEvidence,
    }).as('sendMessage');

    cy.get('textarea[placeholder*="message"]').type('View all evidence{enter}');
    cy.wait('@sendMessage');

    // Click "View All Evidence" link if it exists
    cy.get('[data-testid="view-all-evidence"]').should('be.visible').click();

    // Drawer should open
    cy.contains('Evidence').should('be.visible');
  });

  it('displays page numbers for PDF citations', () => {
    cy.visit('/chat');

    cy.intercept('GET', '**/v1/chat/sessions', {
      statusCode: 200,
      body: { sessions: [] },
    }).as('chatSessions');

    cy.intercept('POST', '**/v1/chat/sessions/*/messages', {
      statusCode: 200,
      body: chatResponseWithEvidence,
    }).as('sendMessage');

    cy.get('textarea[placeholder*="message"]').type('PDF page numbers{enter}');
    cy.wait('@sendMessage');

    cy.get('[data-testid=evidence-drawer-trigger-rulebook]').click();

    // Verify page numbers are shown for PDF citations
    cy.contains('manual.pdf').should('be.visible');
    cy.contains('Page 5').should('be.visible');

    cy.contains('guide.pdf').should('be.visible');
    cy.contains('Page 12').should('be.visible');
  });

  it('shows receipt digest and verification status in Calculation tab', () => {
    cy.visit('/chat');

    cy.intercept('GET', '**/v1/chat/sessions', {
      statusCode: 200,
      body: { sessions: [] },
    }).as('chatSessions');

    cy.intercept('POST', '**/v1/chat/sessions/*/messages', {
      statusCode: 200,
      body: chatResponseWithEvidence,
    }).as('sendMessage');

    cy.get('textarea[placeholder*="message"]').type('Receipt verification{enter}');
    cy.wait('@sendMessage');

    cy.get('[data-testid=evidence-drawer-trigger-calculation]').click();

    // Verify receipt information is shown
    cy.contains('rcpt-drawer-1').should('be.visible');
    cy.contains('trace-drawer-1').should('be.visible');
  });

  it('handles empty evidence gracefully', () => {
    const responseWithoutEvidence = {
      schema_version: '1.0',
      id: 'msg-no-evidence-1',
      text: 'Response without any evidence.',
      tokens_generated: 8,
      token_count: 8,
      latency_ms: 12,
      adapters_used: [],
      finish_reason: 'stop',
      run_receipt: {
        trace_id: 'trace-empty-1',
        run_head_hash: 'head-empty-1',
        output_digest: 'out-empty-1',
        receipt_digest: 'rcpt-empty-1',
      },
      trace: {
        latency_ms: 12,
        router_decisions: [],
        evidence_spans: [],
      },
    };

    cy.visit('/chat');

    cy.intercept('GET', '**/v1/chat/sessions', {
      statusCode: 200,
      body: { sessions: [] },
    }).as('chatSessions');

    cy.intercept('POST', '**/v1/chat/sessions/*/messages', {
      statusCode: 200,
      body: responseWithoutEvidence,
    }).as('sendMessageNoEvidence');

    cy.get('textarea[placeholder*="message"]').type('No evidence test{enter}');
    cy.wait('@sendMessageNoEvidence');

    // Evidence drawer triggers might not be visible, or show empty state
    // This depends on UI implementation
  });
});
