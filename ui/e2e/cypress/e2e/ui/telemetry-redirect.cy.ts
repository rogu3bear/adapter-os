/// <reference types="cypress" />

describe('Telemetry redirects', () => {
  beforeEach(() => {
    cy.stubApiRoutes();
    cy.visit('/login');
    cy.disableAnimations();
    cy.get('[data-cy=login-email]').type('test@example.com');
    cy.get('[data-cy=login-password]').type('password');
    cy.get('[data-cy=login-submit]').click();
    cy.wait('@loginRequest');
    cy.wait('@currentUser');
    cy.setUiMode('user');
  });

  it('redirects /telemetry/viewer to the telemetry shell', () => {
    cy.visit('/telemetry/viewer');
    cy.location('pathname').should('include', '/telemetry');
    cy.location('search').should('include', 'tab=viewer');
  });
});
