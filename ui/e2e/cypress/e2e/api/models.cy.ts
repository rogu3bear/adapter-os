// Model Management API Tests
import { validateErrorResponse } from '../../support/api-helpers';

describe('Model Management API', () => {
  beforeEach(() => {
    cy.login();
  });

  describe('Import Model', () => {
    it('should import a model', () => {
      cy.apiRequest({
        method: 'POST',
        url: '/v1/models/import',
        body: {
          model_id: 'test-model',
          path: '/path/to/model',
        },
        failOnStatusCode: false,
      }).then((response) => {
        // May succeed or fail depending on model availability and permissions
        expect(response.status).to.be.oneOf([200, 201, 400, 403, 404]);
        if (response.status >= 400) {
          validateErrorResponse(response);
        }
      });
    });
  });

  describe('Get Model Status', () => {
    it('should get base model status', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/models/status',
      }).then((response) => {
        expect(response.status).to.eq(200);
        expect(response.body).to.be.an('object');
      });
    });

    it('should get all models status', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/models/status/all',
      }).then((response) => {
        expect(response.status).to.eq(200);
        expect(response.body).to.be.an('object');
      });
    });
  });

  describe('Model Import Status', () => {
    it('should get import status', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/models/import/status',
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.be.oneOf([200, 404]);
        if (response.status === 200) {
          expect(response.body).to.be.an('object');
        }
      });
    });
  });

  describe('Cursor Config', () => {
    it('should get cursor configuration', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/models/cursor-config',
      }).then((response) => {
        expect(response.status).to.eq(200);
        expect(response.body).to.be.an('object');
      });
    });
  });

  describe('Model Diagnostics', () => {
    it('should get model diagnostics', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/models/diagnostics',
      }).then((response) => {
        expect(response.status).to.eq(200);
        expect(response.body).to.be.an('object');
      });
    });
  });
});

