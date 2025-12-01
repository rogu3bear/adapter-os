import { expect } from 'vitest';
import type { StatusV2 } from '@/api/status';

/**
 * Custom matchers for Vitest/Jest testing.
 * These matchers provide domain-specific assertions for AdapterOS types.
 */

/**
 * Check if a string is a valid tenant ID.
 * Tenant IDs should be lowercase alphanumeric with hyphens.
 */
expect.extend({
  toBeValidTenantId(received: string) {
    const pass = /^[a-z0-9-]+$/.test(received) && received.length > 0 && received.length <= 100;
    return {
      message: () => `expected ${received} to be a valid tenant ID (lowercase alphanumeric with hyphens)`,
      pass,
    };
  },
});

/**
 * Check if an object is a valid StatusV2 structure.
 * Validates schema, version, and required fields.
 */
expect.extend({
  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- test matcher needs flexible typing
  toBeValidStatusV2(received: any) {
    const pass =
      received?.schema === 'status.v2' &&
      typeof received?.version === 'number' &&
      received?.version === 2 &&
      typeof received?.issuedAt === 'string' &&
      typeof received?.nonce === 'string' &&
      Array.isArray(received?.tenants) &&
      Array.isArray(received?.operations) &&
      received?.signature &&
      typeof received?.signature?.algorithm === 'string' &&
      typeof received?.signature?.value === 'string' &&
      typeof received?.signature?.keyId === 'string' &&
      typeof received?.signature?.issuedAt === 'string';

    return {
      message: () => {
        if (!pass) {
          const issues: string[] = [];
          if (received?.schema !== 'status.v2') issues.push('schema must be "status.v2"');
          if (received?.version !== 2) issues.push('version must be 2');
          if (!received?.issuedAt) issues.push('issuedAt is required');
          if (!received?.nonce) issues.push('nonce is required');
          if (!Array.isArray(received?.tenants)) issues.push('tenants must be an array');
          if (!Array.isArray(received?.operations)) issues.push('operations must be an array');
          if (!received?.signature) issues.push('signature is required');
          return `expected ${JSON.stringify(received)} to be a valid StatusV2 object. Issues: ${issues.join(', ')}`;
        }
        return `expected ${JSON.stringify(received)} not to be a valid StatusV2 object`;
      },
      pass,
    };
  },
});

/**
 * Check if an object has a valid signature structure.
 * Validates signature fields are present and correctly typed.
 */
expect.extend({
  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- test matcher needs flexible typing
  toHaveValidSignature(received: any) {
    const sig = received?.signature;
    const pass =
      sig &&
      typeof sig?.algorithm === 'string' &&
      sig?.algorithm.length > 0 &&
      typeof sig?.value === 'string' &&
      sig?.value.length > 0 &&
      typeof sig?.keyId === 'string' &&
      sig?.keyId.length > 0 &&
      typeof sig?.issuedAt === 'string' &&
      sig?.issuedAt.length > 0;

    return {
      message: () => {
        if (!pass) {
          const issues: string[] = [];
          if (!sig) issues.push('signature object is missing');
          if (!sig?.algorithm) issues.push('signature.algorithm is required');
          if (!sig?.value) issues.push('signature.value is required');
          if (!sig?.keyId) issues.push('signature.keyId is required');
          if (!sig?.issuedAt) issues.push('signature.issuedAt is required');
          return `expected ${JSON.stringify(received)} to have a valid signature. Issues: ${issues.join(', ')}`;
        }
        return `expected ${JSON.stringify(received)} not to have a valid signature`;
      },
      pass,
    };
  },
});

/**
 * Initialize custom matchers.
 * Call this function in your test setup file.
 */
export function extendMatchers() {
  // Matchers are already registered via expect.extend() above
  // This function exists for explicit initialization if needed
}


