/**
 * Common validation schemas used across multiple domains
 *
 * Maps to backend validation from:
 * - adapteros-server-api/src/validation.rs
 * - adapteros-policy/src/packs/
 */

import { z } from 'zod';

/**
 * Tenant ID schema
 *
 * From: adapteros-server-api/src/validation.rs::validate_tenant_id()
 * - Must contain only lowercase letters, numbers, underscores, and hyphens
 * - Maximum length: 50 characters
 */
export const tenantIdSchema = z.string()
  .min(1, 'Tenant ID is required')
  .max(50, 'Tenant ID must not exceed 50 characters')
  .regex(
    /^[a-z0-9_-]+$/,
    'Tenant ID must contain only lowercase letters, numbers, underscores, and hyphens'
  )
  .describe('Tenant identifier');

export type TenantId = z.infer<typeof tenantIdSchema>;

/**
 * Repository ID schema
 *
 * From: adapteros-server-api/src/validation/mod.rs::validate_repo_id()
 * - Format: owner/repo
 * - Maximum length: 256 characters
 */
export const RepositoryIdSchema = z.string()
  .min(3, 'Repository ID is too short')
  .max(256, 'Repository ID must not exceed 256 characters')
  .regex(
    /^[a-zA-Z0-9_-]+\/[a-zA-Z0-9_-]+$/,
    'Repository ID must be in format: owner/repo'
  )
  .describe('Repository identifier');

export type RepositoryId = z.infer<typeof RepositoryIdSchema>;

/**
 * Commit SHA schema
 *
 * From: adapteros-server-api/src/validation.rs::validate_commit_sha()
 * - 7-40 hexadecimal characters (lowercase)
 */
export const CommitShaSchema = z.string()
  .min(7, 'Commit SHA must be at least 7 characters')
  .max(40, 'Commit SHA must not exceed 40 characters')
  .regex(
    /^[a-f0-9]{7,40}$/,
    'Commit SHA must be 7-40 lowercase hexadecimal characters'
  )
  .describe('Git commit SHA');

export type CommitSha = z.infer<typeof CommitShaSchema>;

/**
 * BLAKE3 hash schema
 *
 * From: adapteros-server-api/src/validation.rs::validate_hash_b3()
 * - Format: b3:{64 hex characters}
 */
export const blake3HashSchema = z.string()
  .regex(
    /^b3:[a-f0-9]{64}$/,
    'Hash must be in format: b3:{64 lowercase hexadecimal characters}'
  )
  .describe('BLAKE3 hash');

export type Blake3Hash = z.infer<typeof blake3HashSchema>;

/**
 * Description schema (with security validation)
 *
 * From: adapteros-server-api/src/validation/mod.rs::validate_description()
 * - Maximum length: 10000 characters
 * - Checks for suspicious patterns (SQL injection, XSS)
 */
export const descriptionSchema = z.string()
  .min(1, 'Description is required')
  .max(10000, 'Description must not exceed 10000 characters')
  .refine(
    (val) => {
      const upper = val.toUpperCase();
      const suspiciousPatterns = [
        'DROP TABLE',
        'DELETE FROM',
        'INSERT INTO',
        'UPDATE SET',
        '<SCRIPT',
        'JAVASCRIPT:',
        'EVAL(',
        'EXEC(',
      ];
      return !suspiciousPatterns.some((pattern) => upper.includes(pattern));
    },
    'Description contains suspicious content'
  )
  .describe('Description text');

export type Description = z.infer<typeof descriptionSchema>;

/**
 * File path schema (with security validation)
 *
 * From: adapteros-server-api/src/validation.rs::validate_file_paths()
 * - No directory traversal (..)
 * - No absolute paths
 * - Maximum length: 500 characters
 */
export const FilePathSchema = z.string()
  .min(1, 'File path is required')
  .max(500, 'File path must not exceed 500 characters')
  .refine(
    (val) => !val.includes('..'),
    'Directory traversal not allowed'
  )
  .refine(
    (val) => !val.startsWith('/') && !val.includes(':'),
    'Absolute paths not allowed'
  )
  .describe('Relative file path');

export type FilePath = z.infer<typeof FilePathSchema>;

/**
 * Pagination schema
 *
 * Common pagination parameters
 */
export const paginationSchema = z.object({
  // Page number (1-indexed)
  page: z.number()
    .int('Page must be an integer')
    .min(1, 'Page must be at least 1')
    .default(1)
    .describe('Page number'),

  // Items per page
  limit: z.number()
    .int('Limit must be an integer')
    .min(1, 'Limit must be at least 1')
    .max(100, 'Limit must not exceed 100')
    .default(20)
    .describe('Items per page'),

  // Sort field
  sort_by: z.string()
    .max(50, 'Sort field too long')
    .optional()
    .describe('Field to sort by'),

  // Sort order
  sort_order: z.enum(['asc', 'desc'])
    .default('asc')
    .describe('Sort order'),
});

