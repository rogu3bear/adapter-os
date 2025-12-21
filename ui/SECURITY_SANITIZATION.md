# Security: Form Value Sanitization

## Overview

This document describes the security measures implemented to prevent credential exposure in logging and error handling for form validation.

## The Problem

Previously, the `LoginForm.tsx` component had a potential security vulnerability in its form validation error handling:

```typescript
const safeZodResolver: Resolver<LoginFormData> = async (values, context, options) => {
  try {
    return await zodResolver(LoginFormSchema)(values, context, options);
  } catch (err) {
    logger.warn('Form validation error during initial render', {
      component: 'LoginForm',
      operation: 'validation',
      // RISK: If we logged 'values' here, passwords would be exposed in logs
    });
    return { values: {} as LoginFormData, errors: {} };
  }
};
```

Even though the current implementation didn't log the values directly, the risk existed that future developers might add debugging code that could expose sensitive information like passwords, tokens, or API keys in application logs.

## The Solution

We've implemented a reusable sanitization utility that provides two key functions:

### 1. `sanitizeFormValues<T>(values: T, additionalFields?: string[])`

This function sanitizes an object by redacting any fields that match sensitive patterns. It:

- Searches for field names containing common sensitive keywords (case-insensitive)
- Replaces sensitive values with `'[REDACTED]'`
- Handles nested objects recursively
- Allows specifying additional sensitive field patterns
- Preserves non-sensitive data

**Sensitive field patterns:**
- `password`
- `secret`
- `token`
- `apikey` / `api_key`
- `credential`
- `key`
- `auth`
- `totp` / `otp` / `mfa`
- `private`

**Usage example:**

```typescript
import { sanitizeFormValues } from '@/utils/sanitize';

const formData = {
  email: 'user@example.com',
  password: 'secret123',
  totp: '123456'
};

const sanitized = sanitizeFormValues(formData);
// Result:
// {
//   email: 'user@example.com',
//   password: '[REDACTED]',
//   totp: '[REDACTED]'
// }

logger.warn('Form validation failed', { values: sanitized });
```

### 2. `getFieldNames<T>(values: T)`

This function extracts only the field names from an object, useful when you only need to log which fields were present without exposing any values.

**Usage example:**

```typescript
import { getFieldNames } from '@/utils/sanitize';

const formData = {
  email: 'user@example.com',
  password: 'secret123'
};

const fields = getFieldNames(formData);
// Result: ['email', 'password']

logger.warn('Form validation failed', { fields });
```

## Implementation in LoginForm

The `LoginForm.tsx` component now uses `getFieldNames()` to safely log validation errors:

```typescript
import { getFieldNames } from '@/utils/sanitize';

const safeZodResolver: Resolver<LoginFormData> = async (values, context, options) => {
  try {
    return await zodResolver(LoginFormSchema)(values, context, options);
  } catch (err) {
    // SECURITY: Only log field names, never values (which may contain passwords)
    logger.warn('Form validation error during initial render', {
      component: 'LoginForm',
      operation: 'validation',
      fields: getFieldNames(values),
    });
    return { values: {} as LoginFormData, errors: {} };
  }
};
```

## Best Practices for Forms

When working with forms that handle sensitive data:

### ✅ DO:

1. **Use `getFieldNames()` for logging validation errors**
   ```typescript
   logger.warn('Validation failed', { fields: getFieldNames(values) });
   ```

2. **Use `sanitizeFormValues()` when you need to log the structure**
   ```typescript
   logger.debug('Form state', { values: sanitizeFormValues(values) });
   ```

3. **Add custom sensitive fields when needed**
   ```typescript
   const sanitized = sanitizeFormValues(values, ['sessionId', 'customToken']);
   ```

4. **Sanitize before any logging or error reporting**
   ```typescript
   try {
     await submitForm(data);
   } catch (error) {
     logger.error('Form submission failed', {
       sanitizedData: sanitizeFormValues(data)
     });
   }
   ```

### ❌ DON'T:

1. **Never log raw form values directly**
   ```typescript
   // BAD - exposes passwords!
   logger.warn('Form error', { values });
   ```

2. **Never stringify form objects for debugging without sanitization**
   ```typescript
   // BAD - exposes sensitive data!
   console.log('Form data:', JSON.stringify(values));
   ```

3. **Don't assume field names reveal sensitivity**
   ```typescript
   // BAD - custom field names might still be sensitive
   logger.warn('Error', { data: values.customAuthField });
   ```

## Testing

The sanitization utilities are fully tested in `ui/src/utils/__tests__/sanitize.test.ts`. Tests cover:

- Basic password redaction
- All default sensitive field patterns
- Case-insensitive matching
- Additional sensitive fields parameter
- Nested object sanitization
- Edge cases (empty objects, partial matches)

Run tests with:
```bash
cd ui
pnpm test sanitize.test.ts
```

## Security Audit Checklist

When reviewing forms or validation code:

- [ ] Check for any `logger.*()` calls that include form values
- [ ] Verify form submission error handlers don't log sensitive data
- [ ] Look for `console.log()` debugging code that might expose credentials
- [ ] Ensure error reporting/monitoring services sanitize form data
- [ ] Check that API error responses don't echo back sensitive form fields
- [ ] Verify state management doesn't persist sensitive values unnecessarily

## Related Files

- **Utility:** `/Users/mln-dev/Dev/adapter-os/ui/src/utils/sanitize.ts`
- **Tests:** `/Users/mln-dev/Dev/adapter-os/ui/src/utils/__tests__/sanitize.test.ts`
- **Implementation:** `/Users/mln-dev/Dev/adapter-os/ui/src/components/LoginForm.tsx`
- **Exports:** `/Users/mln-dev/Dev/adapter-os/ui/src/utils/index.ts`

## Future Improvements

Potential enhancements for the sanitization system:

1. **Runtime validation** - Add development-mode warnings when unsanitized form values are logged
2. **Custom redaction strategies** - Support partial redaction (e.g., show last 4 digits)
3. **Integration with error reporting** - Automatically sanitize in Sentry/monitoring integrations
4. **TypeScript types** - Create utility types for "sanitized" values
5. **Linting rules** - ESLint rules to catch potential credential logging

---

**Last Updated:** 2025-12-15
**Security Level:** Critical
**Maintained By:** MLNavigator Security Team
