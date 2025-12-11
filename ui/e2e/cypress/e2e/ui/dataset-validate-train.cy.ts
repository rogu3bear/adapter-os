/// <reference types="cypress" />
import '../../support/commands';

describe('Dataset validate -> train gating', () => {
  beforeEach(() => {
    cy.login();
    cy.intercept('GET', '**/v1/datasets/test-dataset', {
      statusCode: 200,
      body: {
        schema_version: 'v1',
        id: 'test-dataset',
        name: 'Test Dataset',
        description: null,
        validation_status: 'valid',
        trust_state: 'unknown',
        created_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
      },
    }).as('getDataset');
    cy.intercept('POST', '**/v1/datasets/test-dataset/validate', {
      statusCode: 200,
      body: {
        schema_version: 'v1',
        dataset_id: 'test-dataset',
        is_valid: true,
        validation_status: 'valid',
        errors: [],
        trust_state: 'allowed',
      },
    }).as('validateDataset');
  });

  it('blocks training when trust is unknown and allows after validate', () => {
    cy.visit('/datasets/test-dataset');
    cy.wait('@getDataset');

    // Unknown trust blocks training button
    cy.get('[data-cy=dataset-start-training]').should('be.disabled');

    // Validate
    cy.get('[data-cy=dataset-validate]').click();
    cy.wait('@validateDataset');

    // After validation (mocked optimistic UI), training is enabled
    cy.get('[data-cy=dataset-start-training]').should('not.be.disabled');
  });
});
