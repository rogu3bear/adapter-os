// OpenAI-Compatible API Tests
import { validateErrorResponse } from '../support/api-helpers';

describe('OpenAI-Compatible API', () => {
  const apiBase = Cypress.env('API_BASE_URL') || 'http://localhost:8080';

  describe('Chat Completions', () => {
    beforeEach(() => {
      cy.login();
    });

    it('should handle chat completions with JWT auth', () => {
      cy.apiRequest({
        method: 'POST',
        url: '/v1/chat/completions',
        body: {
          model: 'default',
          messages: [
            {
              role: 'user',
              content: 'Hello, world!',
            },
          ],
        },
        failOnStatusCode: false,
      }).then((response) => {
        // May succeed or fail depending on model availability
        expect(response.status).to.be.oneOf([200, 400, 503]);
        if (response.status === 200) {
          expect(response.body).to.have.property('choices');
          expect(response.body.choices).to.be.an('array');
        } else {
          validateErrorResponse(response);
        }
      });
    });

    it('should handle chat completions with API key', () => {
      const apiKey = Cypress.env('OPENAI_API_KEY');
      if (apiKey) {
        cy.request({
          method: 'POST',
          url: `${apiBase}/v1/chat/completions`,
          headers: {
            Authorization: `Bearer ${apiKey}`,
            'Content-Type': 'application/json',
          },
          body: {
            model: 'default',
            messages: [
              {
                role: 'user',
                content: 'Hello, world!',
              },
            ],
          },
          failOnStatusCode: false,
        }).then((response) => {
          expect(response.status).to.be.oneOf([200, 400, 503]);
        });
      } else {
        cy.log('Skipping API key test - OPENAI_API_KEY not configured');
      }
    });

    it('should reject requests without authentication', () => {
      cy.request({
        method: 'POST',
        url: `${apiBase}/v1/chat/completions`,
        body: {
          model: 'default',
          messages: [
            {
              role: 'user',
              content: 'Hello, world!',
            },
          ],
        },
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.eq(401);
        validateErrorResponse(response);
      });
    });

    it('should validate request format', () => {
      cy.apiRequest({
        method: 'POST',
        url: '/v1/chat/completions',
        body: {
          model: 'default',
          // Missing messages field
        },
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.be.at.least(400);
        validateErrorResponse(response);
      });
    });
  });

  describe('List Models', () => {
    beforeEach(() => {
      cy.login();
    });

    it('should list available models', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/models',
      }).then((response) => {
        expect(response.status).to.eq(200);
        expect(response.body).to.have.property('data');
        expect(response.body.data).to.be.an('array');
      });
    });

    it('should support API key authentication', () => {
      const apiKey = Cypress.env('OPENAI_API_KEY');
      if (apiKey) {
        cy.request({
          method: 'GET',
          url: `${apiBase}/v1/models`,
          headers: {
            Authorization: `Bearer ${apiKey}`,
          },
        }).then((response) => {
          expect(response.status).to.eq(200);
        });
      } else {
        cy.log('Skipping API key test - OPENAI_API_KEY not configured');
      }
    });
  });
});

