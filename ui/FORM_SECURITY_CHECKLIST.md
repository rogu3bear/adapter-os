# Form Security Checklist

Use this checklist when creating or reviewing forms that handle sensitive data.

## Pre-Development

- [ ] Identify which fields contain sensitive data (passwords, tokens, keys, etc.)
- [ ] Plan error handling and logging strategy
- [ ] Review if custom validation resolver is needed
- [ ] Check if form data will be sent to error reporting services

## During Development

### Form Setup
- [ ] Import sanitization utilities: `import { sanitizeFormValues, getFieldNames } from '@/utils/sanitize';`
- [ ] Identify any custom sensitive fields beyond defaults
- [ ] Plan validation error messages (should NOT echo back sensitive values)

### Validation Error Handling
- [ ] Use `getFieldNames()` for logging field presence
- [ ] Use `sanitizeFormValues()` if you need to log form structure
- [ ] Never log raw form values directly
- [ ] Add security comment explaining sanitization choice

```typescript
// ✅ GOOD
const safeZodResolver = async (values, context, options) => {
  try {
    return await zodResolver(schema)(values, context, options);
  } catch (err) {
    // SECURITY: Only log field names, never values
    logger.warn('Validation failed', {
      fields: getFieldNames(values),
    });
    return { values: {}, errors: {} };
  }
};

// ❌ BAD
logger.warn('Validation failed', { values }); // Exposes passwords!
```

### Form Submission
- [ ] Sanitize before logging submission errors
- [ ] Don't include raw form data in error messages
- [ ] Verify API errors don't echo sensitive fields
- [ ] Check network request logging doesn't expose credentials

```typescript
// ✅ GOOD
try {
  await submitForm(data);
} catch (error) {
  logger.error('Submission failed', {
    formData: sanitizeFormValues(data),
  });
}

// ❌ BAD
catch (error) {
  logger.error('Failed with data:', data); // Exposes credentials!
}
```

### Debug/Development Code
- [ ] Use sanitization in console.log statements
- [ ] Wrap debug code with environment checks
- [ ] Remove or sanitize debug logging before commit

```typescript
// ✅ GOOD
if (import.meta.env.DEV) {
  console.log('Form state:', sanitizeFormValues(formState));
}

// ❌ BAD
console.log('Submitting:', formData); // Might expose passwords!
```

### Error Reporting Integration
- [ ] Sanitize before sending to Sentry/error tracking
- [ ] Check error context doesn't include raw form values
- [ ] Verify breadcrumbs don't capture sensitive input

```typescript
// ✅ GOOD
Sentry.captureException(error, {
  extra: {
    formData: sanitizeFormValues(data),
  },
});

// ❌ BAD
Sentry.captureException(error, { extra: { data } }); // Exposes secrets!
```

## Code Review Checklist

When reviewing form-related PRs:

### Logging Review
- [ ] Search for `logger.` - verify no raw form values
- [ ] Search for `console.` - verify sanitization or removal
- [ ] Check error handlers use sanitization
- [ ] Verify custom fields are added to sanitization if needed

### State Management
- [ ] Redux/state debuggers configured to sanitize sensitive fields
- [ ] State persistence doesn't include passwords/tokens
- [ ] Form state logging uses sanitization

### Testing
- [ ] Test files don't commit real credentials
- [ ] Mock data uses fake credentials
- [ ] Test logs use sanitization utilities

### Documentation
- [ ] Security concerns documented in code comments
- [ ] README mentions sensitive data handling
- [ ] API documentation warns about sensitive fields

## Pre-Commit

- [ ] No `console.log` with unsanitized form data
- [ ] No `TODO` comments about adding logging that might expose credentials
- [ ] All logger calls reviewed for sensitive data
- [ ] Tests passing: `pnpm test sanitize.test.ts`
- [ ] TypeScript compiles: `pnpm tsc --noEmit`

## Post-Deployment

- [ ] Review application logs for any exposed credentials
- [ ] Check error reporting service for sensitive data leaks
- [ ] Verify monitoring dashboards don't display passwords
- [ ] Audit log aggregation services for credential exposure

## Red Flags (High Risk)

🚨 These patterns indicate potential security issues:

```typescript
// 🚨 DANGER: Raw values in logs
logger.error('Form error', { values });
logger.error('Failed', values);
console.log('Submitting', formData);

// 🚨 DANGER: Form data in error messages
throw new Error(`Invalid: ${JSON.stringify(values)}`);
new Error('Failed with: ' + JSON.stringify(data));

// 🚨 DANGER: Echoing sensitive fields
setError(`Password ${values.password} is invalid`); // Shows password!
alert(`Token ${token} expired`); // Shows token!

// 🚨 DANGER: Unfiltered error context
Sentry.captureException(error, { extra: formState });
trackEvent('form_error', { formData });
```

## Quick Fix Template

When you find unsanitized logging:

```typescript
// Before
logger.warn('Error', { data });

// After
import { sanitizeFormValues } from '@/utils/sanitize';
logger.warn('Error', { data: sanitizeFormValues(data) });

// Or if you only need field names
import { getFieldNames } from '@/utils/sanitize';
logger.warn('Error', { fields: getFieldNames(data) });
```

## Resources

- **Detailed docs:** `/ui/SECURITY_SANITIZATION.md`
- **Quick reference:** `/ui/src/utils/SANITIZE_QUICK_REFERENCE.md`
- **Utility source:** `/ui/src/utils/sanitize.ts`
- **Tests:** `/ui/src/utils/__tests__/sanitize.test.ts`

## Questions?

If you're unsure whether to sanitize:

**Default to YES** - It's better to over-sanitize than to expose credentials.

When in doubt:
1. Use `getFieldNames()` for the safest option
2. Use `sanitizeFormValues()` if you need values for debugging
3. Add custom fields if your form has non-standard sensitive data
4. Document your choice with a comment

---

**Last Updated:** 2025-12-15
**Maintained By:** MLNavigator Security Team
