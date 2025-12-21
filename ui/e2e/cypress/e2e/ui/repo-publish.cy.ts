/// <reference types="cypress" />
import '../../support/commands';

describe('Repo publish happy path', () => {
  beforeEach(() => {
    cy.login();
    cy.intercept('GET', '**/v1/repos/repo-1', {
      statusCode: 200,
      body: {
        id: 'repo-1',
        name: 'Repo One',
        base_model: 'qwen',
        default_branch: 'main',
        status: 'healthy',
        branches: [
          { name: 'main', default: true, latest_active_version: null },
        ],
        created_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
      },
    }).as('getRepo');

    cy.intercept('GET', '**/v1/repos/repo-1/versions', {
      statusCode: 200,
      body: [
        {
          id: 'v-ready',
          version: '1.0.0',
          branch: 'main',
          release_state: 'ready',
          serveable: true,
          created_at: new Date().toISOString(),
        },
      ],
    }).as('getRepoVersions');

    cy.intercept('POST', '**/v1/adapter-versions/v-ready/promote', {
      statusCode: 204,
      body: {},
    }).as('promoteVersion');
  });

  it('promotes a ready and serveable version', () => {
    cy.visit('/repos/repo-1');
    cy.wait(['@getRepo', '@getRepoVersions']);

    cy.get('[data-cy=version-promote-v-ready]').click();
    cy.wait('@promoteVersion');

    // Expect toast or disabled state cleared; minimal assertion is that request was made
    cy.get('@promoteVersion').its('response.statusCode').should('eq', 204);
  });
});
