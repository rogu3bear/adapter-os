import { describe, it, expect, vi } from 'vitest';
import React from 'react';
import { render, screen } from '@testing-library/react';
import { ActiveAlertsWidget } from '@/components/dashboard/ActiveAlertsWidget';
import { LayoutProvider } from '@/layout/LayoutProvider';
import { MemoryRouter } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

vi.mock('@/api/client', () => {
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

describe('ActiveAlertsWidget', () => {
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
    // Matches the title from our mock data
    expect(await screen.findByText(/High latency/)).toBeTruthy();
  });
});