export type Pagination = z.infer<typeof paginationSchema>;

/**
 * Timestamp schema (RFC3339 format)
 */
export const timestampSchema = z.string()
  .regex(
    /^\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}/,
    'Timestamp must be in RFC3339 format (YYYY-MM-DD HH:MM:SS or ISO 8601)'
  )
  .describe('RFC3339 timestamp');

export type Timestamp = z.infer<typeof timestampSchema>;

/**
 * Email schema
 */
export const EmailSchema = z.string()
  .email('Invalid email address')
  .max(254, 'Email must not exceed 254 characters')
  .describe('Email address');

export type Email = z.infer<typeof EmailSchema>;

/**
 * UUID schema
 */
export const UuidSchema = z.string()
  .uuid('Invalid UUID format')
  .describe('UUID');

export type Uuid = z.infer<typeof UuidSchema>;

/**
 * Reusable regex patterns for validation
 *
 * Common patterns used across multiple validation schemas to ensure consistency.
 */
export const patterns = {
  /** Adapter revision pattern: rXXX format (e.g., r001, r042) */
  revision: /^r\d{3,}$/,
  /** UUID pattern */
  uuid: /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i,
  /** Slug pattern: lowercase with hyphens */
  slug: /^[a-z0-9]+(?:-[a-z0-9]+)*$/,
  /** Semantic name component: lowercase letters, numbers, underscores, and hyphens */
  semanticNameComponent: /^[a-z0-9_-]+$/,
} as const;

/**
 * Reusable Zod validators
 *
 * Common validators to prevent duplication across schemas.
 */
export const validators = {
  /** Adapter revision validator: rXXX format (e.g., r001, r042) */
  revision: z.string()
    .regex(patterns.revision, 'Revision must be in format rXXX (e.g., r001, r042)'),

  /** UUID validator */
  uuid: z.string()
    .regex(patterns.uuid, 'Must be a valid UUID'),

  /** Slug validator: lowercase with hyphens only */
  slug: z.string()
    .regex(patterns.slug, 'Must be lowercase with hyphens only'),

  /** Non-empty string validator */
  nonEmptyString: z.string().min(1, 'Required'),

  /** Semantic name component validator */
  semanticNameComponent: z.string()
    .min(1, 'Cannot be empty')
    .max(50, 'Must not exceed 50 characters')
    .regex(
      patterns.semanticNameComponent,
      'Must contain only lowercase letters, numbers, underscores, and hyphens'
    ),
} as const;

/**
 * URL schema
 */
export const UrlSchema = z.string()
  .url('Invalid URL')
  .max(2048, 'URL must not exceed 2048 characters')
  .describe('URL');

export type Url = z.infer<typeof UrlSchema>;

/**
 * Percentage schema (0-100)
 */
export const PercentageSchema = z.number()
  .min(0, 'Percentage must be at least 0')
  .max(100, 'Percentage must not exceed 100')
  .describe('Percentage value');

export type Percentage = z.infer<typeof PercentageSchema>;

/**
 * Chunk size schema (for file uploads)
 *
 * From: adapteros-server-api/src/handlers/chunked_upload.rs
 * - MIN_CHUNK_SIZE: 1 MB
 * - DEFAULT_CHUNK_SIZE: 10 MB
 * - MAX_CHUNK_SIZE: 100 MB
 */
export const ChunkSizeSchema = z.number()
  .int('Chunk size must be an integer')
  .min(1024 * 1024, 'Chunk size must be at least 1 MB')
  .max(100 * 1024 * 1024, 'Chunk size must not exceed 100 MB')
  .default(10 * 1024 * 1024)
  .describe('Chunk size in bytes');

export type ChunkSize = z.infer<typeof ChunkSizeSchema>;

/**
 * File size schema
 *
 * From: adapteros-server-api/src/handlers/datasets.rs
 * - MAX_FILE_SIZE: 100 MB
 * - MAX_TOTAL_SIZE: 500 MB
 */
export const FileSizeSchema = z.number()
  .int('File size must be an integer')
  .min(0, 'File size must be non-negative')
  .max(100 * 1024 * 1024, 'File size must not exceed 100 MB')
  .describe('File size in bytes');

export type FileSize = z.infer<typeof FileSizeSchema>;

/**
 * Batch size schema
 *
 * From: adapteros-server-api/src/handlers/batch.rs
 * - MAX_BATCH_SIZE: 32
 */
export const BatchSizeSchema = z.number()
  .int('Batch size must be an integer')
  .min(1, 'Batch size must be at least 1')
  .max(32, 'Batch size must not exceed 32')
  .describe('Batch size');

export type BatchSize = z.infer<typeof BatchSizeSchema>;

