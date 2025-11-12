// API Helper Utilities for Cypress Backend API Tests
// Provides reusable utilities for authentication, request ID generation, and error validation

import * as crypto from 'crypto';

/**
 * Serialize body for request ID computation matching frontend exactly
 * Frontend uses: body.toString() where body is already JSON.stringify() result
 * This matches frontend behavior exactly for request ID consistency
 */
function serializeBodyForRequestId(body: any): string {
  if (body === null || body === undefined) {
    return '';
  }
  if (typeof body === 'string') {
    return body;
  }
  // Match frontend: use JSON.stringify() directly (preserves insertion order)
  return JSON.stringify(body);
}

/**
 * Extract path without query parameters for request ID computation
 */
function extractPathWithoutQuery(url: string): string {
  try {
    const urlObj = new URL(url, 'http://dummy');
    return urlObj.pathname;
  } catch {
    // If URL parsing fails, extract path manually
    const queryIndex = url.indexOf('?');
    return queryIndex >= 0 ? url.substring(0, queryIndex) : url;
  }
}

/**
 * Compute deterministic request ID matching backend expectations
 * Format: SHA-256 hash of "METHOD:PATH:BODY" truncated to 32 chars
 * Uses Node.js crypto module for SHA-256 computation
 * Matches frontend implementation exactly
 */
export function computeRequestId(
  method: string,
  path: string,
  body: string = ''
): string {
  // Extract path without query parameters for consistent hashing
  const cleanPath = extractPathWithoutQuery(path);
  const canonical = `${method}:${cleanPath}:${body}`;
  
  // Use Node.js crypto for SHA-256 (matches frontend crypto.subtle.digest)
  const hash = crypto.createHash('sha256');
  hash.update(canonical, 'utf8');
  const hashBuffer = hash.digest();
  
  // Convert to hex string and truncate to 32 chars (matching frontend)
  return Array.from(new Uint8Array(hashBuffer))
    .map(b => b.toString(16).padStart(2, '0'))
    .join('')
    .substring(0, 32);
}

/**
 * Get API base URL from environment or default
 */
export function getApiBaseUrl(): string {
  return Cypress.env('API_BASE_URL') || 'http://localhost:8080';
}

/**
 * Get test user credentials from environment or defaults
 */
export function getTestCredentials(): { email: string; password: string } {
  return {
    email: Cypress.env('TEST_USER_EMAIL') || 'test@example.com',
    password: Cypress.env('TEST_USER_PASSWORD') || 'password',
  };
}

/**
 * Validate error response format matches backend ErrorResponse structure
 * Enhanced validation with format checks
 */
export function validateErrorResponse(response: Cypress.Response<any>): void {
  expect(response.body).to.have.property('error');
  expect(response.body.error).to.be.a('string');
  expect(response.body.error.length).to.be.greaterThan(0);
  
  expect(response.body).to.have.property('code');
  expect(response.body.code).to.be.a('string');
  // Error codes should be uppercase with underscores (e.g., "RATE_LIMIT_EXCEEDED")
  expect(response.body.code).to.match(/^[A-Z][A-Z0-9_]*$/);
  
  if (response.body.details) {
    expect(response.body.details).to.be.an('object');
    expect(response.body.details).to.not.be.an('array');
  }
  
  // Validate Content-Type header
  const contentType = response.headers['content-type'];
  if (contentType) {
    expect(contentType).to.include('application/json');
  }
}

/**
 * Make an authenticated API request with proper headers
 * Note: This function is designed to be called from Cypress commands, not directly
 */
