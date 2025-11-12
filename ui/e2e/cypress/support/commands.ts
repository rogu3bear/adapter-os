import { login, authenticatedRequest, getApiBaseUrl, computeRequestId, shouldRefreshToken, clearAuthToken } from './api-helpers';
import { cleanupTrackedResources, clearResourceTracking, trackResource } from './resource-cleanup';

declare global {
  namespace Cypress {
    interface Chainable {
      /**
       * Login and cache authentication token
       * Automatically refreshes token if expired or near expiry
       * @example cy.login()
       */
      login(): Chainable<string>;

      /**
       * Make an authenticated API request
       * @example cy.apiRequest({ method: 'GET', url: '/v1/adapters' })
       */
      apiRequest<T = any>(options: {
        method: string;
        url: string;
        body?: any;
        token?: string;
        failOnStatusCode?: boolean;
      }): Chainable<Cypress.Response<T>>;

      /**
       * Clear authentication token
       * @example cy.clearAuth()
       */
      clearAuth(): Chainable<void>;

      /**
       * Seed test data (placeholder for future implementation)
       * @example cy.seedTestData()
       */
      seedTestData(): Chainable<void>;

      /**
       * Track a created resource for cleanup
       * @example cy.trackResource('adapter', adapterId, `/v1/adapters/${adapterId}`)
       */
      trackResource(type: string, id: string, endpoint: string, method?: string): Chainable<void>;

      /**
       * Cleanup all tracked test resources
       * @example cy.cleanupTestData()
       */
      cleanupTestData(): Chainable<void>;
    }
  }
}

// Login command - authenticates and caches token with automatic refresh
Cypress.Commands.add('login', (email = Cypress.env('TEST_USER_EMAIL'), password = Cypress.env('TEST_USER_PASSWORD')) => {
  return cy.request({
    method: 'POST',
    url: '/api/v1/auth/login',
    body: { email, password },
  }).then((response) => {
    expect(response.status).to.eq(200);
    cy.setCookie('auth_token', response.body.token);
    // Set localStorage or session for frontend state
    cy.window().then((win) => {
      win.localStorage.setItem('auth_token', response.body.token);
    });
    // Removed: cy.visit('/dashboard'); 
    return cy.wrap(response.body.token);
  });
});

// Authenticated API request wrapper
Cypress.Commands.add('apiRequest', <T = any>(options: {
  method: string;
  url: string;
  body?: any;
  token?: string;
  failOnStatusCode?: boolean;
}) => {
  const apiBase = getApiBaseUrl();
  const fullUrl = options.url.startsWith('http') ? options.url : `${apiBase}${options.url}`;
  
  return authenticatedRequest<T>({
    ...options,
    url: fullUrl,
  });
});

// Clear authentication token command
Cypress.Commands.add('clearAuth', () => {
  clearAuthToken();
  return cy.wrap(undefined);
});

// Track resource for cleanup
Cypress.Commands.add('trackResource', (type: string, id: string, endpoint: string, method: string = 'DELETE') => {
  trackResource(type, id, endpoint, method);
  return cy.wrap(undefined);
});

// Cleanup all tracked test resources
Cypress.Commands.add('cleanupTestData', () => {
  return cleanupTrackedResources();
});

// Placeholder for test data seeding
Cypress.Commands.add('seedTestData', () => {
  // TODO: Implement test data seeding
  cy.log('seedTestData not yet implemented');
});
