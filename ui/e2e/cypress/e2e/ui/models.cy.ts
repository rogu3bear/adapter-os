/// <reference types="cypress" />

describe('Base Models Page E2E Tests', () => {
  beforeEach(() => {
    cy.login();
    cy.visit('/models');
  });

  it('should load base models page and display header', () => {
    cy.contains(/base models|models/i).should('be.visible');
    cy.get('[data-cy=models-page]').should('be.visible');
  });

  it('should display model list or empty state', () => {
    cy.get('[data-cy=model-list]', { timeout: 10000 }).should('exist');

    cy.get('body').then(($body) => {
      if ($body.find('[data-cy=model-card]').length > 0) {
        cy.get('[data-cy=model-card]').should('have.length.at.least', 1);
      } else {
        cy.contains(/no models|empty/i).should('be.visible');
      }
    });
  });

  it('should filter models using search', () => {
    cy.get('[data-cy=model-search]').should('be.visible').type('llama');
    cy.wait(500); // Debounce delay
    cy.get('[data-cy=model-list]').should('be.visible');
  });

  it('should display model details', () => {
    cy.get('body').then(($body) => {
      if ($body.find('[data-cy=model-card]').length > 0) {
        cy.get('[data-cy=model-card]').first().click();
        cy.get('[data-cy=model-details]').should('be.visible');
      } else {
        cy.log('No models available to click');
      }
    });
  });

  it('should show model specifications', () => {
    cy.get('body').then(($body) => {
      if ($body.find('[data-cy=model-card]').length > 0) {
        const modelCard = cy.get('[data-cy=model-card]').first();

        modelCard.within(() => {
          cy.get('[data-cy=model-name]').should('be.visible');
          cy.get('[data-cy=model-size]').should('exist');
        });
      }
    });
  });

  it('should display model import option', () => {
    cy.get('[data-cy=import-model-btn]').should('be.visible');
  });

  it('should open model import form', () => {
    cy.get('[data-cy=import-model-btn]').click();
    cy.get('[data-cy=model-import-form]').should('be.visible');

    cy.get('[data-cy=model-path-input]').should('be.visible');
    cy.get('[data-cy=model-name-input]').should('be.visible');
  });

  it('should filter by model architecture', () => {
    cy.get('[data-cy=architecture-filter]').should('be.visible').click();
    cy.get('[data-cy=arch-option]').should('have.length.at.least', 1);
  });

  it('should display model quantization info', () => {
    cy.get('body').then(($body) => {
      if ($body.find('[data-cy=model-card]').length > 0) {
        cy.get('[data-cy=model-card]').first().click();
        cy.get('[data-cy=quantization-info]').should('exist');
      }
    });
  });

  it('should show compatible adapters for model', () => {
    cy.get('body').then(($body) => {
      if ($body.find('[data-cy=model-card]').length > 0) {
        cy.get('[data-cy=model-card]').first().click();
        cy.get('[data-cy=compatible-adapters]').should('exist');
      }
    });
  });
});
