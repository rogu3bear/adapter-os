// Test Utilities for Cypress Backend API Tests
// Provides common patterns and helpers for test cleanup and validation

/**
 * Helper to safely extract ID from list response
 * Validates response structure before accessing properties
 */
export function safeGetFirstId<T extends { id?: string }>(
  response: Cypress.Response<T[]>
): string | null {
  if (response.status !== 200) {
    return null;
  }
  
  if (!Array.isArray(response.body) || response.body.length === 0) {
    return null;
  }
  
  const firstItem = response.body[0];
  if (!firstItem || typeof firstItem !== 'object' || !('id' in firstItem)) {
    return null;
  }
  
  const id = firstItem.id;
  if (typeof id !== 'string' || id.length === 0) {
    return null;
  }
  
  return id;
}

/**
 * Helper to validate array response structure
 */
export function validateArrayResponse<T>(
  response: Cypress.Response<T[]>,
  minLength: number = 0
): void {
  expect(response.status).to.eq(200);
  expect(response.body).to.be.an('array');
  if (minLength > 0) {
    expect(response.body.length).to.be.at.least(minLength);
  }
}

/**
 * Helper to validate object response structure
 */
export function validateObjectResponse<T extends Record<string, any>>(
  response: Cypress.Response<T>,
  requiredFields: (keyof T)[]
): void {
  expect(response.status).to.eq(200);
  expect(response.body).to.be.an('object');
  requiredFields.forEach(field => {
    expect(response.body).to.have.property(field as string);
  });
}

/**
 * Helper to validate exact status code (not permissive)
 */
export function expectExactStatus(
  response: Cypress.Response<any>,
  expectedStatus: number
): void {
  expect(response.status).to.eq(expectedStatus);
}

/**
 * Helper to validate Content-Type header
 */
export function validateContentType(
  response: Cypress.Response<any>,
  expectedType: string = 'application/json'
): void {
  const contentType = response.headers['content-type'] || response.headers['Content-Type'];
  if (contentType) {
    expect(contentType).to.include(expectedType);
  }
}

