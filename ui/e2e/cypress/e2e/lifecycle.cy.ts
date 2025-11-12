describe('Lifecycle smoke test', () => {
  const apiBase = Cypress.env('API_BASE_URL') || 'http://localhost:8080';
  const personaPage = '/personas';

  it('validates readiness endpoints', () => {
    const endpoints = ['/healthz', '/readyz', '/metrics'];
    endpoints.forEach((endpoint) => {
      cy.request({
        url: `${apiBase}${endpoint}`,
        retryOnStatusCodeFailure: true,
        timeout: 20000,
      }).its('status').should('eq', 200);
    });
  });

  it('exercises the Persona Journey demo interactions', () => {
    cy.visit(personaPage);
    cy.contains('Persona Journey Demo').should('be.visible');
    cy.get('[data-cy=persona-stage-indicator]').should('contain', 'ML Engineer • Stage 1 of 4');
    cy.get('[data-cy=persona-previous-stage]').should('be.disabled');
    cy.get('[data-cy=persona-next-stage]').should('not.be.disabled').click();
    cy.get('[data-cy=persona-stage-indicator]').should('contain', 'Stage 2 of 4');

    cy.get('[data-cy=persona-card-devops-engineer]').click();
    cy.get('[data-cy=persona-stage-indicator]').should('contain', 'DevOps Engineer • Stage 1 of 4');
    cy.get('[data-cy=persona-previous-stage]').should('not.be.disabled').click();
    cy.get('[data-cy=persona-stage-indicator]').should('contain', 'Stage 4 of 4');
    cy.get('[data-cy=persona-next-stage]').should('not.be.disabled').click();
    cy.get('[data-cy=persona-stage-indicator]').should('contain', 'Stage 1 of 4');

    cy.contains('What Appears').should('be.visible');
    cy.contains('Why').should('be.visible');
    cy.contains('Context').should('be.visible');
  });
});
