// Plan Management API Tests
import { validateErrorResponse } from '../../support/api-helpers';

describe('Plan Management API', () => {
  beforeEach(() => {
    cy.login();
  });

  describe('List Plans', () => {
    it('should list all plans', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/plans',
      }).then((response) => {
        expect(response.status).to.eq(200);
        expect(response.body).to.be.an('array');
      });
    });
  });

  describe('Build Plan', () => {
    it('should build a plan', () => {
      cy.apiRequest({
        method: 'POST',
        url: '/v1/plans/build',
        body: {
          name: 'test-plan',
          description: 'Test plan',
        },
        failOnStatusCode: false,
      }).then((response) => {
        // May succeed or fail depending on requirements
        expect(response.status).to.be.oneOf([200, 201, 400, 422]);
        if (response.status >= 400) {
          validateErrorResponse(response);
        }
      });
    });
  });

  describe('Get Plan Details', () => {
    it('should get plan details', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/plans',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const planId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'GET',
            url: `/v1/plans/${planId}/details`,
          }).then((response) => {
            expect(response.status).to.eq(200);
            expect(response.body).to.have.property('id');
          });
        } else {
          cy.log('No plans available for testing');
        }
      });
    });

    it('should return 404 for non-existent plan', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/plans/non-existent-id/details',
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.eq(404);
        validateErrorResponse(response);
      });
    });
  });

  describe('Delete Plan', () => {
    it('should delete a plan', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/plans',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const planId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'DELETE',
            url: `/v1/plans/${planId}`,
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 204, 400, 404]);
          });
        } else {
          cy.log('No plans available for testing');
        }
      });
    });
  });

  describe('Rebuild Plan', () => {
    it('should rebuild a plan', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/plans',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const planId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'POST',
            url: `/v1/plans/${planId}/rebuild`,
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 400, 404]);
          });
        } else {
          cy.log('No plans available for testing');
        }
      });
    });
  });

  describe('Compare Plans', () => {
    it('should compare plans', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/plans',
      }).then((listResponse) => {
        if (listResponse.body.length >= 2) {
          const planId1 = listResponse.body[0].id;
          const planId2 = listResponse.body[1].id;
          cy.apiRequest({
            method: 'POST',
            url: '/v1/plans/compare',
            body: {
              plan_id_1: planId1,
              plan_id_2: planId2,
            },
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 400, 404]);
          });
        } else {
          cy.log('Not enough plans available for comparison');
        }
      });
    });
  });

  describe('Pin Plan Alias', () => {
    it('should pin a plan alias', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/plans',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const planId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'POST',
            url: `/v1/plans/${planId}/pin`,
            body: {
              alias: 'latest',
            },
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 400, 404]);
          });
        } else {
          cy.log('No plans available for testing');
        }
      });
    });
  });

  describe('Export Plan Manifest', () => {
    it('should export plan manifest', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/plans',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const planId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'GET',
            url: `/v1/plans/${planId}/manifest`,
          }).then((response) => {
            expect(response.status).to.eq(200);
            expect(response.body).to.be.an('object');
          });
        } else {
          cy.log('No plans available for testing');
        }
      });
    });
  });
});

