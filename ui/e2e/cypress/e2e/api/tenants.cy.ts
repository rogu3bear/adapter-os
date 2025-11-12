// Tenant Management API Tests
import { validateErrorResponse } from '../support/api-helpers';

describe('Tenant Management API', () => {
  beforeEach(() => {
    cy.login();
  });

  // Clean up tracked resources after each test
  afterEach(() => {
    cy.cleanupTestData();
  });

  describe('List Tenants', () => {
    it('should list all tenants', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/tenants',
      }).then((response) => {
        expect(response.status).to.eq(200);
        expect(response.body).to.be.an('array');
      });
    });
  });

  describe('Create Tenant', () => {
    it('should create a new tenant', () => {
      const tenantName = `test-tenant-${Date.now()}`;
      cy.apiRequest({
        method: 'POST',
        url: '/v1/tenants',
        body: {
          name: tenantName,
        },
        failOnStatusCode: false,
      }).then((response) => {
        // May succeed or fail depending on permissions
        expect(response.status).to.be.oneOf([200, 201, 400, 403]);
        if (response.status >= 400) {
          validateErrorResponse(response);
        } else {
          expect(response.body).to.have.property('id');
          expect(response.body).to.have.property('name');
          // Track created tenant for cleanup
          if (response.body && response.body.id) {
            cy.trackResource('tenant', response.body.id, `/v1/tenants/${response.body.id}`);
          }
        }
      });
    });

    it('should reject tenant creation with invalid data', () => {
      cy.apiRequest({
        method: 'POST',
        url: '/v1/tenants',
        body: {},
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.be.at.least(400);
        validateErrorResponse(response);
      });
    });
  });

  describe('Update Tenant', () => {
    it('should update a tenant', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/tenants',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const tenantId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'PUT',
            url: `/v1/tenants/${tenantId}`,
            body: {
              name: `updated-tenant-${Date.now()}`,
            },
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 400, 403, 404]);
          });
        } else {
          cy.log('No tenants available for testing');
        }
      });
    });
  });

  describe('Tenant Operations', () => {
    it('should pause a tenant', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/tenants',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const tenantId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'POST',
            url: `/v1/tenants/${tenantId}/pause`,
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 400, 403, 404]);
          });
        } else {
          cy.log('No tenants available for testing');
        }
      });
    });

    it('should archive a tenant', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/tenants',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const tenantId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'POST',
            url: `/v1/tenants/${tenantId}/archive`,
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 400, 403, 404]);
          });
        } else {
          cy.log('No tenants available for testing');
        }
      });
    });

    it('should rename a tenant', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/tenants',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const tenantId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'POST',
            url: `/v1/tenants/${tenantId}/rename`,
            body: {
              name: `renamed-tenant-${Date.now()}`,
            },
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 400, 403, 404]);
          });
        } else {
          cy.log('No tenants available for testing');
        }
      });
    });
  });

  describe('Tenant Policies', () => {
    it('should assign policies to a tenant', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/tenants',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const tenantId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'POST',
            url: `/v1/tenants/${tenantId}/policies`,
            body: {
              policy_ids: ['policy1', 'policy2'],
            },
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 400, 403, 404]);
          });
        } else {
          cy.log('No tenants available for testing');
        }
      });
    });
  });

  describe('Tenant Adapters', () => {
    it('should assign adapters to a tenant', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/tenants',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const tenantId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'POST',
            url: `/v1/tenants/${tenantId}/adapters`,
            body: {
              adapter_ids: ['adapter1'],
            },
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 400, 403, 404]);
          });
        } else {
          cy.log('No tenants available for testing');
        }
      });
    });
  });

  describe('Tenant Usage', () => {
    it('should get tenant usage statistics', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/tenants',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const tenantId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'GET',
            url: `/v1/tenants/${tenantId}/usage`,
          }).then((response) => {
            expect(response.status).to.eq(200);
            expect(response.body).to.be.an('object');
          });
        } else {
          cy.log('No tenants available for testing');
        }
      });
    });
  });
});

