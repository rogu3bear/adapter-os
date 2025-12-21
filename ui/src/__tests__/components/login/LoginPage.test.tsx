/**
 * LoginPage Component Tests
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { LoginPage } from '@/components/login/LoginPage';
import { createUseAuthFlowMock } from '@/test/mocks/hooks/auth';
import type { AuthFlowState } from '@/hooks/auth/useAuthFlow';

// Mock logger
vi.mock('@/utils/logger', () => ({
  logger: {
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
  },
}));

describe('LoginPage', () => {
  const mockConfig = {
    mfa_required: false,
    max_login_attempts: 5,
    access_token_ttl_minutes: 60,
    session_timeout_minutes: 1440,
    dev_bypass_allowed: true,
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('rendering', () => {
    it('renders login form when ready', () => {
      const authFlow = createUseAuthFlowMock();

      render(<LoginPage authFlow={authFlow} />);

      expect(screen.getByRole('heading', { name: /welcome back/i })).toBeInTheDocument();
      expect(screen.getByLabelText(/email/i)).toBeInTheDocument();
      expect(screen.getByLabelText(/password/i)).toBeInTheDocument();
    });

    it('renders AdapterOS branding', () => {
      const authFlow = createUseAuthFlowMock();

      render(<LoginPage authFlow={authFlow} />);

      expect(screen.getByText('AdapterOS')).toBeInTheDocument();
      expect(screen.getByText(/sign in to access the control plane/i)).toBeInTheDocument();
    });

    it('renders system health panel', () => {
      const authFlow = createUseAuthFlowMock();

      render(<LoginPage authFlow={authFlow} />);

      expect(screen.getByText(/system status/i)).toBeInTheDocument();
    });

    it('renders dev bypass section when allowed', () => {
      const authFlow = createUseAuthFlowMock({ devBypassAllowed: true });

      render(<LoginPage authFlow={authFlow} />);

      expect(screen.getByRole('button', { name: /dev bypass/i })).toBeInTheDocument();
    });

    it('does not render dev bypass section when not allowed', () => {
      const authFlow = createUseAuthFlowMock({ devBypassAllowed: false });

      render(<LoginPage authFlow={authFlow} />);

      expect(screen.queryByRole('button', { name: /dev bypass/i })).not.toBeInTheDocument();
    });
  });

  describe('health error state', () => {
    it('shows health error panel when backend unavailable', () => {
      const authFlow = createUseAuthFlowMock({
        state: { status: 'health_error', error: 'Connection failed' },
        healthOptions: {
          backendStatus: 'error',
          isReady: false,
          healthError: 'Connection failed',
        },
      });

      render(<LoginPage authFlow={authFlow} />);

      expect(screen.getByText(/control plane unavailable/i)).toBeInTheDocument();
    });
  });

  describe('config loading state', () => {
    it('shows loading state when loading config', () => {
      const authFlow = createUseAuthFlowMock({
        state: { status: 'loading_config' },
      });

      render(<LoginPage authFlow={authFlow} />);

      expect(screen.getByText(/preparing sign-in/i)).toBeInTheDocument();
      expect(screen.getByText(/loading sign-in settings/i)).toBeInTheDocument();
    });
  });

  describe('config error state', () => {
    it('shows config error alert with retry button', () => {
      const state: AuthFlowState = { status: 'config_error', error: 'Failed to load config' };
      const authFlow = createUseAuthFlowMock({ state });

      render(<LoginPage authFlow={authFlow} />);

      // The error message from state.error should be displayed (may appear in multiple places)
      const errorMessages = screen.getAllByText(/failed to load config/i);
      expect(errorMessages.length).toBeGreaterThanOrEqual(1);
      expect(screen.getByRole('button', { name: /retry/i })).toBeInTheDocument();
    });

    it('calls retryConfig when retry is clicked', async () => {
      const state: AuthFlowState = { status: 'config_error', error: 'Failed to load config' };
      const authFlow = createUseAuthFlowMock({ state });

      render(<LoginPage authFlow={authFlow} />);

      fireEvent.click(screen.getByRole('button', { name: /retry/i }));

      expect(authFlow.retryConfig).toHaveBeenCalled();
    });
  });

  describe('error state', () => {
    it('displays error message when login fails', () => {
      const state: AuthFlowState = {
        status: 'error',
        config: mockConfig,
        error: { message: 'Invalid credentials' },
      };
      const authFlow = createUseAuthFlowMock({ state, failedAttempts: 1 });

      render(<LoginPage authFlow={authFlow} />);

      expect(screen.getByText(/invalid credentials/i)).toBeInTheDocument();
    });
  });

  describe('locked out state', () => {
    it('displays lockout message when account locked', () => {
      const state: AuthFlowState = {
        status: 'locked_out',
        config: mockConfig,
        message: 'Too many failed attempts',
      };
      const authFlow = createUseAuthFlowMock({ state, failedAttempts: 5, canSubmit: false });

      render(<LoginPage authFlow={authFlow} />);

      expect(screen.getByText(/too many failed attempts/i)).toBeInTheDocument();
    });
  });

  describe('MFA state', () => {
    it('shows TOTP field when MFA is required', () => {
      const state: AuthFlowState = {
        status: 'mfa_required',
        config: mockConfig,
        email: 'test@example.com',
      };
      const authFlow = createUseAuthFlowMock({ state, showMfaField: true });

      render(<LoginPage authFlow={authFlow} />);

      expect(screen.getByLabelText(/totp code/i)).toBeInTheDocument();
    });
  });

  describe('authenticating state', () => {
    it('shows loading state during authentication', () => {
      const state: AuthFlowState = { status: 'authenticating', config: mockConfig };
      const authFlow = createUseAuthFlowMock({ state, canSubmit: false });

      render(<LoginPage authFlow={authFlow} />);

      // Button shows "Signing in..." when authenticating
      expect(screen.getByRole('button', { name: /signing in/i })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /signing in/i })).toBeDisabled();
    });
  });

  describe('form fields', () => {
    it('renders email and password input fields', () => {
      const authFlow = createUseAuthFlowMock();

      render(<LoginPage authFlow={authFlow} />);

      expect(screen.getByLabelText(/email/i)).toBeInTheDocument();
      expect(screen.getByLabelText(/password/i)).toBeInTheDocument();
    });

    it('disables submit button when fields are empty', () => {
      const authFlow = createUseAuthFlowMock();

      render(<LoginPage authFlow={authFlow} />);

      // Submit button should be disabled when form is empty
      const submitButton = screen.getByRole('button', { name: /sign in/i });
      expect(submitButton).toBeDisabled();
    });

    it('allows typing in email and password fields', async () => {
      const user = userEvent.setup();
      const authFlow = createUseAuthFlowMock();

      render(<LoginPage authFlow={authFlow} />);

      const emailInput = screen.getByLabelText(/email/i);
      const passwordInput = screen.getByLabelText(/password/i);

      await user.type(emailInput, 'test@example.com');
      await user.type(passwordInput, 'password123');

      expect(emailInput).toHaveValue('test@example.com');
      expect(passwordInput).toHaveValue('password123');
    });
  });

  describe('dev bypass', () => {
    it('calls devBypass when dev bypass button is clicked', async () => {
      const user = userEvent.setup();
      const authFlow = createUseAuthFlowMock({ devBypassAllowed: true });

      render(<LoginPage authFlow={authFlow} />);

      await user.click(screen.getByRole('button', { name: /dev bypass/i }));

      expect(authFlow.devBypass).toHaveBeenCalled();
    });
  });

  describe('TOTP field toggle', () => {
    it('shows TOTP field when "Add TOTP code" button is clicked', async () => {
      const user = userEvent.setup();
      const authFlow = createUseAuthFlowMock();

      render(<LoginPage authFlow={authFlow} />);

      // Initially no TOTP field visible (button to add it is there)
      expect(screen.queryByLabelText(/totp code/i)).not.toBeInTheDocument();
      expect(screen.getByRole('button', { name: /use totp code/i })).toBeInTheDocument();

      // Click the "Use TOTP code" button
      const showTotpButton = screen.getByRole('button', { name: /use totp code/i });
      await user.click(showTotpButton);

      // Now TOTP field should be visible
      expect(screen.getByLabelText(/totp code/i)).toBeInTheDocument();
    });
  });
});
