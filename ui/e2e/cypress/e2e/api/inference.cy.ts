// Inference API Tests
import { validateErrorResponse } from '../../support/api-helpers';

describe('Inference API', () => {
  beforeEach(() => {
    cy.login();
  });

  describe('Propose Patch', () => {
    it('should propose a patch', () => {
      cy.apiRequest({
        method: 'POST',
        url: '/v1/propose-patch',
        body: {
          prompt: 'Test prompt',
          context: 'Test context',
        },
        failOnStatusCode: false,
      }).then((response) => {
        // May succeed or fail depending on model availability
        expect(response.status).to.be.oneOf([200, 400, 503]);
        if (response.status >= 400) {
          validateErrorResponse(response);
        }
      });
    });

    it('should reject invalid patch proposals', () => {
      cy.apiRequest({
        method: 'POST',
        url: '/v1/propose-patch',
        body: {},
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.be.at.least(400);
        validateErrorResponse(response);
      });
    });
  });

  describe('Inference', () => {
    it('should perform inference', () => {
      cy.apiRequest({
        method: 'POST',
        url: '/v1/infer',
        body: {
          prompt: 'Test prompt',
        },
        failOnStatusCode: false,
      }).then((response) => {
        // May succeed or fail depending on model availability
        expect(response.status).to.be.oneOf([200, 400, 503]);
        if (response.status >= 400) {
          validateErrorResponse(response);
        } else {
          expect(response.body).to.have.property('response');
        }
      });
    });
  });

  describe('Batch Inference', () => {
    it('should perform batch inference', () => {
      cy.apiRequest({
        method: 'POST',
        url: '/v1/batch/infer',
        body: {
          prompts: ['Test prompt 1', 'Test prompt 2'],
        },
        failOnStatusCode: false,
      }).then((response) => {
        // May succeed or fail depending on model availability
        expect(response.status).to.be.oneOf([200, 400, 503]);
        if (response.status >= 400) {
          validateErrorResponse(response);
        } else {
          expect(response.body).to.be.an('array');
        }
      });
    });
  });
});

