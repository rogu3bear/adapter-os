/**
 * Login Credentials Form
 *
 * Form fields for email, password, and optional TOTP.
 * Uses react-hook-form with Zod validation.
 */

import { useForm, type Resolver } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Loader2 } from 'lucide-react';
import { LoginFormSchema, type LoginFormData } from '@/schemas/common.schema';
import { logger } from '@/utils/logger';
import { getFieldNames } from '@/utils/sanitize';
import type { LoginCredentials } from '@/hooks/auth/useAuthFlow';

interface LoginCredentialsFormProps {
  /** Called when form is submitted with valid data */
  onSubmit: (credentials: LoginCredentials) => Promise<void>;
  /** Whether the form is currently submitting */
  isSubmitting: boolean;
  /** Whether the submit button should be disabled */
  disabled: boolean;
  /** Error message to display */
  error?: string | null;
  /** Lockout message (takes precedence over error) */
  lockoutMessage?: string | null;
  /** Whether to show the TOTP field */
  showTotpField: boolean;
  /** Callback to show TOTP field */
  onShowTotpField: () => void;
}

/**
 * Safe Zod resolver that catches validation errors during initial render.
 * Prevents console errors when form mounts with empty values.
 */
const safeZodResolver: Resolver<LoginFormData> = async (values, context, options) => {
  try {
    return await zodResolver(LoginFormSchema)(values, context, options);
  } catch (err) {
    // Log validation errors for debugging but don't block render
    // SECURITY: Only log field names, never values (which may contain passwords)
    logger.warn('Form validation error during initial render', {
      component: 'LoginCredentialsForm',
      operation: 'validation',
      fields: getFieldNames(values),
    });
    return { values: {} as LoginFormData, errors: {} };
  }
};

export function LoginCredentialsForm({
  onSubmit,
  isSubmitting,
  disabled,
  error,
  lockoutMessage,
  showTotpField,
  onShowTotpField,
}: LoginCredentialsFormProps) {
  const {
    register,
    handleSubmit,
    formState: { errors },
    watch,
  } = useForm<LoginFormData>({
    resolver: safeZodResolver,
    mode: 'onBlur',
    reValidateMode: 'onChange',
    criteriaMode: 'firstError',
    defaultValues: { email: '', password: '', totp: '' },
    shouldFocusError: false,
  });

  const watchedFields = watch();

  const handleFormSubmit = async (data: LoginFormData) => {
    if (lockoutMessage) {
      return;
    }
    await onSubmit({
      email: data.email.trim(),
      password: data.password.trim(),
      totp: data.totp?.trim() || undefined,
    });
  };

  const isFormDisabled = isSubmitting || disabled;
  const canSubmit =
    !isFormDisabled &&
    !lockoutMessage &&
    watchedFields.email?.trim() &&
    watchedFields.password?.trim();

  return (
    <form
      onSubmit={handleSubmit(handleFormSubmit)}
      className="space-y-5"
      aria-label="Login form"
    >
      {/* Lockout alert */}
      {lockoutMessage && (
        <Alert variant="destructive">
          <AlertDescription>{lockoutMessage}</AlertDescription>
        </Alert>
      )}

      {/* Error alert */}
      {error && !lockoutMessage && (
        <Alert variant="destructive">
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}

      {/* Email and password fields */}
      <div className="space-y-4">
        <div className="space-y-2">
          <Label htmlFor="email" className="text-sm font-medium">
            Email address
          </Label>
          <Input
            id="email"
            type="email"
            data-testid="login-email"
            data-cy="login-email"
            placeholder="you@example.com"
            autoComplete="email"
            autoFocus
            aria-describedby={errors.email ? 'email-error' : undefined}
            aria-invalid={errors.email ? 'true' : 'false'}
            {...register('email')}
            disabled={isFormDisabled}
            className="h-11"
          />
          {errors.email && (
            <p id="email-error" className="text-sm text-destructive" role="alert">
              {errors.email.message}
            </p>
          )}
        </div>

        <div className="space-y-2">
          <Label htmlFor="password" className="text-sm font-medium">
            Password
          </Label>
          <Input
            id="password"
            type="password"
            data-testid="login-password"
            data-cy="login-password"
            placeholder="Enter your password"
            autoComplete="current-password"
            aria-describedby={errors.password ? 'password-error' : undefined}
            aria-invalid={errors.password ? 'true' : 'false'}
            {...register('password')}
            disabled={isFormDisabled}
            className="h-11"
          />
          {errors.password && (
            <p id="password-error" className="text-sm text-destructive" role="alert">
              {errors.password.message}
            </p>
          )}
        </div>
      </div>

      {/* TOTP field (conditional) */}
      {showTotpField && (
        <div className="space-y-2">
          <Label htmlFor="totp" className="text-sm font-medium">
            TOTP code
          </Label>
          <Input
            id="totp"
            type="text"
            inputMode="numeric"
            data-cy="login-totp"
            placeholder="000000"
            autoComplete="one-time-code"
            aria-describedby={errors.totp ? 'totp-error' : undefined}
            aria-invalid={errors.totp ? 'true' : 'false'}
            {...register('totp')}
            disabled={isFormDisabled}
            className="h-11"
            maxLength={6}
          />
          {errors.totp && (
            <p id="totp-error" className="text-sm text-destructive" role="alert">
              {errors.totp.message}
            </p>
          )}
        </div>
      )}

      {/* Submit button */}
      <div className="pt-2">
        <Button
          type="submit"
          className="w-full h-11 text-base font-medium"
          data-testid="login-submit"
          data-cy="login-submit"
          disabled={!canSubmit}
        >
          {isSubmitting ? (
            <>
              <Loader2 className="h-4 w-4 mr-2 animate-spin" />
              Signing in...
            </>
          ) : (
            'Sign in'
          )}
        </Button>
        {!showTotpField && (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={onShowTotpField}
            className="w-full mt-3 text-muted-foreground"
          >
            Use TOTP code
          </Button>
        )}
      </div>
    </form>
  );
}
