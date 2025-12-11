/// <reference types="cypress" />
import '../../support/commands';

describe('Training lineage & trust guardrails', () => {
  beforeEach(() => {
    cy.login();
    cy.visit('/training');
  });

  it('blocks non-synthetic submission without dataset versions', () => {
    cy.get('[data-cy=new-training-job-btn]').click();
    cy.get('[data-cy=training-job-form]').should('be.visible');

    cy.get('[data-cy=submit-training-job]').click();
    cy.contains(/dataset version/i).should('be.visible');
  });

  it('shows trust-state errors returned from API', () => {
    cy.intercept('POST', '**/v1/training/start', {
      statusCode: 400,
      body: {
        schema_version: 'v1',
        error: 'dataset version dsv-9 trust_state=blocked blocks training',
        code: 'DATASET_TRUST_BLOCKED',
      },
    }).as('startTrainingBlocked');

    cy.get('[data-cy=new-training-job-btn]').click();
    cy.get('[data-cy=training-job-form]').should('be.visible');
    cy.get('[data-cy=adapter-name-input]').clear().type('adapter-trust');

    cy.get('[data-cy=submit-training-job]').click();
    cy.wait('@startTrainingBlocked');
    cy.contains(/trust_state=blocked|DATASET_TRUST_BLOCKED/i).should('be.visible');
  });
});
