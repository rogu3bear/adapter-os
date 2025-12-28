/// <reference types="cypress" />

const apiBase = Cypress.env('API_BASE_URL') || 'http://localhost:8080';
const fixturePath = 'e2e/cypress/fixtures/documents/sample-doc.md';
const documentName = 'sample-doc.md';

describe('Documents Page - Upload and List', () => {
  let uploadedDocId: string | undefined;

  before(() => {
    // No-op: backend runs with AOS_DEV_NO_AUTH=1 in dev, so auth is bypassed.
  });

  beforeEach(() => {
    // Wait for backend to be healthy before navigating
    cy.request({
      url: '/healthz',
      retryOnStatusCodeFailure: false,
      retryOnNetworkFailure: true,
      timeout: 30000,
      failOnStatusCode: false,
    });
    cy.visit('/documents');
  });

  afterEach(() => {
    if (uploadedDocId) {
      cy.request({
        method: 'DELETE',
        url: `${apiBase}/v1/documents/${uploadedDocId}`,
        failOnStatusCode: false,
      }).then(() => {
        uploadedDocId = undefined;
      });
    }
  });

  it('uploads a document and shows it in the table', () => {
    cy.intercept('POST', '**/v1/documents/upload').as('uploadDocument');

    cy.get('[data-cy=document-dropzone]').should('be.visible');
    cy.get('[data-cy=document-file-input]').selectFile(fixturePath, {
      force: true,
      subjectType: 'input',
    });
    cy.get('[data-cy=document-upload-button]').should('be.enabled').click();

    cy.wait('@uploadDocument').then(({ response }) => {
      cy.log(`upload status ${response?.statusCode}`);
      cy.log(`upload body ${JSON.stringify(response?.body)}`);
      expect(response?.statusCode).to.eq(200);
      uploadedDocId = (response?.body as { document_id?: string })?.document_id;
      expect(uploadedDocId, 'document id from upload response').to.be.a('string');
    });

    cy.contains('[data-cy=document-row]', documentName, { timeout: 20000 }).should('be.visible');
    cy.contains('[data-cy=document-row]', documentName).should('contain.text', 'Processing');
  });
});

