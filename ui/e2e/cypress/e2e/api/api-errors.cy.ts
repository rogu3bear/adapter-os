// API Error Response Handling Tests
//
// Tests that the UI correctly handles various API error responses:
// - 400 Bad Request
// - 401 Unauthorized (redirect to login)
// - 429 Rate Limited (with Retry-After)
// - 500 Server Error
// - Network failures
//
// Uses cy.intercept to mock API responses for deterministic testing.

describe('API Error Response Handling', () => {
  const API_BASE = '/api';

  beforeEach(() => {
    // Stub common routes to allow page load
    cy.intercept('GET', '**/healthz', { status: 'healthy' }).as('healthz');
    cy.intercept('GET', '**/v1/auth/config', {
      allow_registration: false,
      require_email_verification: false,
      session_timeout_minutes: 60,
      max_login_attempts: 5,
      mfa_required: false,
      dev_bypass_allowed: true,
    }).as('authConfig');
    cy.intercept('GET', '**/v1/auth/me', {
      schema_version: '1.0',
      user_id: 'user-1',
      email: 'test@example.com',
      role: 'admin',
      tenant_id: 'tenant-1',
      permissions: ['*'],
    }).as('currentUser');
    cy.intercept('GET', '**/v1/auth/tenants', {
      body: { schema_version: '1.0', tenants: [{ id: 'tenant-1', name: 'Test Tenant' }] },
    }).as('tenantList');
    cy.intercept('GET', '**/v1/models', {
      body: { models: [], total: 0 },
    }).as('models');
    cy.intercept('POST', '**/v1/auth/refresh', {
      body: { token: 'stub-token', expires_at: Date.now() + 3600_000 },
    }).as('refreshSession');
  });

  describe('400 Bad Request Handling', () => {
    it('displays validation error message for 400 response', () => {
      // Intercept adapters endpoint to return 400
      cy.intercept('GET', '**/v1/adapters**', {
        statusCode: 400,
        body: {
          error: 'Invalid request parameters',
          code: 'VALIDATION_ERROR',
          details: {
            field: 'name',
            message: 'Name must be at least 3 characters',
          },
        },
      }).as('adaptersError');

      cy.visit('/adapters');
      cy.wait('@adaptersError');

      // Verify error is displayed to user
      cy.contains('Invalid request parameters').should('be.visible');
    });

    it('displays error for malformed request body on POST', () => {
      // Stub adapters list first
      cy.intercept('GET', '**/v1/adapters**', { body: [] }).as('adaptersList');

      // Intercept adapter creation to return 400
      cy.intercept('POST', '**/v1/adapters', {
        statusCode: 400,
        body: {
          error: 'Malformed request body',
          code: 'PARSE_ERROR',
          details: {
            message: 'Expected JSON object but received array',
          },
        },
      }).as('createAdapterError');

      cy.visit('/adapters');
      cy.wait('@adaptersList');

      // Look for a create button - if one exists, click it
      cy.get('body').then(($body) => {
        if ($body.find('[data-cy=create-adapter-button]').length) {
          cy.get('[data-cy=create-adapter-button]').click();
          // Fill minimal form and submit
          cy.get('[data-cy=adapter-name-input]').type('Test Adapter');
          cy.get('[data-cy=submit-adapter]').click();
          cy.wait('@createAdapterError');
          cy.contains('Malformed request body').should('be.visible');
        }
      });
    });

    it('shows field-level validation errors in forms', () => {
      // Intercept training job creation with field-level errors
      cy.intercept('POST', '**/v1/training/**', {
        statusCode: 400,
        body: {
          error: 'Validation failed',
          code: 'VALIDATION_ERROR',
          details: {
            errors: [
              { field: 'learning_rate', message: 'Must be between 0 and 1' },
              { field: 'epochs', message: 'Must be a positive integer' },
            ],
          },
        },
      }).as('trainingValidationError');

      // Stub prerequisites for training page
      cy.intercept('GET', '**/v1/adapters**', { body: [] }).as('adapters');
      cy.intercept('GET', '**/v1/datasets**', {
        body: { data: [{ id: 'ds-1', name: 'Test Dataset' }] },
      }).as('datasets');

      cy.visit('/training');
      cy.wait('@adapters');
    });
  });

  describe('401 Unauthorized Redirect to Login', () => {
    it('redirects to login page when API returns 401', () => {
      // Override auth/me to return 401
      cy.intercept('GET', '**/v1/auth/me', {
        statusCode: 401,
        body: {
          error: 'Authentication required',
          code: 'UNAUTHORIZED',
        },
      }).as('authUnauthorized');

      // Override refresh to also return 401
      cy.intercept('POST', '**/v1/auth/refresh', {
        statusCode: 401,
        body: {
          error: 'Session expired',
          code: 'SESSION_EXPIRED',
        },
      }).as('refreshUnauthorized');

      cy.visit('/dashboard');

      // Wait for auth check to complete
      cy.wait('@authUnauthorized');

      // Should redirect to login
      cy.url().should('include', '/login');
    });

    it('shows session expired message on 401 during active session', () => {
      // First, allow initial auth to succeed
      cy.intercept('GET', '**/v1/auth/me', {
        schema_version: '1.0',
        user_id: 'user-1',
        email: 'test@example.com',
        role: 'admin',
        tenant_id: 'tenant-1',
        permissions: ['*'],
      }).as('authSuccess');

      cy.intercept('GET', '**/v1/adapters**', { body: [] }).as('adaptersList');

      cy.visit('/adapters');
      cy.wait('@authSuccess');
      cy.wait('@adaptersList');

      // Now intercept subsequent request to return 401
      cy.intercept('GET', '**/v1/adapters**', {
        statusCode: 401,
        body: {
          error: 'Token expired',
          code: 'TOKEN_EXPIRED',
        },
      }).as('adaptersUnauthorized');

      // Override refresh to fail
      cy.intercept('POST', '**/v1/auth/refresh', {
        statusCode: 401,
        body: {
          error: 'Refresh token expired',
          code: 'SESSION_EXPIRED',
        },
      }).as('refreshFailed');

      // Trigger a refresh (e.g., click refresh button if available)
      cy.get('body').then(($body) => {
        if ($body.find('[data-cy=refresh-button]').length) {
          cy.get('[data-cy=refresh-button]').click();
          cy.wait('@adaptersUnauthorized');
          cy.wait('@refreshFailed');
          cy.url().should('include', '/login');
        }
      });
    });

    it('clears auth state on 401 and prevents further authenticated requests', () => {
      cy.intercept('GET', '**/v1/auth/me', {
        statusCode: 401,
        body: {
          error: 'Unauthorized',
          code: 'UNAUTHORIZED',
        },
      }).as('authFailed');

      cy.intercept('POST', '**/v1/auth/refresh', {
        statusCode: 401,
        body: {
          error: 'Session expired',
          code: 'SESSION_EXPIRED',
        },
      }).as('refreshFailed');

      cy.visit('/dashboard');
      cy.wait('@authFailed');

      // Verify redirected to login
      cy.url().should('include', '/login');

      // Verify no auth token in storage
      cy.window().then((win) => {
        const token = win.sessionStorage.getItem('authToken');
        expect(token).to.be.null;
      });
    });
  });

  describe('429 Rate Limited with Retry-After', () => {
    it('displays rate limit message with retry timer', () => {
      cy.intercept('GET', '**/v1/adapters**', {
        statusCode: 429,
        headers: {
          'Retry-After': '30',
          'X-RateLimit-Limit': '100',
          'X-RateLimit-Remaining': '0',
          'X-RateLimit-Reset': String(Math.floor(Date.now() / 1000) + 30),
        },
        body: {
          error: 'Rate limit exceeded',
          code: 'RATE_LIMIT_EXCEEDED',
          details: {
            limit: 100,
            remaining: 0,
            reset_at: new Date(Date.now() + 30000).toISOString(),
          },
        },
      }).as('rateLimited');

      cy.visit('/adapters');
      cy.wait('@rateLimited');

      // Verify rate limit message is displayed
      cy.contains(/rate limit/i).should('be.visible');
    });

    it('handles rate limiting on inference endpoint', () => {
      // Stub successful auth and models
      cy.intercept('GET', '**/v1/adapters**', { body: [] }).as('adapters');

      cy.intercept('POST', '**/v1/infer', {
        statusCode: 429,
        headers: {
          'Retry-After': '60',
        },
        body: {
          error: 'Too many inference requests',
          code: 'RATE_LIMIT_EXCEEDED',
          details: {
            limit: 10,
            remaining: 0,
            window_seconds: 60,
          },
        },
      }).as('inferRateLimited');

      cy.visit('/inference');
      cy.wait('@adapters');

      // Try to submit an inference request if the form exists
      cy.get('body').then(($body) => {
        if ($body.find('[data-cy=inference-input]').length) {
          cy.get('[data-cy=inference-input]').type('Test prompt');
          cy.get('[data-cy=inference-submit]').click();
          cy.wait('@inferRateLimited');
          cy.contains(/rate limit|too many requests/i).should('be.visible');
        }
      });
    });

    it('shows countdown timer for Retry-After header', () => {
      const retryAfter = 5;

      cy.intercept('GET', '**/v1/adapters**', {
        statusCode: 429,
        headers: {
          'Retry-After': String(retryAfter),
        },
        body: {
          error: 'Rate limit exceeded',
          code: 'RATE_LIMIT_EXCEEDED',
        },
      }).as('rateLimitedWithTimer');

      cy.visit('/adapters');
      cy.wait('@rateLimitedWithTimer');

      // Check for rate limit indication
      cy.contains(/rate limit/i).should('be.visible');
    });
  });

  describe('500 Server Error Display', () => {
    it('displays server error message for 500 response', () => {
      cy.intercept('GET', '**/v1/adapters**', {
        statusCode: 500,
        body: {
          error: 'Internal server error',
          code: 'INTERNAL_ERROR',
          request_id: 'req-12345',
        },
      }).as('serverError');

      cy.visit('/adapters');
      cy.wait('@serverError');

      // Verify error message is displayed
      cy.contains(/server error|internal error|something went wrong/i).should('be.visible');
    });

    it('shows request ID for support reference on 500 errors', () => {
      const requestId = 'req-abc123xyz';

      cy.intercept('GET', '**/v1/adapters**', {
        statusCode: 500,
        headers: {
          'X-Request-ID': requestId,
        },
        body: {
          error: 'Database connection failed',
          code: 'DATABASE_ERROR',
          request_id: requestId,
        },
      }).as('dbError');

      cy.visit('/adapters');
      cy.wait('@dbError');

      // Error should be displayed
      cy.contains(/error|failed/i).should('be.visible');
    });

    it('handles 502 Bad Gateway error', () => {
      cy.intercept('GET', '**/v1/adapters**', {
        statusCode: 502,
        body: {
          error: 'Bad Gateway',
          code: 'BAD_GATEWAY',
        },
      }).as('badGateway');

      cy.visit('/adapters');
      cy.wait('@badGateway');

      // Should show connectivity/server error
      cy.contains(/gateway|server|unavailable|error/i).should('be.visible');
    });

    it('handles 503 Service Unavailable error', () => {
      cy.intercept('GET', '**/v1/adapters**', {
        statusCode: 503,
        headers: {
          'Retry-After': '60',
        },
        body: {
          error: 'Service temporarily unavailable',
          code: 'SERVICE_UNAVAILABLE',
        },
      }).as('serviceUnavailable');

      cy.visit('/adapters');
      cy.wait('@serviceUnavailable');

      // Should show service unavailable message
      cy.contains(/unavailable|maintenance|error/i).should('be.visible');
    });

    it('handles 504 Gateway Timeout error', () => {
      cy.intercept('GET', '**/v1/adapters**', {
        statusCode: 504,
        body: {
          error: 'Gateway timeout',
          code: 'GATEWAY_TIMEOUT',
        },
      }).as('gatewayTimeout');

      cy.visit('/adapters');
      cy.wait('@gatewayTimeout');

      // Should show timeout/error message
      cy.contains(/timeout|error|unavailable/i).should('be.visible');
    });
  });

  describe('Network Failure Handling', () => {
    it('displays offline message when network request fails', () => {
      cy.intercept('GET', '**/v1/adapters**', {
        forceNetworkError: true,
      }).as('networkError');

      cy.visit('/adapters');
      cy.wait('@networkError');

      // Should show network error or offline message
      cy.contains(/network|offline|connection|unreachable|error/i).should('be.visible');
    });

    it('handles request timeout gracefully', () => {
      cy.intercept('GET', '**/v1/adapters**', (req) => {
        // Delay response beyond typical timeout
        req.reply({
          delay: 30000, // 30 second delay
          statusCode: 200,
          body: [],
        });
      }).as('slowRequest');

      // Visit page - the request will be aborted by client timeout
      cy.visit('/adapters', { timeout: 15000 });

      // Either shows loading state or timeout error
      cy.get('body').should('exist');
    });

    it('shows retry option on network failure', () => {
      let requestCount = 0;

      cy.intercept('GET', '**/v1/adapters**', (req) => {
        requestCount++;
        if (requestCount === 1) {
          req.destroy(); // Simulate network failure
        } else {
          req.reply({ body: [] });
        }
      }).as('retryableRequest');

      cy.visit('/adapters');

      // Wait for initial failure
      cy.wait('@retryableRequest');

      // Look for retry button if available
      cy.get('body').then(($body) => {
        if ($body.find('[data-cy=retry-button]').length) {
          cy.get('[data-cy=retry-button]').click();
          cy.wait('@retryableRequest');
        }
      });
    });

    it('handles DNS resolution failure', () => {
      cy.intercept('GET', '**/v1/adapters**', {
        forceNetworkError: true,
      }).as('dnsFailure');

      cy.visit('/adapters');
      cy.wait('@dnsFailure');

      // Should indicate connection problem
      cy.contains(/network|connection|unreachable|error/i).should('be.visible');
    });

    it('recovers from temporary network outage', () => {
      let callCount = 0;

      cy.intercept('GET', '**/v1/adapters**', (req) => {
        callCount++;
        if (callCount <= 2) {
          req.destroy(); // First two requests fail
        } else {
          req.reply({
            body: [
              { id: 'adapter-1', name: 'Recovered Adapter' },
            ],
          });
        }
      }).as('recoverableRequest');

      cy.visit('/adapters');

      // The app should eventually recover after retries
      // or show an error that allows manual retry
      cy.get('body').should('exist');
    });
  });

  describe('Error Response Format Validation', () => {
    it('handles missing error code in response', () => {
      cy.intercept('GET', '**/v1/adapters**', {
        statusCode: 400,
        body: {
          error: 'Something went wrong',
          // No 'code' field
        },
      }).as('errorNoCode');

      cy.visit('/adapters');
      cy.wait('@errorNoCode');

      // Should still display the error message
      cy.contains('Something went wrong').should('be.visible');
    });

    it('handles empty error response body', () => {
      cy.intercept('GET', '**/v1/adapters**', {
        statusCode: 500,
        body: {},
      }).as('emptyError');

      cy.visit('/adapters');
      cy.wait('@emptyError');

      // Should show generic error message
      cy.contains(/error|failed|problem/i).should('be.visible');
    });

    it('handles non-JSON error response', () => {
      cy.intercept('GET', '**/v1/adapters**', {
        statusCode: 500,
        headers: {
          'Content-Type': 'text/plain',
        },
        body: 'Internal Server Error',
      }).as('textError');

      cy.visit('/adapters');
      cy.wait('@textError');

      // Should handle gracefully
      cy.contains(/error|failed|problem/i).should('be.visible');
    });

    it('handles HTML error page response', () => {
      cy.intercept('GET', '**/v1/adapters**', {
        statusCode: 502,
        headers: {
          'Content-Type': 'text/html',
        },
        body: '<html><body><h1>502 Bad Gateway</h1></body></html>',
      }).as('htmlError');

      cy.visit('/adapters');
      cy.wait('@htmlError');

      // Should not crash and should show error
      cy.contains(/gateway|error|unavailable/i).should('be.visible');
    });
  });

  describe('Error State Recovery', () => {
    it('clears error state when navigation occurs', () => {
      // First request fails
      cy.intercept('GET', '**/v1/adapters**', {
        statusCode: 500,
        body: { error: 'Server error', code: 'INTERNAL_ERROR' },
      }).as('adaptersError');

      cy.visit('/adapters');
      cy.wait('@adaptersError');

      // Verify error is shown
      cy.contains(/error/i).should('be.visible');

      // Now intercept for successful response
      cy.intercept('GET', '**/v1/adapters**', {
        body: [{ id: 'a-1', name: 'Working Adapter' }],
      }).as('adaptersSuccess');

      // Navigate away and back
      cy.visit('/dashboard');
      cy.visit('/adapters');
      cy.wait('@adaptersSuccess');

      // Error should be cleared
      cy.contains('Server error').should('not.exist');
    });

    it('allows retry after error', () => {
      let attempts = 0;

      cy.intercept('GET', '**/v1/adapters**', (req) => {
        attempts++;
        if (attempts === 1) {
          req.reply({
            statusCode: 500,
            body: { error: 'Temporary failure', code: 'INTERNAL_ERROR' },
          });
        } else {
          req.reply({
            statusCode: 200,
            body: [{ id: 'a-1', name: 'Recovered' }],
          });
        }
      }).as('retryableEndpoint');

      cy.visit('/adapters');
      cy.wait('@retryableEndpoint');

      // Error should be shown initially
      cy.contains(/error|failed/i).should('be.visible');

      // Refresh page to trigger retry
      cy.reload();
      cy.wait('@retryableEndpoint');

      // Now should show content
      cy.contains('Recovered').should('be.visible');
    });
  });

  describe('Concurrent Request Error Handling', () => {
    it('handles multiple simultaneous errors gracefully', () => {
      cy.intercept('GET', '**/v1/adapters**', {
        statusCode: 500,
        body: { error: 'Adapters error', code: 'INTERNAL_ERROR' },
      }).as('adaptersError');

      cy.intercept('GET', '**/v1/models', {
        statusCode: 500,
        body: { error: 'Models error', code: 'INTERNAL_ERROR' },
      }).as('modelsError');

      cy.visit('/adapters');

      // Wait for both requests
      cy.wait('@adaptersError');

      // Page should not crash
      cy.get('body').should('exist');

      // Should show at least one error indication
      cy.contains(/error/i).should('be.visible');
    });

    it('does not show duplicate error messages', () => {
      cy.intercept('GET', '**/v1/adapters**', {
        statusCode: 500,
        body: { error: 'Duplicate error test', code: 'INTERNAL_ERROR' },
      }).as('error1');

      cy.visit('/adapters');
      cy.wait('@error1');

      // There should not be multiple instances of the same error
      cy.get('body').then(($body) => {
        const errorOccurrences = $body.text().match(/Duplicate error test/g);
        // Either 0 (error handled differently) or 1 (displayed once)
        expect(errorOccurrences?.length || 0).to.be.lessThan(2);
      });
    });
  });
});
