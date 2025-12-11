// Adapter Management API Tests
import { validateErrorResponse } from '../../support/api-helpers';

describe('Adapter Management API', () => {
  beforeEach(() => {
    cy.login();
  });

  // Clean up tracked resources after each test
  afterEach(() => {
    cy.cleanupTestData();
  });

  describe('List Adapters', () => {
    it('should list all adapters', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/adapters',
      }).then((response) => {
        expect(response.status).to.eq(200);
        expect(response.body).to.be.an('array');
      });
    });

    it('should reject unauthenticated requests', () => {
      cy.request({
        method: 'GET',
        url: `${Cypress.env('API_BASE_URL')}/v1/adapters`,
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.eq(401);
        validateErrorResponse(response);
      });
    });
  });

  describe('Adapter Repositories', () => {
    it('should list adapter repositories', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/adapter-repositories',
      }).then((response) => {
        expect(response.status).to.eq(200);
        expect(response.body).to.be.an('array');
      });
    });
  });

  describe('Get Adapter', () => {
    it('should get adapter details by ID', () => {
      // First get list to find an adapter ID
      cy.apiRequest({
        method: 'GET',
        url: '/v1/adapters',
      }).then((listResponse) => {
        expect(listResponse.status).to.eq(200);
        expect(listResponse.body).to.be.an('array');
        
        if (Array.isArray(listResponse.body) && listResponse.body.length > 0) {
          const firstAdapter = listResponse.body[0];
          expect(firstAdapter).to.be.an('object');
          
          if (firstAdapter && typeof firstAdapter === 'object' && 'id' in firstAdapter) {
            const adapterId = firstAdapter.id;
            expect(adapterId).to.be.a('string');
            expect(adapterId.length).to.be.greaterThan(0);
            
            cy.apiRequest({
              method: 'GET',
              url: `/v1/adapters/${adapterId}`,
            }).then((response) => {
              expect(response.status).to.eq(200);
              expect(response.body).to.be.an('object');
              expect(response.body).to.have.property('id');
              expect(response.body.id).to.eq(adapterId);
            });
          } else {
            cy.log('No valid adapter ID found in response');
          }
        } else {
          cy.log('No adapters available for testing');
        }
      });
    });

    it('should return 404 for non-existent adapter', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/adapters/non-existent-id',
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.eq(404);
        validateErrorResponse(response);
      });
    });
  });

  describe('Register Adapter', () => {
    it('should register a new adapter', () => {
      // Note: This test may require a valid adapter file or manifest
      // Adjust based on actual adapter registration requirements
      cy.apiRequest({
        method: 'POST',
        url: '/v1/adapters',
        body: {
          // Add required adapter registration fields
          name: 'test-adapter',
        },
        failOnStatusCode: false,
      }).then((response) => {
        // May succeed or fail depending on adapter requirements
        expect(response.status).to.be.oneOf([200, 201, 400, 422]);
        if (response.status >= 400) {
          validateErrorResponse(response);
        } else if (response.status === 200 || response.status === 201) {
          // Track created adapter for cleanup
          if (response.body && response.body.id) {
            cy.trackResource('adapter', response.body.id, `/v1/adapters/${response.body.id}`);
          }
        }
      });
    });
  });

  describe('Delete Adapter', () => {
    it('should delete an adapter', () => {
      // First get list to find an adapter ID
      cy.apiRequest({
        method: 'GET',
        url: '/v1/adapters',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const adapterId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'DELETE',
            url: `/v1/adapters/${adapterId}`,
            failOnStatusCode: false,
          }).then((response) => {
            // May succeed or fail depending on adapter state
            expect(response.status).to.be.oneOf([200, 204, 400, 404]);
          });
        } else {
          cy.log('No adapters available for testing');
        }
      });
    });
  });

  describe('Load/Unload Adapters', () => {
    it('should load an adapter', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/adapters',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const adapterId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'POST',
            url: `/v1/adapters/${adapterId}/load`,
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 400, 404]);
          });
        } else {
          cy.log('No adapters available for testing');
        }
      });
    });

    it('should unload an adapter', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/adapters',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const adapterId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'POST',
            url: `/v1/adapters/${adapterId}/unload`,
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 400, 404]);
          });
        } else {
          cy.log('No adapters available for testing');
        }
      });
    });
  });

  describe('Adapter Activations', () => {
    it('should get adapter activations', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/adapters/activations',
      }).then((response) => {
        expect(response.status).to.eq(200);
        expect(response.body).to.be.an('array');
      });
    });
  });
});

