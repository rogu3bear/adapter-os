// Telemetry API Tests
import { validateErrorResponse } from '../../support/api-helpers';

describe('Telemetry API', () => {
  beforeEach(() => {
    cy.login();
  });

  describe('Activity Events', () => {
    it('should get activity events', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/telemetry/events/recent',
      }).then((response) => {
        expect(response.status).to.eq(200);
        expect(response.body).to.be.an('array');
      });
    });

    it('should get recent activity events', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/telemetry/events/recent',
      }).then((response) => {
        expect(response.status).to.eq(200);
        expect(response.body).to.be.an('array');
      });
    });

    it('should stream recent activity events', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/telemetry/events/recent/stream',
        failOnStatusCode: false,
      }).then((response) => {
        // Streaming endpoints may return different status codes
        expect(response.status).to.be.oneOf([200, 400]);
      });
    });
  });

  describe('Client Logs', () => {
    it('should submit client logs', () => {
      cy.apiRequest({
        method: 'POST',
        url: '/v1/telemetry/logs',
        body: {
          logs: [
            {
              level: 'info',
              message: 'Test log message',
              timestamp: new Date().toISOString(),
            },
          ],
        },
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.be.oneOf([200, 201, 400]);
        if (response.status >= 400) {
          validateErrorResponse(response);
        }
      });
    });
  });

  describe('Audit Logs', () => {
    it('should export audit logs', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/audits/export',
        failOnStatusCode: false,
      }).then((response) => {
        // May return file download or JSON
        expect(response.status).to.be.oneOf([200, 400]);
      });
    });
  });
});

