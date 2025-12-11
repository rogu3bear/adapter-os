// Health and Readiness API Tests
import { getApiBaseUrl, validateErrorResponse } from '../../support/api-helpers';

describe('Health & Readiness API', () => {
  const apiBase = getApiBaseUrl();

  describe('Public Health Endpoints', () => {
    it('should return 200 for /healthz', () => {
      cy.request({
        method: 'GET',
        url: `${apiBase}/healthz`,
      }).then((response) => {
        expect(response.status).to.eq(200);
        expect(response.body).to.have.property('status');
      });
    });

    it('should return 200 for /readyz', () => {
      cy.request({
        method: 'GET',
        url: `${apiBase}/readyz`,
      }).then((response) => {
        expect(response.status).to.eq(200);
        expect(response.body).to.have.property('status');
      });
    });

    it('should return metadata from /v1/meta', () => {
      cy.request({
        method: 'GET',
        url: `${apiBase}/v1/meta`,
      }).then((response) => {
        expect(response.status).to.eq(200);
        expect(response.body).to.be.an('object');
      });
    });
  });

  describe('Metrics Endpoint', () => {
    it('should require bearer token for /metrics', () => {
      cy.request({
        method: 'GET',
        url: `${apiBase}/metrics`,
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.eq(401);
        validateErrorResponse(response);
      });
    });

    it('should accept valid bearer token for /metrics', () => {
      // Note: This test requires a valid metrics bearer token
      // In practice, this would be configured via environment variable
      const metricsToken = Cypress.env('METRICS_BEARER_TOKEN');
      if (metricsToken) {
        cy.request({
          method: 'GET',
          url: `${apiBase}/metrics`,
          headers: {
            Authorization: `Bearer ${metricsToken}`,
          },
        }).then((response) => {
          expect(response.status).to.eq(200);
        });
      } else {
        cy.log('Skipping metrics test - METRICS_BEARER_TOKEN not configured');
      }
    });
  });
});

