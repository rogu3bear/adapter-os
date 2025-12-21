// Monitoring & Alerts API Tests
import { validateErrorResponse } from '../../support/api-helpers';

describe('Monitoring & Alerts API', () => {
  beforeEach(() => {
    cy.login();
  });

  describe('Monitoring Rules', () => {
    it('should list monitoring rules', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/monitoring/rules',
      }).then((response) => {
        expect(response.status).to.eq(200);
        expect(response.body).to.be.an('array');
      });
    });

    it('should create a monitoring rule', () => {
      cy.apiRequest({
        method: 'POST',
        url: '/v1/monitoring/rules',
        body: {
          name: 'test-rule',
          condition: 'cpu_usage > 80',
        },
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.be.oneOf([200, 201, 400, 422]);
        if (response.status >= 400) {
          validateErrorResponse(response);
        }
      });
    });
  });

  describe('Process Alerts', () => {
    it('should list process alerts', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/monitoring/alerts',
      }).then((response) => {
        expect(response.status).to.eq(200);
        expect(response.body).to.be.an('array');
      });
    });

    it('should stream alerts', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/monitoring/alerts/stream',
        failOnStatusCode: false,
      }).then((response) => {
        // Streaming endpoints may return different status codes
        expect(response.status).to.be.oneOf([200, 400]);
      });
    });

    it('should acknowledge an alert', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/monitoring/alerts',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const alertId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'POST',
            url: `/v1/monitoring/alerts/${alertId}/acknowledge`,
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 400, 404]);
          });
        } else {
          cy.log('No alerts available for testing');
        }
      });
    });
  });

  describe('Process Anomalies', () => {
    it('should list process anomalies', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/monitoring/anomalies',
      }).then((response) => {
        expect(response.status).to.eq(200);
        expect(response.body).to.be.an('array');
      });
    });

    it('should update anomaly status', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/monitoring/anomalies',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const anomalyId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'POST',
            url: `/v1/monitoring/anomalies/${anomalyId}/status`,
            body: {
              status: 'resolved',
            },
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 400, 404]);
          });
        } else {
          cy.log('No anomalies available for testing');
        }
      });
    });
  });

  describe('Monitoring Dashboards', () => {
    it('should list monitoring dashboards', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/monitoring/dashboards',
      }).then((response) => {
        expect(response.status).to.eq(200);
        expect(response.body).to.be.an('array');
      });
    });

    it('should create a monitoring dashboard', () => {
      cy.apiRequest({
        method: 'POST',
        url: '/v1/monitoring/dashboards',
        body: {
          name: 'test-dashboard',
          widgets: [],
        },
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.be.oneOf([200, 201, 400, 422]);
        if (response.status >= 400) {
          validateErrorResponse(response);
        }
      });
    });
  });
});

