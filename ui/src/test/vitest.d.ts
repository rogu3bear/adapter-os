import '@testing-library/jest-dom';
import type { StatusV2 } from '@/api/status';

/**
 * TypeScript type extensions for Vitest custom matchers.
 * This file extends Vitest's type definitions to include our custom matchers.
 */

declare module 'vitest' {
  interface Assertion<T = any> {
    /**
     * Asserts that a string is a valid tenant ID.
     * Tenant IDs should be lowercase alphanumeric with hyphens.
     * 
     * @example
     * expect('tenant-1').toBeValidTenantId();
     * expect('INVALID').not.toBeValidTenantId();
     */
    toBeValidTenantId(): void;

    /**
     * Asserts that an object is a valid StatusV2 structure.
     * Validates schema, version, and required fields.
     * 
     * @example
     * expect(status).toBeValidStatusV2();
     */
    toBeValidStatusV2(): void;

    /**
     * Asserts that an object has a valid signature structure.
     * Validates signature fields are present and correctly typed.
     * 
     * @example
     * expect(status).toHaveValidSignature();
     */
    toHaveValidSignature(): void;
  }

  interface AsymmetricMatchersContaining {
    /**
     * Asserts that a string is a valid tenant ID.
     */
    toBeValidTenantId(): void;

    /**
     * Asserts that an object is a valid StatusV2 structure.
     */
    toBeValidStatusV2(): void;

    /**
     * Asserts that an object has a valid signature structure.
     */
    toHaveValidSignature(): void;
  }
}


