import { describe, it, expect, vi, beforeEach } from 'vitest';
import React from 'react';
import { render, screen } from '@testing-library/react';
import { ActiveAlertsWidget } from '@/components/dashboard/ActiveAlertsWidget';
import { LayoutProvider } from '@/layout/LayoutProvider';
import { MemoryRouter } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

vi.mock('@/api/client', () => {
  const mockObj = {
    // LayoutProvider requirements
    getToken: vi.fn(() => null),
    setToken: vi.fn(),
    getCurrentUser: vi.fn().mockResolvedValue({ user_id: 'u1', email: 'u@test.dev', role: 'viewer' }),
    listTenants: vi.fn().mockResolvedValue([]),
    // Alerts API
    listAlerts: vi.fn().mockResolvedValue([
      { id: '1', severity: 'high', title: 'High latency', message: 'p95 = 30ms', status: 'active', created_at: new Date().toISOString(), updated_at: new Date().toISOString() }
    ]),
    acknowledgeAlert: vi.fn().mockResolvedValue({}),
    subscribeToAlerts: vi.fn().mockReturnValue(() => {}),
  };
  return {
    __esModule: true,
    default: mockObj,
    apiClient: mockObj,
  };
});

describe('ActiveAlertsWidget', () => {
  beforeEach(() => {
    const localStorageMock = {
      getItem: vi.fn(),
      setItem: vi.fn(),
      removeItem: vi.fn(),
      clear: vi.fn(),
    };
    Object.defineProperty(window, 'localStorage', {
      value: localStorageMock,
      writable: true,
    });
    vi.stubGlobal('localStorage', window.localStorage);
  });

  it('renders alerts from API', async () => {
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <MemoryRouter>
        <QueryClientProvider client={queryClient}>
          <LayoutProvider>
            <ActiveAlertsWidget />
          </LayoutProvider>
        </QueryClientProvider>
      </MemoryRouter>
    );
    // Matches one of the built-in mock alert titles in the widget
    expect(await screen.findByText(/High latency/)).toBeTruthy();
  });
});
