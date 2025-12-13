/// <reference types="cypress" />

describe('Tenant switcher', () => {
  beforeEach(() => {
    cy.stubApiRoutes();
    cy.visit('/login');
    cy.disableAnimations();
    cy.get('[data-cy=login-email]').type('test@example.com');
    cy.get('[data-cy=login-password]').type('password');
    cy.get('[data-cy=login-submit]').click();
    cy.wait('@loginRequest');
    cy.wait('@currentUser');
  });

  it('switches tenants via header control', () => {
    cy.url().should('include', '/dashboard');
    cy.get('[data-cy=tenant-switcher]').click();
    cy.get('[data-cy=tenant-option][data-tenant-id="tenant-2"]').click();
    cy.wait('@switchTenant');

    cy.boundedPoll(
      () => cy.get('[data-cy=tenant-switcher]').should('contain', 'Tenant Two'),
      { timeout: 8000, interval: 500 }
    );
  });
});
