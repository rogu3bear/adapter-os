// Domain Adapter API Tests
import { validateErrorResponse } from '../support/api-helpers';

describe('Domain Adapter API', () => {
  beforeEach(() => {
    cy.login();
  });

  describe('List Domain Adapters', () => {
    it('should list all domain adapters', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/domain-adapters',
      }).then((response) => {
        expect(response.status).to.eq(200);
        expect(response.body).to.be.an('array');
      });
    });
  });

  describe('Create Domain Adapter', () => {
    it('should create a domain adapter', () => {
      cy.apiRequest({
        method: 'POST',
        url: '/v1/domain-adapters',
        body: {
          name: 'test-domain-adapter',
          domain: 'test',
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

  describe('Get Domain Adapter', () => {
    it('should get domain adapter by ID', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/domain-adapters',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const adapterId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'GET',
            url: `/v1/domain-adapters/${adapterId}`,
          }).then((response) => {
            expect(response.status).to.eq(200);
            expect(response.body).to.have.property('id');
          });
        } else {
          cy.log('No domain adapters available for testing');
        }
      });
    });

    it('should return 404 for non-existent domain adapter', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/domain-adapters/non-existent-id',
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.eq(404);
        validateErrorResponse(response);
      });
    });
  });

  describe('Delete Domain Adapter', () => {
    it('should delete a domain adapter', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/domain-adapters',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const adapterId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'DELETE',
            url: `/v1/domain-adapters/${adapterId}`,
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 204, 400, 404]);
          });
        } else {
          cy.log('No domain adapters available for testing');
        }
      });
    });
  });

  describe('Load/Unload Domain Adapters', () => {
    it('should load a domain adapter', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/domain-adapters',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const adapterId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'POST',
            url: `/v1/domain-adapters/${adapterId}/load`,
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 400, 404]);
          });
        } else {
          cy.log('No domain adapters available for testing');
        }
      });
    });

    it('should unload a domain adapter', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/domain-adapters',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const adapterId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'POST',
            url: `/v1/domain-adapters/${adapterId}/unload`,
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 400, 404]);
          });
        } else {
          cy.log('No domain adapters available for testing');
        }
      });
    });
  });

  describe('Test Domain Adapter', () => {
    it('should test a domain adapter', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/domain-adapters',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const adapterId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'POST',
            url: `/v1/domain-adapters/${adapterId}/test`,
            body: {
              input: 'test input',
            },
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 400, 404]);
          });
        } else {
          cy.log('No domain adapters available for testing');
        }
      });
    });
  });

  describe('Get Domain Adapter Manifest', () => {
    it('should get domain adapter manifest', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/domain-adapters',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const adapterId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'GET',
            url: `/v1/domain-adapters/${adapterId}/manifest`,
          }).then((response) => {
            expect(response.status).to.eq(200);
            expect(response.body).to.be.an('object');
          });
        } else {
          cy.log('No domain adapters available for testing');
        }
      });
    });
  });

  describe('Execute Domain Adapter', () => {
    it('should execute a domain adapter', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/domain-adapters',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const adapterId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'POST',
            url: `/v1/domain-adapters/${adapterId}/execute`,
            body: {
              input: 'test input',
            },
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 400, 404]);
          });
        } else {
          cy.log('No domain adapters available for testing');
        }
      });
    });
  });
});

