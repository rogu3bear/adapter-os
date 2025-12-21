/**
 * useHealthPolling Hook Tests
 *
 * Basic tests for the health polling hook's initial state and exported interface.
 * More complex tests involving API mocking are in integration tests.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook } from '@testing-library/react';

// Mock the API client module
vi.mock('@/api/services', () => ({
  apiClient: {
    request: vi.fn(() => new Promise(() => {})), // Never resolves initially
  },
}));

// Mock the auth constants
vi.mock('@/auth/constants', () => ({
  AUTH_DEFAULTS: {
    HEALTH_CHECK_TIMEOUT: 5000,
    HEALTH_POLL_INTERVAL_READY: 10000,
    HEALTH_POLL_INTERVAL_DEGRADED: 2500,
  },
}));

describe('useHealthPolling', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('starts with checking status', async () => {
    // Import hook after mocks are set up
    const { useHealthPolling } = await import('@/hooks/auth/useHealthPolling');

    const { result } = renderHook(() => useHealthPolling());

    expect(result.current.backendStatus).toBe('checking');
    expect(result.current.isReady).toBe(false);
    expect(result.current.health).toBeNull();
    expect(result.current.systemHealth).toBeNull();
    expect(result.current.healthError).toBeNull();
  });

  it('exposes required interface', async () => {
    const { useHealthPolling } = await import('@/hooks/auth/useHealthPolling');

    const { result } = renderHook(() => useHealthPolling());

    // Verify the hook returns all expected properties
    expect(result.current).toHaveProperty('backendStatus');
    expect(result.current).toHaveProperty('health');
    expect(result.current).toHaveProperty('systemHealth');
    expect(result.current).toHaveProperty('healthError');
    expect(result.current).toHaveProperty('isReady');
    expect(result.current).toHaveProperty('issueComponents');
    expect(result.current).toHaveProperty('allComponents');
    expect(result.current).toHaveProperty('lastUpdated');
    expect(result.current).toHaveProperty('refresh');
    expect(typeof result.current.refresh).toBe('function');
  });

  it('initializes with empty collections', async () => {
    const { useHealthPolling } = await import('@/hooks/auth/useHealthPolling');

    const { result } = renderHook(() => useHealthPolling());

    expect(result.current.issueComponents).toEqual([]);
    expect(result.current.allComponents).toEqual({});
    expect(result.current.lastUpdated).toBeNull();
  });
});
