/// <reference types="cypress" />

describe('Inference Page E2E Tests', () => {
  beforeEach(() => {
    cy.login();
    cy.visit('/inference');
  });

  it('should load inference page and display header', () => {
    cy.contains(/inference|playground/i).should('be.visible');
    cy.get('[data-cy=inference-page]').should('be.visible');
  });

  it('should display model selector', () => {
    cy.get('[data-cy=model-selector]').should('be.visible');
  });

  it('should display adapter selector', () => {
    cy.get('[data-cy=adapter-selector]').should('be.visible');
  });

  it('should show prompt input area', () => {
    cy.get('[data-cy=prompt-input]').should('be.visible');
  });

  it('should have run inference button', () => {
    cy.get('[data-cy=run-inference-btn]').should('be.visible');
  });

  it('should configure inference parameters', () => {
    cy.get('[data-cy=parameters-toggle]').click();

    cy.get('[data-cy=temperature-slider]').should('be.visible');
    cy.get('[data-cy=max-tokens-input]').should('be.visible');
    cy.get('[data-cy=top-p-slider]').should('be.visible');
  });

  it('should validate prompt input', () => {
    cy.get('[data-cy=run-inference-btn]').click();

    // Should show validation error for empty prompt
    cy.contains(/enter.*prompt|prompt.*required/i).should('be.visible');
  });

  it('should run inference with valid input', () => {
    // Select a model
    cy.get('[data-cy=model-selector]').click();
    cy.get('[data-cy=model-option]').first().click();

    // Enter a prompt
    cy.get('[data-cy=prompt-input]').type('Hello, how are you?');

    // Run inference
    cy.get('[data-cy=run-inference-btn]').click();

    // Should show loading state or results
    cy.get('[data-cy=inference-output]', { timeout: 30000 }).should('exist');
  });

  it('should display inference results', () => {
    cy.get('[data-cy=inference-history]').should('exist');
  });

  it('should show token usage statistics', () => {
    cy.get('body').then(($body) => {
      if ($body.find('[data-cy=inference-result]').length > 0) {
        cy.get('[data-cy=inference-result]').first().within(() => {
          cy.get('[data-cy=token-usage]').should('exist');
        });
      }
    });
  });

  it('should display inference latency', () => {
    cy.get('body').then(($body) => {
      if ($body.find('[data-cy=inference-result]').length > 0) {
        cy.get('[data-cy=inference-result]').first().within(() => {
          cy.get('[data-cy=latency]').should('exist');
        });
      }
    });
  });

  it('should allow clearing prompt input', () => {
    cy.get('[data-cy=prompt-input]').type('Test prompt');
    cy.get('[data-cy=clear-prompt-btn]').should('be.visible').click();
    cy.get('[data-cy=prompt-input]').should('have.value', '');
  });

  it('should show streaming toggle', () => {
    cy.get('[data-cy=streaming-toggle]').should('exist');
  });

  it('should support multi-adapter selection', () => {
    cy.get('[data-cy=adapter-selector]').click();
    cy.get('[data-cy=adapter-multi-select]').should('exist');
  });

  it('should display K-sparse routing info', () => {
    cy.get('[data-cy=routing-info]').should('exist');
  });

  it('should show inference history', () => {
    cy.get('[data-cy=history-tab]').click();
    cy.get('[data-cy=inference-history-list]').should('be.visible');
  });
});
