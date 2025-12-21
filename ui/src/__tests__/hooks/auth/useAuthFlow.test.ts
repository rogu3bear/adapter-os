/**
 * useAuthFlow Hook Tests
 *
 * Tests the auth flow state machine transitions.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, waitFor, act } from '@testing-library/react';
import type { AuthFlowState } from '@/hooks/auth/useAuthFlow';

// Mock modules before importing the hook
const mockLogin = vi.fn();
const mockDevBypassLogin = vi.fn();
const mockRequest = vi.fn();

vi.mock('@/providers/CoreProviders', () => ({
  useAuth: () => ({
    user: null,
    login: mockLogin,
    devBypassLogin: mockDevBypassLogin,
    sessionMode: 'normal',
  }),
}));

vi.mock('@/api/services', () => ({
  apiClient: {
    getAuthConfig: (signal?: AbortSignal) => mockRequest(signal),
  },
}));

vi.mock('@/auth/session', () => ({
  consumeSessionExpiredFlag: vi.fn(() => null),
}));

vi.mock('@/auth/authBootstrap', () => ({
  isDevBypassEnabled: vi.fn(() => true),
}));

vi.mock('@/config/demo', () => ({
  isDemoMvpMode: vi.fn(() => false),
  getDemoEntryPath: vi.fn(() => '/demo'),
}));

vi.mock('@/utils/logger', () => ({
  logger: {
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
  },
  toError: (err: unknown) => (err instanceof Error ? err : new Error(String(err))),
}));

// Mock health polling hook
const mockHealthPolling = {
  backendStatus: 'ready' as const,
  isReady: true,
  healthError: null,
  health: { status: 'healthy', components: {} },
  systemHealth: { status: 'healthy', components: {} },
  allComponents: {},
  issueComponents: [],
  lastUpdated: 'just now',
  refresh: vi.fn(),
};

vi.mock('@/hooks/auth/useHealthPolling', () => ({
  useHealthPolling: () => mockHealthPolling,
}));

// Import after mocks
import { useAuthFlow } from '@/hooks/auth/useAuthFlow';
import { consumeSessionExpiredFlag } from '@/auth/session';

// Test data
const mockAuthConfig = {
  mfa_required: false,
  max_login_attempts: 5,
  access_token_ttl_minutes: 60,
  session_timeout_minutes: 1440,
  dev_bypass_allowed: true,
};

const mockLoginResponse = {
  user_id: 'user-1',
  tenant_id: 'tenant-1',
  role: 'admin',
  token: 'mock-token',
};

describe('useAuthFlow', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockHealthPolling.backendStatus = 'ready';
    mockHealthPolling.isReady = true;
    mockHealthPolling.healthError = null;
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe('initialization', () => {
    it('starts in checking_health state', () => {
      mockHealthPolling.isReady = false;
      mockHealthPolling.backendStatus = 'checking';

      const { result } = renderHook(() => useAuthFlow());

      expect(result.current.state.status).toBe('checking_health');
    });

    it('loads config when health becomes ready', async () => {
      mockRequest.mockResolvedValue(mockAuthConfig);

      const { result } = renderHook(() => useAuthFlow());

      await waitFor(() => {
        expect(result.current.state.status).toBe('ready');
      });

      expect(mockRequest).toHaveBeenCalled();
    });

    it('transitions to config_error on config load failure', async () => {
      mockRequest.mockRejectedValue(new Error('Config load failed'));

      const { result } = renderHook(() => useAuthFlow());

      await waitFor(() => {
        expect(result.current.state.status).toBe('config_error');
      });

      if (result.current.state.status === 'config_error') {
        expect(result.current.state.error).toContain('Unable to load sign-in settings');
      }
    });

    it('shows session expired message if flag was set', async () => {
      vi.mocked(consumeSessionExpiredFlag).mockReturnValueOnce(
        'Session expired. Please log in again.'
      );
      mockRequest.mockResolvedValue(mockAuthConfig);

      const { result } = renderHook(() => useAuthFlow());

      await waitFor(() => {
        expect(result.current.state.status).toBe('error');
      });

      if (result.current.state.status === 'error') {
        expect(result.current.state.error.message).toContain('Session expired');
      }
    });
  });

  describe('login flow', () => {
    beforeEach(() => {
      mockRequest.mockResolvedValue(mockAuthConfig);
    });

    it('transitions to authenticating on login', async () => {
      const { result } = renderHook(() => useAuthFlow());

      await waitFor(() => {
        expect(result.current.state.status).toBe('ready');
      });

      mockLogin.mockImplementation(() => new Promise(() => {})); // Never resolves

      act(() => {
        result.current.login({ email: 'test@example.com', password: 'password' });
      });

      expect(result.current.state.status).toBe('authenticating');
    });

    it('transitions to success on successful login', async () => {
      mockLogin.mockResolvedValue(mockLoginResponse);

      const { result } = renderHook(() => useAuthFlow());

      await waitFor(() => {
        expect(result.current.state.status).toBe('ready');
      });

      await act(async () => {
        await result.current.login({ email: 'test@example.com', password: 'password' });
      });

      expect(result.current.state.status).toBe('success');
      if (result.current.state.status === 'success') {
        expect(result.current.state.redirectPath).toBe('/dashboard');
      }
    });

    it('transitions to error on login failure', async () => {
      mockLogin.mockRejectedValue(new Error('Invalid credentials'));

      const { result } = renderHook(() => useAuthFlow());

      await waitFor(() => {
        expect(result.current.state.status).toBe('ready');
      });

      await act(async () => {
        await result.current.login({ email: 'test@example.com', password: 'wrong' });
      });

      expect(result.current.state.status).toBe('error');
      expect(result.current.failedAttempts).toBe(1);
    });

    it('increments failed attempts on each failure', async () => {
      mockLogin.mockRejectedValue(new Error('Invalid credentials'));

      const { result } = renderHook(() => useAuthFlow());

      await waitFor(() => {
        expect(result.current.state.status).toBe('ready');
      });

      // First attempt
      await act(async () => {
        await result.current.login({ email: 'test@example.com', password: 'wrong' });
      });
      expect(result.current.failedAttempts).toBe(1);

      // Clear error for next attempt
      act(() => {
        result.current.clearError();
      });

      // Second attempt
      await act(async () => {
        await result.current.login({ email: 'test@example.com', password: 'wrong' });
      });
      expect(result.current.failedAttempts).toBe(2);
    });

    it('transitions to locked_out after max attempts', async () => {
      mockLogin.mockRejectedValue(new Error('Invalid credentials'));

      const { result } = renderHook(() => useAuthFlow());

      await waitFor(() => {
        expect(result.current.state.status).toBe('ready');
      });

      // Exhaust attempts (max is 5)
      for (let i = 0; i < 5; i++) {
        await act(async () => {
          await result.current.login({ email: 'test@example.com', password: 'wrong' });
        });
        if (result.current.state.status === 'error') {
          act(() => {
            result.current.clearError();
          });
        }
      }

      expect(result.current.state.status).toBe('locked_out');
      expect(result.current.failedAttempts).toBe(5);
    });
  });

  describe('MFA flow', () => {
    beforeEach(() => {
      mockRequest.mockResolvedValue(mockAuthConfig);
    });

    it('transitions to mfa_required when MFA is needed', async () => {
      const mfaError = new Error('MFA required');
      (mfaError as Error & { code?: string }).code = 'MFA_REQUIRED';
      mockLogin.mockRejectedValue(mfaError);

      const { result } = renderHook(() => useAuthFlow());

      await waitFor(() => {
        expect(result.current.state.status).toBe('ready');
      });

      await act(async () => {
        await result.current.login({ email: 'test@example.com', password: 'password' });
      });

      expect(result.current.state.status).toBe('mfa_required');
      expect(result.current.showMfaField).toBe(true);
    });

    it('shows MFA field when config requires it', async () => {
      mockRequest.mockResolvedValue({ ...mockAuthConfig, mfa_required: true });

      const { result } = renderHook(() => useAuthFlow());

      await waitFor(() => {
        expect(result.current.state.status).toBe('ready');
      });

      expect(result.current.showMfaField).toBe(true);
    });
  });

  describe('dev bypass', () => {
    beforeEach(() => {
      mockRequest.mockResolvedValue(mockAuthConfig);
    });

    it('reports devBypassAllowed when enabled', async () => {
      const { result } = renderHook(() => useAuthFlow());

      await waitFor(() => {
        expect(result.current.state.status).toBe('ready');
      });

      expect(result.current.devBypassAllowed).toBe(true);
    });

    it('transitions to success on successful dev bypass', async () => {
      mockDevBypassLogin.mockResolvedValue(mockLoginResponse);

      const { result } = renderHook(() => useAuthFlow());

      await waitFor(() => {
        expect(result.current.state.status).toBe('ready');
      });

      await act(async () => {
        await result.current.devBypass();
      });

      expect(result.current.state.status).toBe('success');
    });

    it('transitions to error on dev bypass failure', async () => {
      mockDevBypassLogin.mockRejectedValue(new Error('Dev bypass failed'));

      const { result } = renderHook(() => useAuthFlow());

      await waitFor(() => {
        expect(result.current.state.status).toBe('ready');
      });

      await act(async () => {
        await result.current.devBypass();
      });

      expect(result.current.state.status).toBe('error');
    });
  });

  describe('clearError', () => {
    beforeEach(() => {
      mockRequest.mockResolvedValue(mockAuthConfig);
    });

    it('clears error and returns to ready state', async () => {
      mockLogin.mockRejectedValue(new Error('Invalid credentials'));

      const { result } = renderHook(() => useAuthFlow());

      await waitFor(() => {
        expect(result.current.state.status).toBe('ready');
      });

      await act(async () => {
        await result.current.login({ email: 'test@example.com', password: 'wrong' });
      });

      expect(result.current.state.status).toBe('error');

      act(() => {
        result.current.clearError();
      });

      expect(result.current.state.status).toBe('ready');
    });
  });

  describe('retryConfig', () => {
    it('retries config loading after config_error', async () => {
      mockRequest.mockRejectedValueOnce(new Error('Failed')).mockResolvedValueOnce(mockAuthConfig);

      const { result } = renderHook(() => useAuthFlow());

      await waitFor(() => {
        expect(result.current.state.status).toBe('config_error');
      });

      await act(async () => {
        await result.current.retryConfig();
      });

      expect(result.current.state.status).toBe('ready');
    });
  });

  describe('canSubmit', () => {
    beforeEach(() => {
      mockRequest.mockResolvedValue(mockAuthConfig);
    });

    it('is true when ready', async () => {
      const { result } = renderHook(() => useAuthFlow());

      await waitFor(() => {
        expect(result.current.state.status).toBe('ready');
      });

      expect(result.current.canSubmit).toBe(true);
    });

    it('is false when locked out', async () => {
      mockLogin.mockRejectedValue(new Error('Invalid credentials'));

      const { result } = renderHook(() => useAuthFlow());

      await waitFor(() => {
        expect(result.current.state.status).toBe('ready');
      });

      // Exhaust attempts
      for (let i = 0; i < 5; i++) {
        await act(async () => {
          await result.current.login({ email: 'test@example.com', password: 'wrong' });
        });
        if (result.current.state.status === 'error') {
          act(() => {
            result.current.clearError();
          });
        }
      }

      expect(result.current.canSubmit).toBe(false);
    });

    it('is false when authenticating', async () => {
      mockLogin.mockImplementation(() => new Promise(() => {}));

      const { result } = renderHook(() => useAuthFlow());

      await waitFor(() => {
        expect(result.current.state.status).toBe('ready');
      });

      act(() => {
        result.current.login({ email: 'test@example.com', password: 'password' });
      });

      expect(result.current.canSubmit).toBe(false);
    });
  });

  describe('maxAttempts', () => {
    it('uses config value when available', async () => {
      mockRequest.mockResolvedValue({ ...mockAuthConfig, max_login_attempts: 3 });

      const { result } = renderHook(() => useAuthFlow());

      await waitFor(() => {
        expect(result.current.state.status).toBe('ready');
      });

      expect(result.current.maxAttempts).toBe(3);
    });

    it('uses default when config not loaded', () => {
      mockHealthPolling.isReady = false;
      mockHealthPolling.backendStatus = 'checking';

      const { result } = renderHook(() => useAuthFlow());

      expect(result.current.maxAttempts).toBe(5); // Default value
    });
  });
});
