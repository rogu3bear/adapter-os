import { describe, it, expect, vi, beforeEach } from 'vitest';
import React from 'react';
import { render, screen, fireEvent } from '@testing-library/react';
import { ActiveAlertsWidget } from '@/components/dashboard/ActiveAlertsWidget';
import { MemoryRouter } from 'react-router-dom';

const refetchMock = vi.hoisted(() => vi.fn());

vi.mock('@/api/services', () => {
  return {
    __esModule: true,
    default: {
      // LayoutProvider requirements
      getToken: vi.fn(() => null),
      setToken: vi.fn(),
      getCurrentUser: vi.fn().mockResolvedValue({ user_id: 'u1', email: 'u@test.dev', role: 'viewer' }),
      // Return a tenant so TenantProvider sets selectedTenant, enabling alert fetching
      listTenants: vi.fn().mockResolvedValue([{ id: 'test-tenant', name: 'Test Tenant' }]),
      getStatus: vi.fn().mockResolvedValue({ status: 'healthy', services: [] }),
      // Alerts API
      listAlerts: vi.fn().mockResolvedValue([
        { id: '1', severity: 'high', title: 'High latency', message: 'p95 = 30ms', status: 'active', created_at: new Date().toISOString(), updated_at: new Date().toISOString() },
        { id: '2', severity: 'critical', title: 'Memory pressure', message: 'Memory at 95%', status: 'active', created_at: new Date().toISOString(), updated_at: new Date().toISOString() },
        { id: '3', severity: 'medium', title: 'Queue backlog', message: 'Tasks pending', status: 'active', created_at: new Date().toISOString(), updated_at: new Date().toISOString() }
      ]),
      acknowledgeAlert: vi.fn().mockResolvedValue({}),
      subscribeToAlerts: vi.fn().mockReturnValue(() => {}),
    },
    // Also export apiClient for FeatureProviders import
    apiClient: {
      getToken: vi.fn(() => null),
      setToken: vi.fn(),
      getCurrentUser: vi.fn().mockResolvedValue({ user_id: 'u1', email: 'u@test.dev', role: 'viewer' }),
      listTenants: vi.fn().mockResolvedValue([{ id: 'test-tenant', name: 'Test Tenant' }]),
      getStatus: vi.fn().mockResolvedValue({ status: 'healthy', services: [] }),
      listAlerts: vi.fn().mockResolvedValue([
        { id: '1', severity: 'high', title: 'High latency', message: 'p95 = 30ms', status: 'active', created_at: new Date().toISOString(), updated_at: new Date().toISOString() },
        { id: '2', severity: 'critical', title: 'Memory pressure', message: 'Memory at 95%', status: 'active', created_at: new Date().toISOString(), updated_at: new Date().toISOString() },
        { id: '3', severity: 'medium', title: 'Queue backlog', message: 'Tasks pending', status: 'active', created_at: new Date().toISOString(), updated_at: new Date().toISOString() }
      ]),
      acknowledgeAlert: vi.fn().mockResolvedValue({}),
      subscribeToAlerts: vi.fn().mockReturnValue(() => {}),
    },
  };
});

vi.mock('@/hooks/realtime/usePolling', () => ({
  usePolling: vi.fn().mockReturnValue({
    data: [
      { id: '1', severity: 'high', title: 'High latency', message: 'p95 = 30ms', status: 'active', created_at: new Date().toISOString(), updated_at: new Date().toISOString() },
    ],
    isLoading: false,
    error: null,
    lastUpdated: new Date('2025-01-01'),
    refetch: refetchMock,
  }),
}));

vi.mock('@/hooks/system/useServiceStatus', () => ({
  useServiceStatus: () => ({
    status: { services: [] },
    isLoading: false,
    lastUpdated: null,
    refetch: vi.fn(),
    error: null,
  }),
}));

vi.mock('@/providers/FeatureProviders', () => ({
  useTenant: () => ({ selectedTenant: 'test-tenant' }),
}));

vi.mock('@/hooks/ui/useTimestamp', () => ({
  useRelativeTime: () => 'moments ago',
}));

beforeEach(() => {
  refetchMock.mockClear();
});

describe('ActiveAlertsWidget', () => {
  it('renders alerts from API', async () => {
    render(
      <MemoryRouter>
        <ActiveAlertsWidget />
      </MemoryRouter>
    );
    // Matches the title from our mock data
    expect(await screen.findByText(/High latency/)).toBeTruthy();
  });

  it('invokes refresh handler when refresh clicked', async () => {
    render(
      <MemoryRouter>
        <ActiveAlertsWidget />
      </MemoryRouter>
    );

    const refreshButton = await screen.findByRole('button', { name: /Refresh/ });
    fireEvent.click(refreshButton);
    expect(refetchMock).toHaveBeenCalled();
  });
});
