/// <reference types="cypress" />

describe('Auth flow', () => {
  beforeEach(() => {
    cy.stubApiRoutes();
    cy.visit('/login');
    cy.disableAnimations();
  });

  it('signs in with stubbed API and shows tenant switcher', () => {
    cy.get('[data-cy=login-email]').type('test@example.com');
    cy.get('[data-cy=login-password]').type('password');

    cy.get('[data-cy=login-submit]').click();
    cy.wait('@loginRequest');
    cy.wait('@currentUser');

    cy.url().should('include', '/dashboard');
    cy.get('[data-cy=tenant-switcher]').should('be.visible');
  });
});
