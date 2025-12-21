# Sanitization Utility - Quick Reference

## When to Use

Use these utilities **any time** you log or display form values that might contain sensitive information.

## Functions

### `sanitizeFormValues(values, additionalFields?)`

Redacts sensitive field values from an object.

```typescript
import { sanitizeFormValues } from '@/utils/sanitize';

// Basic usage
const safe = sanitizeFormValues({ email: 'user@test.com', password: 'secret' });
// → { email: 'user@test.com', password: '[REDACTED]' }

// With additional fields
const safe = sanitizeFormValues(
  { username: 'john', sessionId: 'abc123' },
  ['session']
);
// → { username: 'john', sessionId: '[REDACTED]' }

// With nested objects
const safe = sanitizeFormValues({
  user: { email: 'test@test.com', password: 'secret' }
});
// → { user: { email: 'test@test.com', password: '[REDACTED]' } }
```

### `getFieldNames(values)`

Returns only the field names, no values at all.

```typescript
import { getFieldNames } from '@/utils/sanitize';

const fields = getFieldNames({ email: 'user@test.com', password: 'secret' });
// → ['email', 'password']

logger.warn('Validation failed', { fields });
// Log output: "Validation failed" fields: ['email', 'password']
```

## Default Sensitive Patterns

These field names (case-insensitive, substring match) are automatically redacted:

- `password`
- `secret`
- `token`
- `apikey` / `api_key`
- `credential`
- `key`
- `auth`
- `totp` / `otp` / `mfa`
- `private`

## Common Use Cases

### Form Validation Errors

```typescript
const safeZodResolver = async (values, context, options) => {
  try {
    return await zodResolver(schema)(values, context, options);
  } catch (err) {
    logger.warn('Validation error', {
      fields: getFieldNames(values),  // ✅ Safe
    });
    return { values: {}, errors: {} };
  }
};
```

### Form Submission Errors

```typescript
try {
  await submitForm(formData);
} catch (error) {
  logger.error('Submission failed', {
    data: sanitizeFormValues(formData),  // ✅ Safe
  });
}
```

### Debugging (Development Only)

```typescript
if (import.meta.env.DEV) {
  console.log('Form state:', sanitizeFormValues(formState));  // ✅ Safe
}
```

### Error Reporting

```typescript
try {
  await apiCall(data);
} catch (error) {
  Sentry.captureException(error, {
    extra: {
      formData: sanitizeFormValues(data),  // ✅ Safe
    },
  });
}
```

## Anti-Patterns (Don't Do This)

```typescript
// ❌ BAD - Exposes password!
logger.warn('Form error', { values });

// ❌ BAD - Exposes credentials!
console.log('Submitting:', JSON.stringify(formData));

// ❌ BAD - Exposes token!
throw new Error(`Invalid data: ${JSON.stringify(values)}`);

// ❌ BAD - Might expose sensitive nested data!
logger.error('Failed', { user: values.user });
```

## Quick Checklist

Before committing form-related code, verify:

- [ ] No `logger.*()` calls with unsanitized form values
- [ ] No `console.log()` with form data
- [ ] Error handlers use `sanitizeFormValues()` or `getFieldNames()`
- [ ] State debuggers/dev tools sanitize sensitive data
- [ ] Error reporting integrations sanitize before sending

---

**See also:** `/Users/mln-dev/Dev/adapter-os/ui/SECURITY_SANITIZATION.md` for detailed documentation
