/// <reference types="cypress" />

describe('Dashboard E2E Test', () => {
  beforeEach(() => {
    cy.login(); // Login first
    cy.visit('/dashboard'); // Then visit protected route
  });

  it('should load dashboard and interact with persona slider', () => {
    cy.contains('Dashboard').should('be.visible');
    cy.get('[data-cy=persona-stage-indicator]').should('be.visible');
    cy.get('[data-cy=persona-next-stage]').click();
    cy.get('[data-cy=persona-stage-indicator]').should('contain', 'Stage 2');
    cy.get('[data-cy=persona-card-devops-engineer]').click();
    cy.get('[data-cy=persona-stage-indicator]').should('contain', 'DevOps Engineer');
  });

  it('should display widgets and metrics', () => {
    cy.get('[data-cy=system-health-widget]').should('be.visible');
    cy.get('[data-cy=active-alerts-widget]').should('be.visible');
    cy.get('[data-cy=realtime-metrics-widget]').should('be.visible');
  });

  it('should navigate to other pages from dashboard', () => {
    cy.get('[data-cy=nav-adapters]').click();
    cy.url().should('include', '/adapters');
    cy.go('back');
    cy.get('[data-cy=nav-tenants]').click();
    cy.url().should('include', '/tenants');
  });
});
