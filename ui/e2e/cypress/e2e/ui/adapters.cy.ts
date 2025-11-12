/// <reference types="cypress" />

describe('Adapters Page UI Tests', () => {
  beforeEach(() => {
    cy.login();
    cy.visit('/adapters');
  });

  describe('Page Load and Navigation', () => {
    it('should load adapters page successfully', () => {
      cy.url().should('include', '/adapters');
      cy.contains('Adapters').should('be.visible');
    });

    it('should display page header', () => {
      cy.get('[data-cy=page-header]').should('be.visible');
    });

    it('should have breadcrumb navigation', () => {
      cy.get('[data-cy=breadcrumb]').should('be.visible');
    });
  });

  describe('Adapters List Display', () => {
    it('should display adapters list or empty state', () => {
      cy.get('body').then(($body) => {
        if ($body.find('[data-cy=adapters-list]').length > 0) {
          cy.get('[data-cy=adapters-list]').should('be.visible');
        } else {
          cy.get('[data-cy=empty-state]').should('be.visible');
        }
      });
    });

    it('should display adapter cards with key information', () => {
      cy.get('[data-cy=adapter-card]').first().within(() => {
        cy.get('[data-cy=adapter-name]').should('be.visible');
        cy.get('[data-cy=adapter-id]').should('be.visible');
        cy.get('[data-cy=adapter-status]').should('be.visible');
      });
    });
  });

  describe('Register New Adapter', () => {
    it('should open adapter registration dialog', () => {
      cy.get('[data-cy=register-adapter-button]').click();
      cy.get('[data-cy=adapter-registration-dialog]').should('be.visible');
    });
  });

  describe('Adapter Actions', () => {
    it('should load adapter', () => {
      cy.get('[data-cy=adapter-card]').first().within(() => {
        cy.get('[data-cy=adapter-actions]').click();
        cy.get('[data-cy=load-adapter]').click();
      });
      cy.get('[data-cy=success-message]').should('be.visible');
    });
  });
});