/**
 * Language codes
 *
 * From: adapteros-server-api/src/validation.rs::validate_languages()
 */
export const LanguageSchema = z.enum([
  'python',
  'rust',
  'typescript',
  'javascript',
  'go',
  'java',
  'c',
  'cpp',
  'csharp',
]);

export type Language = z.infer<typeof LanguageSchema>;

/**
 * Validation status enum
 */
export const validationStatusSchema = z.enum([
  'pending',
  'valid',
  'invalid',
  'error',
]);

export type ValidationStatus = z.infer<typeof validationStatusSchema>;

/**
 * Error response schema (for API errors)
 */
export const errorResponseSchema = z.object({
  error: z.string()
    .min(1, 'Error message is required')
    .describe('Error message'),

  code: z.string()
    .optional()
    .describe('Error code'),

  failure_code: z.string()
    .optional()
    .describe('Structured failure code for diagnostics'),

  details: z.union([z.string(), z.record(z.string(), z.any())])
    .optional()
    .describe('Additional error details'),

  timestamp: timestampSchema
    .optional()
    .describe('Error timestamp'),
});

export type ErrorResponse = z.infer<typeof errorResponseSchema>;

/**
 * Helper functions for common validation
 */
/**
 * Login form schema
 *
 * For authentication form validation
 */
export const LoginFormSchema = z.object({
  password: z.string().min(8, 'Password must be at least 8 characters'),
  email: z.string().email('Invalid email address').max(254, 'Email must not exceed 254 characters'),
  totp: z
    .string()
    .trim()
    .optional()
    .refine((val) => !val || (val.length >= 6 && val.length <= 10), 'TOTP must be 6-10 digits'),
});

export type LoginFormData = z.infer<typeof LoginFormSchema>;

/**
 * Login response schema (matches backend LoginResponse)
 *
 * From: crates/adapteros-api-types/src/auth.rs::LoginResponse
 * - schema_version: API version (default: "v1")
 * - token: JWT token (Ed25519 signed)
 * - user_id: Unique user identifier
 * - tenant_id: Associated tenant ID (required by backend)
 * - role: User role (admin, operator, sre, compliance, auditor, viewer)
 * - expires_in: Token expiration in seconds (u64)
 */
export const TenantSummarySchema = z.object({
  schema_version: z.string().default('v1'),
  id: z.string().min(1, 'Tenant ID is required'),
  name: z.string().min(1, 'Tenant name is required'),
  status: z.string().nullable().optional(),
  created_at: z.string().nullable().optional(),
});

export const LoginResponseSchema = z.object({
  schema_version: z.string()
    .default('v1')
    .describe('API schema version'),

  token: z.string()
    .min(1, 'Token is required')
    .describe('JWT authentication token'),

  user_id: z.string()
    .min(1, 'User ID is required')
    .describe('Unique user identifier'),

  tenant_id: z.string()
    .min(1, 'Tenant ID is required')
    .describe('Associated tenant ID'),

  role: z.string()
    .min(1, 'Role is required')
    .describe('User role (developer, admin, operator, sre, compliance, auditor, viewer)'),

  expires_in: z.number()
    .int('Expiration must be an integer')
    .positive('Expiration must be positive')
    .describe('Token expiration in seconds'),

  session_mode: z.enum(['normal', 'dev_bypass']).optional(),

  tenants: z.array(TenantSummarySchema).optional(),
});

export type LoginResponseData = z.infer<typeof LoginResponseSchema>;

export const ValidationUtils = {
  /**
   * Check if string is a valid JSON
   */
  isValidJson(str: string): boolean {
    try {
      JSON.parse(str);
      return true;
    } catch {
      return false;
    }
  },

  /**
   * Format file size for display
   */
  formatFileSize(bytes: number): string {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return `${parseFloat((bytes / Math.pow(k, i)).toFixed(2))} ${sizes[i]}`;
  },

  /**
   * Sanitize string for display (basic XSS prevention)
   */
  sanitizeString(str: string): string {
    return str
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&#x27;')
      .replace(/\//g, '&#x2F;');
  },

  /**
   * Validate RFC3339 timestamp
   */
  isValidTimestamp(timestamp: string): boolean {
    try {
      const date = new Date(timestamp);
      return !isNaN(date.getTime());
    } catch {
      return false;
    }
  },

  /**
   * Get relative time string
   */
  getRelativeTime(timestamp: string): string {
    const now = new Date().getTime();
    const then = new Date(timestamp).getTime();
    const diff = now - then;

    const seconds = Math.floor(diff / 1000);
    if (seconds < 60) return `${seconds}s ago`;

    const minutes = Math.floor(seconds / 60);
    if (minutes < 60) return `${minutes}m ago`;

    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `${hours}h ago`;

    const days = Math.floor(hours / 24);
    return `${days}d ago`;
  },
};
