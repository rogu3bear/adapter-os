/**
 * Zod schemas for admin forms
 * Organized by admin features (stacks, tenants, users)
 */

import { z } from 'zod';

/**
 * Stack Form Schema
 * Validates adapter stack creation and editing
 */
export const StackFormSchema = z.object({
  name: z
    .string()
    .min(1, 'Stack name is required')
    .regex(
      /^[a-z0-9-]+$/,
      'Name must be lowercase alphanumeric with hyphens'
    ),

  description: z
    .string()
    .optional(),

  adapters: z
    .array(
      z.object({
        adapter_id: z.string().min(1, 'Adapter ID is required'),
        gate: z
          .number()
          .int('Gate must be an integer')
          .min(0, 'Gate must be at least 0')
          .max(32767, 'Gate must be at most 32767 (Q15 quantized)'),
      })
    )
    .min(1, 'At least one adapter is required'),
});

export type StackFormData = z.infer<typeof StackFormSchema>;

/**
 * Tenant Form Schema
 * Validates tenant creation and update parameters
 * Maps to CreateTenantRequest in API types
 */
export const TenantFormSchema = z.object({
  name: z
    .string()
    .min(1, 'Tenant name is required')
    .min(3, 'Name must be at least 3 characters')
    .max(100, 'Name must be at most 100 characters')
    .regex(
      /^[a-z0-9-]+$/,
      'Name must be lowercase alphanumeric with hyphens'
    ),

  description: z
    .string()
    .max(500, 'Description must be at most 500 characters')
    .optional(),

  uid: z
    .number()
    .int('UID must be an integer')
    .min(1000, 'UID must be at least 1000')
    .optional(),

  gid: z
    .number()
    .int('GID must be an integer')
    .min(1000, 'GID must be at least 1000')
    .optional(),

  isolation_level: z
    .enum(['standard', 'enhanced', 'strict'])
    .optional(),
});

export type TenantFormData = z.infer<typeof TenantFormSchema>;

/**
 * User Form Schema
 * Validates user creation and editing
 * Maps to RegisterUserRequest and UpdateUserRequest in API types
 */
export const UserFormSchema = z.object({
  email: z
    .string()
    .email('Invalid email address')
    .min(1, 'Email is required')
    .max(255, 'Email must be at most 255 characters'),

  password: z
    .string()
    .min(8, 'Password must be at least 8 characters')
    .max(128, 'Password must be at most 128 characters')
    .optional(),

  display_name: z
    .string()
    .min(1, 'Display name cannot be empty if provided')
    .max(100, 'Display name must be at most 100 characters')
    .optional(),

  role: z.enum(['admin', 'operator', 'sre', 'compliance', 'auditor', 'viewer']),

  tenant_id: z
    .string()
    .optional(),
});

export type UserFormData = z.infer<typeof UserFormSchema>;
