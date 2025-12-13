/// <reference types="cypress" />

describe('Policy deny controls', () => {
  beforeEach(() => {
    cy.stubApiRoutes();
    cy.visit('/login');
    cy.disableAnimations();
    cy.get('[data-cy=login-email]').type('test@example.com');
    cy.get('[data-cy=login-password]').type('password');
    cy.get('[data-cy=login-submit]').click();
    cy.wait('@loginRequest');
    cy.wait('@currentUser');
    cy.visit('/policies');
    cy.wait('@policies');
  });

  it('updates deny toggles and saves policy', () => {
    cy.contains('button', /new policy/i).click();
    cy.get('[data-cy$="-toggle"]').first().click();
    cy.get('[data-cy=policy-save-btn]').click();
    cy.wait('@policyValidate');
    cy.wait('@policySave');
  });
});
