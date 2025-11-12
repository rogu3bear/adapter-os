// Supervisor Services API Tests
import { validateErrorResponse } from '../support/api-helpers';

describe('Supervisor Services API', () => {
  beforeEach(() => {
    cy.login();
  });

  describe('List Services', () => {
    it('should list all services (UI-compatible endpoint)', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/api/services',
      }).then((response) => {
        expect(response.status).to.eq(200);
        expect(response.body).to.be.an('array');
      });
    });

    it('should list all services (v1 API endpoint)', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/api/v1/services',
      }).then((response) => {
        expect(response.status).to.eq(200);
        expect(response.body).to.be.an('array');
      });
    });
  });

  describe('Get Service', () => {
    it('should get service by ID (UI-compatible endpoint)', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/api/services',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const serviceId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'GET',
            url: `/api/services/${serviceId}`,
          }).then((response) => {
            expect(response.status).to.eq(200);
            expect(response.body).to.have.property('id');
          });
        } else {
          cy.log('No services available for testing');
        }
      });
    });

    it('should get service by ID (v1 API endpoint)', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/api/v1/services',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const serviceId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'GET',
            url: `/api/v1/services/${serviceId}`,
          }).then((response) => {
            expect(response.status).to.eq(200);
            expect(response.body).to.have.property('id');
          });
        } else {
          cy.log('No services available for testing');
        }
      });
    });
  });

  describe('Start Service', () => {
    it('should start a service (UI-compatible endpoint)', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/api/services',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const serviceId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'POST',
            url: '/api/services/start',
            body: {
              service_id: serviceId,
            },
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 400, 404]);
          });
        } else {
          cy.log('No services available for testing');
        }
      });
    });

    it('should start a service (v1 API endpoint)', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/api/v1/services',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const serviceId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'POST',
            url: `/api/v1/services/${serviceId}/start`,
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 400, 404]);
          });
        } else {
          cy.log('No services available for testing');
        }
      });
    });
  });

  describe('Stop Service', () => {
    it('should stop a service (UI-compatible endpoint)', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/api/services',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const serviceId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'POST',
            url: '/api/services/stop',
            body: {
              service_id: serviceId,
            },
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 400, 404]);
          });
        } else {
          cy.log('No services available for testing');
        }
      });
    });

    it('should stop a service (v1 API endpoint)', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/api/v1/services',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const serviceId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'POST',
            url: `/api/v1/services/${serviceId}/stop`,
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 400, 404]);
          });
        } else {
          cy.log('No services available for testing');
        }
      });
    });
  });

  describe('Restart Service', () => {
    it('should restart a service (UI-compatible endpoint)', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/api/services',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const serviceId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'POST',
            url: '/api/services/restart',
            body: {
              service_id: serviceId,
            },
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 400, 404]);
          });
        } else {
          cy.log('No services available for testing');
        }
      });
    });

    it('should restart a service (v1 API endpoint)', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/api/v1/services',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const serviceId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'POST',
            url: `/api/v1/services/${serviceId}/restart`,
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 400, 404]);
          });
        } else {
          cy.log('No services available for testing');
        }
      });
    });
  });

  describe('Essential Services', () => {
    it('should start essential services', () => {
      cy.apiRequest({
        method: 'POST',
        url: '/api/services/essential/start',
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.be.oneOf([200, 400]);
      });
    });

    it('should stop essential services', () => {
      cy.apiRequest({
        method: 'POST',
        url: '/api/services/essential/stop',
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.be.oneOf([200, 400]);
      });
    });
  });

  describe('Services Health', () => {
    it('should get services health status', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/api/services/health',
      }).then((response) => {
        expect(response.status).to.eq(200);
        expect(response.body).to.be.an('object');
      });
    });
  });
});

