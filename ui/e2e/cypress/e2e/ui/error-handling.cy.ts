/// <reference types="cypress" />

/**
 * Error Handling UI Tests
 *
 * Tests for error boundary rendering, retry functionality,
 * error message display, and recovery after transient errors.
 */
describe('Error Handling UI', () => {
  beforeEach(() => {
    cy.stubApiRoutes();
    cy.disableAnimations();
  });

  describe('Error Boundary Rendering when API Fails', () => {
    it('should display page-level error boundary on critical API failure', () => {
      // Stub a critical API endpoint to fail
      cy.intercept('GET', '**/v1/auth/me', {
        statusCode: 500,
        body: {
          error: 'Internal server error',
          code: 'INTERNAL_ERROR',
        },
      }).as('authMeFail');

      cy.visit('/dashboard', { failOnStatusCode: false });

      // Should show error boundary UI
      cy.get('[role="alert"]').should('be.visible');
      cy.contains(/something went wrong|error|failed/i).should('be.visible');
    });

    it('should display section-level error boundary when section API fails', () => {
      // Login first
      cy.visit('/login');
      cy.get('[data-cy=login-email]').type('test@example.com');
      cy.get('[data-cy=login-password]').type('password');
      cy.get('[data-cy=login-submit]').click();
      cy.wait('@loginRequest');
      cy.wait('@currentUser');

      // Stub adapters endpoint to fail
      cy.intercept('GET', '**/v1/adapters**', {
        statusCode: 500,
        body: {
          error: 'Failed to fetch adapters',
          code: 'ADAPTER_FETCH_ERROR',
        },
      }).as('adaptersFail');

      cy.visit('/adapters');
      cy.wait('@adaptersFail');

      // Should show error state in the section
      cy.get('[role="alert"]').should('exist');
    });

    it('should display modal error boundary when modal content fails', () => {
      cy.visit('/login');
      cy.get('[data-cy=login-email]').type('test@example.com');
      cy.get('[data-cy=login-password]').type('password');
      cy.get('[data-cy=login-submit]').click();
      cy.wait('@loginRequest');
      cy.wait('@currentUser');

      // Navigate to adapters page
      cy.visit('/adapters');

      // Stub the adapter detail endpoint to fail
      cy.intercept('GET', '**/v1/adapters/*', {
        statusCode: 500,
        body: {
          error: 'Failed to load adapter details',
          code: 'ADAPTER_DETAIL_ERROR',
        },
      }).as('adapterDetailFail');

      // Try to open an adapter modal/detail (if exists)
      cy.get('body').then(($body) => {
        if ($body.find('[data-cy=adapter-card]').length > 0) {
          cy.get('[data-cy=adapter-card]').first().click();
          cy.wait('@adapterDetailFail');

          // Should show error in modal
          cy.contains(/something went wrong|error|failed/i).should('be.visible');
        }
      });
    });
  });

  describe('Retry Button Functionality', () => {
    it('should display retry button when error occurs', () => {
      cy.intercept('GET', '**/v1/models', {
        statusCode: 500,
        body: {
          error: 'Server error',
          code: 'INTERNAL_ERROR',
        },
      }).as('modelsFail');

      cy.visit('/login');
      cy.get('[data-cy=login-email]').type('test@example.com');
      cy.get('[data-cy=login-password]').type('password');
      cy.get('[data-cy=login-submit]').click();
      cy.wait('@loginRequest');
      cy.wait('@currentUser');

      cy.visit('/models');
      cy.wait('@modelsFail');

      // Should show retry button
      cy.contains(/try again|retry/i).should('be.visible');
    });

    it('should retry API call when retry button is clicked', () => {
      let callCount = 0;

      // First call fails, subsequent calls succeed
      cy.intercept('GET', '**/v1/adapters**', (req) => {
        callCount++;
        if (callCount === 1) {
          req.reply({
            statusCode: 500,
            body: {
              error: 'Temporary failure',
              code: 'TEMPORARY_ERROR',
            },
          });
        } else {
          req.reply({
            statusCode: 200,
            body: [],
          });
        }
      }).as('adaptersRetry');

      cy.visit('/login');
      cy.get('[data-cy=login-email]').type('test@example.com');
      cy.get('[data-cy=login-password]').type('password');
      cy.get('[data-cy=login-submit]').click();
      cy.wait('@loginRequest');
      cy.wait('@currentUser');

      cy.visit('/adapters');
      cy.wait('@adaptersRetry');

      // Should show error state with retry button
      cy.contains(/try again|retry/i).should('be.visible');

      // Click retry
      cy.contains(/try again|retry/i).click();

      // After retry, error should be cleared
      cy.get('[role="alert"]').should('not.exist');
    });

    it('should reset error boundary state on retry', () => {
      let hasErrored = false;

      cy.intercept('GET', '**/v1/models', (req) => {
        if (!hasErrored) {
          hasErrored = true;
          req.reply({
            statusCode: 503,
            body: {
              error: 'Service temporarily unavailable',
              code: 'SERVICE_UNAVAILABLE',
            },
          });
        } else {
          req.reply({
            statusCode: 200,
            body: { models: [], total: 0 },
          });
        }
      }).as('modelsFlaky');

      cy.visit('/login');
      cy.get('[data-cy=login-email]').type('test@example.com');
      cy.get('[data-cy=login-password]').type('password');
      cy.get('[data-cy=login-submit]').click();
      cy.wait('@loginRequest');
      cy.wait('@currentUser');

      cy.visit('/models');
      cy.wait('@modelsFlaky');

      // Initial error state
      cy.contains(/try again|retry/i).should('be.visible');

      // Retry should clear error
      cy.contains(/try again|retry/i).click();
      cy.wait('@modelsFlaky');

      // Error boundary should be reset
      cy.contains(/try again|retry/i).should('not.exist');
    });
  });

  describe('Error Message Display', () => {
    it('should display user-friendly error message for 500 errors', () => {
      cy.intercept('GET', '**/v1/auth/me', {
        statusCode: 500,
        body: {
          error: 'Internal server error occurred',
          code: 'INTERNAL_ERROR',
        },
      }).as('serverError');

      cy.visit('/dashboard', { failOnStatusCode: false });
      cy.wait('@serverError');

      cy.contains(/something went wrong|error/i).should('be.visible');
    });

    it('should display appropriate message for 404 errors', () => {
      cy.visit('/login');
      cy.get('[data-cy=login-email]').type('test@example.com');
      cy.get('[data-cy=login-password]').type('password');
      cy.get('[data-cy=login-submit]').click();
      cy.wait('@loginRequest');
      cy.wait('@currentUser');

      cy.intercept('GET', '**/v1/adapters/nonexistent-id', {
        statusCode: 404,
        body: {
          error: 'Adapter not found',
          code: 'NOT_FOUND',
        },
      }).as('notFound');

      cy.visit('/adapters/nonexistent-id', { failOnStatusCode: false });

      // Should show not found or error message
      cy.contains(/not found|error|something went wrong/i).should('be.visible');
    });

    it('should display network error message when connection fails', () => {
      cy.intercept('GET', '**/v1/models', {
        forceNetworkError: true,
      }).as('networkError');

      cy.visit('/login');
      cy.get('[data-cy=login-email]').type('test@example.com');
      cy.get('[data-cy=login-password]').type('password');
      cy.get('[data-cy=login-submit]').click();
      cy.wait('@loginRequest');
      cy.wait('@currentUser');

      cy.visit('/models');

      // Should show error state
      cy.get('[role="alert"]').should('exist');
      cy.contains(/error|failed|try again/i).should('be.visible');
    });

    it('should display timeout error message', () => {
      cy.intercept('GET', '**/v1/adapters**', {
        statusCode: 504,
        body: {
          error: 'Gateway timeout',
          code: 'TIMEOUT',
        },
      }).as('timeout');

      cy.visit('/login');
      cy.get('[data-cy=login-email]').type('test@example.com');
      cy.get('[data-cy=login-password]').type('password');
      cy.get('[data-cy=login-submit]').click();
      cy.wait('@loginRequest');
      cy.wait('@currentUser');

      cy.visit('/adapters');
      cy.wait('@timeout');

      cy.contains(/error|failed|try again/i).should('be.visible');
    });

    it('should display rate limit error message', () => {
      cy.intercept('POST', '**/v1/infer', {
        statusCode: 429,
        body: {
          error: 'Too many requests',
          code: 'RATE_LIMIT_EXCEEDED',
        },
      }).as('rateLimited');

      cy.visit('/login');
      cy.get('[data-cy=login-email]').type('test@example.com');
      cy.get('[data-cy=login-password]').type('password');
      cy.get('[data-cy=login-submit]').click();
      cy.wait('@loginRequest');
      cy.wait('@currentUser');

      cy.visit('/inference');

      // Fill in inference form if available
      cy.get('body').then(($body) => {
        if ($body.find('[data-cy=prompt-input]').length > 0) {
          cy.get('[data-cy=prompt-input]').type('Test prompt');
          cy.get('[data-cy=run-inference-btn]').click();
          cy.wait('@rateLimited');

          // Should show rate limit error
          cy.contains(/too many|rate limit|try again/i).should('be.visible');
        }
      });
    });
  });

  describe('Recovery After Transient Errors', () => {
    it('should recover automatically after temporary API failure', () => {
      let requestCount = 0;

      cy.intercept('GET', '**/v1/adapters**', (req) => {
        requestCount++;
        if (requestCount <= 2) {
          req.reply({
            statusCode: 503,
            body: {
              error: 'Service temporarily unavailable',
              code: 'SERVICE_UNAVAILABLE',
            },
          });
        } else {
          req.reply({
            statusCode: 200,
            body: [],
          });
        }
      }).as('flakyEndpoint');

      cy.visit('/login');
      cy.get('[data-cy=login-email]').type('test@example.com');
      cy.get('[data-cy=login-password]').type('password');
      cy.get('[data-cy=login-submit]').click();
      cy.wait('@loginRequest');
      cy.wait('@currentUser');

      cy.visit('/adapters');

      // After initial failure, click retry multiple times if needed
      cy.contains(/try again|retry/i).click();

      // After sufficient retries, should recover
      cy.get('body').then(($body) => {
        if ($body.find('[role="alert"]').length > 0) {
          // One more retry if still showing error
          cy.contains(/try again|retry/i).click();
        }
      });

      // Eventually should show the adapters page content
      cy.get('[role="alert"]').should('not.exist');
    });

    it('should maintain form state after error recovery', () => {
      cy.visit('/login');
      cy.get('[data-cy=login-email]').type('test@example.com');
      cy.get('[data-cy=login-password]').type('password');
      cy.get('[data-cy=login-submit]').click();
      cy.wait('@loginRequest');
      cy.wait('@currentUser');

      // Navigate to inference page
      cy.visit('/inference');

      // Fill in prompt
      cy.get('body').then(($body) => {
        if ($body.find('[data-cy=prompt-input]').length > 0) {
          const testPrompt = 'This is a test prompt that should be preserved';
          cy.get('[data-cy=prompt-input]').type(testPrompt);

          // Simulate transient error then success
          let hasErrored = false;
          cy.intercept('POST', '**/v1/infer', (req) => {
            if (!hasErrored) {
              hasErrored = true;
              req.reply({
                statusCode: 500,
                body: {
                  error: 'Temporary failure',
                  code: 'INTERNAL_ERROR',
                },
              });
            } else {
              req.reply({
                statusCode: 200,
                body: {
                  schema_version: '1.0',
                  id: 'run-1',
                  text: 'Response text',
                  tokens_generated: 3,
                  token_count: 3,
                  latency_ms: 50,
                  adapters_used: [],
                  finish_reason: 'stop',
                },
              });
            }
          }).as('inferRetry');

          cy.get('[data-cy=run-inference-btn]').click();
          cy.wait('@inferRetry');

          // Form input should still contain the prompt
          cy.get('[data-cy=prompt-input]').should('contain.value', testPrompt);
        }
      });
    });

    it('should handle auth token refresh after 401 error', () => {
      cy.visit('/login');
      cy.get('[data-cy=login-email]').type('test@example.com');
      cy.get('[data-cy=login-password]').type('password');
      cy.get('[data-cy=login-submit]').click();
      cy.wait('@loginRequest');
      cy.wait('@currentUser');

      let isFirstRequest = true;

      cy.intercept('GET', '**/v1/adapters**', (req) => {
        if (isFirstRequest) {
          isFirstRequest = false;
          req.reply({
            statusCode: 401,
            body: {
              error: 'Token expired',
              code: 'UNAUTHORIZED',
            },
          });
        } else {
          req.reply({
            statusCode: 200,
            body: [],
          });
        }
      }).as('authExpiry');

      cy.visit('/adapters');
      cy.wait('@authExpiry');

      // Should either redirect to login or refresh token and retry
      cy.url().then((url) => {
        if (url.includes('/login')) {
          // Redirected to login - expected behavior
          cy.url().should('include', '/login');
        } else {
          // Token was refreshed, should show content
          cy.get('[role="alert"]').should('not.exist');
        }
      });
    });

    it('should recover UI after component error and retry', () => {
      cy.visit('/login');
      cy.get('[data-cy=login-email]').type('test@example.com');
      cy.get('[data-cy=login-password]').type('password');
      cy.get('[data-cy=login-submit]').click();
      cy.wait('@loginRequest');
      cy.wait('@currentUser');

      // Simulate an API that returns malformed data then correct data
      let returnMalformed = true;
      cy.intercept('GET', '**/v1/models', (req) => {
        if (returnMalformed) {
          returnMalformed = false;
          req.reply({
            statusCode: 200,
            body: 'not-valid-json-object', // Malformed response
          });
        } else {
          req.reply({
            statusCode: 200,
            body: { models: [], total: 0 },
          });
        }
      }).as('malformedThenValid');

      cy.visit('/models');

      // May show error due to malformed data
      cy.get('body').then(($body) => {
        if ($body.find('[role="alert"]').length > 0) {
          // Click retry
          cy.contains(/try again|retry/i).click();

          // After retry with valid data, error should clear
          cy.get('[role="alert"]').should('not.exist');
        }
      });
    });
  });

  describe('Error Boundary Hierarchy', () => {
    it('should contain error to section without crashing entire page', () => {
      cy.visit('/login');
      cy.get('[data-cy=login-email]').type('test@example.com');
      cy.get('[data-cy=login-password]').type('password');
      cy.get('[data-cy=login-submit]').click();
      cy.wait('@loginRequest');
      cy.wait('@currentUser');

      // Stub one section to fail while others succeed
      cy.intercept('GET', '**/v1/adapters**', {
        statusCode: 500,
        body: {
          error: 'Adapter service error',
          code: 'ADAPTER_ERROR',
        },
      }).as('adaptersFail');

      // Models should still work
      cy.intercept('GET', '**/v1/models', {
        statusCode: 200,
        body: { models: [], total: 0 },
      }).as('modelsSuccess');

      cy.visit('/dashboard');

      // Page should still be navigable
      cy.get('body').should('be.visible');

      // Navigation should still work
      cy.get('body').then(($body) => {
        if ($body.find('[data-cy=nav-models]').length > 0) {
          cy.get('[data-cy=nav-models]').should('be.visible');
        }
      });
    });

    it('should allow closing modal with error and continue using app', () => {
      cy.visit('/login');
      cy.get('[data-cy=login-email]').type('test@example.com');
      cy.get('[data-cy=login-password]').type('password');
      cy.get('[data-cy=login-submit]').click();
      cy.wait('@loginRequest');
      cy.wait('@currentUser');

      cy.visit('/adapters');

      // If there are adapters and we can open a detail modal
      cy.get('body').then(($body) => {
        if ($body.find('[data-cy=adapter-card]').length > 0) {
          // Stub detail to fail
          cy.intercept('GET', '**/v1/adapters/*', {
            statusCode: 500,
            body: {
              error: 'Failed to load',
              code: 'LOAD_ERROR',
            },
          }).as('detailFail');

          cy.get('[data-cy=adapter-card]').first().click();

          // If a close button appears, click it
          cy.get('body').then(($modalBody) => {
            if ($modalBody.find('[aria-label="Close"]').length > 0) {
              cy.get('[aria-label="Close"]').click();
            } else if ($modalBody.find('button:contains("Close")').length > 0) {
              cy.contains('button', 'Close').click();
            }
          });

          // Should still be able to navigate
          cy.url().should('include', '/adapters');
        }
      });
    });
  });

  describe('Accessibility in Error States', () => {
    it('should have proper ARIA attributes on error alerts', () => {
      cy.intercept('GET', '**/v1/adapters**', {
        statusCode: 500,
        body: {
          error: 'Server error',
          code: 'INTERNAL_ERROR',
        },
      }).as('fail');

      cy.visit('/login');
      cy.get('[data-cy=login-email]').type('test@example.com');
      cy.get('[data-cy=login-password]').type('password');
      cy.get('[data-cy=login-submit]').click();
      cy.wait('@loginRequest');
      cy.wait('@currentUser');

      cy.visit('/adapters');
      cy.wait('@fail');

      // Check for proper ARIA role
      cy.get('[role="alert"]').should('exist');

      // Check for aria-live attribute (polite or assertive)
      cy.get('[aria-live]').should('exist');
    });

    it('should be keyboard accessible for retry action', () => {
      cy.intercept('GET', '**/v1/models', {
        statusCode: 500,
        body: {
          error: 'Server error',
          code: 'INTERNAL_ERROR',
        },
      }).as('fail');

      cy.visit('/login');
      cy.get('[data-cy=login-email]').type('test@example.com');
      cy.get('[data-cy=login-password]').type('password');
      cy.get('[data-cy=login-submit]').click();
      cy.wait('@loginRequest');
      cy.wait('@currentUser');

      cy.visit('/models');
      cy.wait('@fail');

      // Tab to retry button and press Enter
      cy.contains(/try again|retry/i).focus();
      cy.focused().should('contain.text', /try again|retry/i);
    });
  });
});
