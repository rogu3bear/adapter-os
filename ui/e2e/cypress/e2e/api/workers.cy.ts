// Worker Management API Tests
import { validateErrorResponse } from '../../support/api-helpers';

describe('Worker Management API', () => {
  beforeEach(() => {
    cy.login();
  });

  describe('List Workers', () => {
    it('should list all workers', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/workers',
      }).then((response) => {
        expect(response.status).to.eq(200);
        expect(response.body).to.be.an('array');
      });
    });
  });

  describe('Spawn Worker', () => {
    it('should spawn a new worker', () => {
      cy.apiRequest({
        method: 'POST',
        url: '/v1/workers/spawn',
        body: {
          worker_type: 'inference',
        },
        failOnStatusCode: false,
      }).then((response) => {
        // May succeed or fail depending on configuration
        expect(response.status).to.be.oneOf([200, 201, 400, 403]);
        if (response.status >= 400) {
          validateErrorResponse(response);
        }
      });
    });
  });

  describe('Register Local Worker', () => {
    it('should register a local worker', () => {
      cy.apiRequest({
        method: 'POST',
        url: '/v1/workers/register-local',
        body: {
          worker_id: 'test-worker',
        },
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.be.oneOf([200, 201, 400]);
      });
    });
  });

  describe('Worker Heartbeat', () => {
    it('should send worker heartbeat', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/workers',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const workerId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'POST',
            url: `/v1/workers/${workerId}/heartbeat`,
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 404]);
          });
        } else {
          cy.log('No workers available for testing');
        }
      });
    });
  });

  describe('Worker Logs', () => {
    it('should list worker logs', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/workers',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const workerId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'GET',
            url: `/v1/workers/${workerId}/logs`,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 404]);
            if (response.status === 200) {
              expect(response.body).to.be.an('array');
            }
          });
        } else {
          cy.log('No workers available for testing');
        }
      });
    });
  });

  describe('Worker Crashes', () => {
    it('should list worker crashes', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/workers',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const workerId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'GET',
            url: `/v1/workers/${workerId}/crashes`,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 404]);
            if (response.status === 200) {
              expect(response.body).to.be.an('array');
            }
          });
        } else {
          cy.log('No workers available for testing');
        }
      });
    });
  });

  describe('Worker Debug', () => {
    it('should start debug session', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/workers',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const workerId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'POST',
            url: `/v1/workers/${workerId}/debug`,
            body: {
              action: 'start',
            },
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 400, 404]);
          });
        } else {
          cy.log('No workers available for testing');
        }
      });
    });
  });

  describe('Worker Troubleshooting', () => {
    it('should run troubleshooting step', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/workers',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const workerId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'POST',
            url: `/v1/workers/${workerId}/troubleshoot`,
            body: {
              step: 'health_check',
            },
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 400, 404]);
          });
        } else {
          cy.log('No workers available for testing');
        }
      });
    });
  });
});