export function authenticatedRequest<T = any>(options: {
  method: string;
  url: string;
  body?: any;
  token?: string;
  failOnStatusCode?: boolean;
}): Cypress.Chainable<Cypress.Response<T>> {
  const token = options.token || Cypress.env('authToken');
  
  // Match frontend exactly: serialize body the same way frontend does
  const bodyString = serializeBodyForRequestId(options.body);
  const requestId = computeRequestId(
    options.method,
    options.url,
    bodyString
  );

  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    'X-Request-ID': requestId,
  };

  if (token) {
    headers['Authorization'] = `Bearer ${token}`;
  }

  return cy.request<T>({
    method: options.method,
    url: options.url,
    body: options.body,
    headers,
    failOnStatusCode: options.failOnStatusCode !== false,
  }).then((response) => {
    // Validate request ID echo in response headers
    const echoedRequestId = response.headers['x-request-id'] || response.headers['X-Request-ID'];
    if (echoedRequestId && echoedRequestId !== requestId) {
      cy.log(`Warning: Request ID mismatch. Sent: ${requestId}, Received: ${echoedRequestId}`);
    }
    
    // Validate Content-Type header for JSON responses
    const contentType = response.headers['content-type'];
    if (contentType && !contentType.includes('application/json') && response.status < 300) {
      cy.log(`Warning: Unexpected Content-Type: ${contentType}`);
    }
    
    return response;
  });
}

/**
 * Login and return token as Cypress chainable
 * Includes request ID header for consistency
 */
export function login(): Cypress.Chainable<string> {
  const apiBase = getApiBaseUrl();
  const credentials = getTestCredentials();
  
  // Compute request ID matching frontend implementation exactly
  const bodyString = serializeBodyForRequestId(credentials);
  const requestId = computeRequestId('POST', '/v1/auth/login', bodyString);

  return cy
    .request({
      method: 'POST',
      url: `${apiBase}/v1/auth/login`,
      body: credentials,
      headers: {
        'Content-Type': 'application/json',
        'X-Request-ID': requestId,
      },
    })
    .then((response) => {
      expect(response.status).to.eq(200);
      expect(response.body).to.have.property('token');
      
      // Validate request ID echo
      const echoedRequestId = response.headers['x-request-id'] || response.headers['X-Request-ID'];
      if (echoedRequestId && echoedRequestId !== requestId) {
        cy.log(`Warning: Login request ID mismatch. Sent: ${requestId}, Received: ${echoedRequestId}`);
      }
      
      return response.body.token as string;
    });
}

/**
 * Convert base64url to base64 for Node.js Buffer compatibility
 * JWTs use base64url encoding (RFC 4648 Section 5) which uses - and _ instead of + and /
 */
function base64UrlToBase64(base64url: string): string {
  // Replace base64url characters with base64 characters
  let base64 = base64url.replace(/-/g, '+').replace(/_/g, '/');
  
  // Add padding if needed
  const padding = (4 - base64.length % 4) % 4;
  return base64 + '='.repeat(padding);
}

/**
 * Check if token is expired or near expiry
 * Returns true if token should be refreshed
 */
export function shouldRefreshToken(token: string): boolean {
  try {
    // Decode JWT payload (base64url encoding per JWT spec)
    const parts = token.split('.');
    if (parts.length !== 3) {
      return true; // Invalid token format, should refresh
    }
    
    const payload = parts[1];
    // Convert base64url to base64 for Node.js Buffer
    const base64Payload = base64UrlToBase64(payload);
    const decoded = Buffer.from(base64Payload, 'base64').toString('utf8');
    const claims = JSON.parse(decoded);
    
    if (!claims.exp) {
      return false; // No expiry claim, assume valid
    }
    
    const expiryTime = claims.exp * 1000; // Convert to milliseconds
    const now = Date.now();
    const oneHour = 60 * 60 * 1000;
    
    // Refresh if less than 1 hour until expiry
    return (expiryTime - now) < oneHour;
  } catch {
    return true; // Error parsing token, should refresh
  }
}

/**
 * Validate login response structure
 */
export function validateLoginResponse(response: Cypress.Response<any>): void {
  expect(response.status).to.eq(200);
  expect(response.body).to.have.property('token');
  expect(response.body.token).to.be.a('string');
  expect(response.body.token.length).to.be.greaterThan(0);
  expect(response.body).to.have.property('user_id');
  expect(response.body.user_id).to.be.a('string');
  expect(response.body).to.have.property('role');
  expect(response.body.role).to.be.a('string');
  
  // Validate Content-Type
  const contentType = response.headers['content-type'];
  if (contentType) {
    expect(contentType).to.include('application/json');
  }
}

/**
 * Clean up authentication token
 */
export function clearAuthToken(): void {
  Cypress.env('authToken', undefined);
}

