import React from 'react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, waitFor, act } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { FeatureProviders, useTenant } from '@/providers/FeatureProviders';

const mockListUserTenants = vi.hoisted(() => vi.fn());
const mockSwitchTenant = vi.hoisted(() => vi.fn());
const mockRefreshUser = vi.hoisted(() => vi.fn());
const mockUser = vi.hoisted(() => ({ id: 'user-1', tenant_id: 't1' }));

vi.mock('@/api/services', () => ({
  apiClient: {
    listUserTenants: (...args: unknown[]) => mockListUserTenants(...args),
    switchTenant: (...args: unknown[]) => mockSwitchTenant(...args),
  },
}));

const TENANT_SELECTION_REQUIRED_KEY = vi.hoisted(() => 'aos-tenant-selection-required');

vi.mock('@/providers/CoreProviders', () => ({
  useAuth: () => ({
    user: mockUser,
    refreshUser: mockRefreshUser,
  }),
  TENANT_SELECTION_REQUIRED_KEY,
}));

function TenantHarness({ onReady }: { onReady: (ctx: ReturnType<typeof useTenant>) => void }) {
  const ctx = useTenant();
  React.useEffect(() => {
    onReady(ctx);
  }, [ctx, onReady]);
  return null;
}

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });

  return ({ children }: { children: React.ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );
}

describe('FeatureProviders / TenantProvider', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    sessionStorage.clear();
  });

  it('avoids switch POST when selecting the already active tenant and clears selection-required flag', async () => {
    sessionStorage.setItem('selectedTenant', JSON.stringify({ tenantId: 't1', userId: 'user-1' }));
    mockListUserTenants.mockResolvedValue([
      { id: 't1', name: 'Tenant One' },
      { id: 't2', name: 'Tenant Two' },
    ]);
    mockSwitchTenant.mockResolvedValue({ tenant_id: 't1', tenants: [] });

    let ctx: ReturnType<typeof useTenant> | null = null;
    const Wrapper = createWrapper();
    render(
      <Wrapper>
        <FeatureProviders>
          <TenantHarness onReady={(value) => { ctx = value; }} />
        </FeatureProviders>
      </Wrapper>
    );

    await waitFor(() => expect(mockListUserTenants).toHaveBeenCalled());
    await waitFor(() => expect(ctx).not.toBeNull());

    let ok = false;
    await act(async () => {
      ok = await ctx!.setSelectedTenant('t1');
    });

    expect(ok).toBe(true);
    expect(mockSwitchTenant).not.toHaveBeenCalled();
    expect(sessionStorage.getItem(TENANT_SELECTION_REQUIRED_KEY)).toBeNull();
    expect(JSON.parse(sessionStorage.getItem('selectedTenant') ?? '{}')).toMatchObject({
      tenantId: 't1',
      userId: 'user-1',
    });
  });

  it('switches tenants once for a different tenant and clears selection-required flag', async () => {
    sessionStorage.setItem('selectedTenant', JSON.stringify({ tenantId: 't1', userId: 'user-1' }));
    mockListUserTenants.mockResolvedValue([
      { id: 't1', name: 'Tenant One' },
      { id: 't2', name: 'Tenant Two' },
    ]);
    mockSwitchTenant.mockResolvedValue({
      tenant_id: 't2',
      tenants: [
        { id: 't1', name: 'Tenant One' },
        { id: 't2', name: 'Tenant Two' },
      ],
    });

    let ctx: ReturnType<typeof useTenant> | null = null;
    const Wrapper = createWrapper();
    render(
      <Wrapper>
        <FeatureProviders>
          <TenantHarness onReady={(value) => { ctx = value; }} />
        </FeatureProviders>
      </Wrapper>
    );

    await waitFor(() => expect(mockListUserTenants).toHaveBeenCalled());
    await waitFor(() => expect(ctx).not.toBeNull());

    let ok = false;
    await act(async () => {
      ok = await ctx!.setSelectedTenant('t2');
    });

    expect(ok).toBe(true);
    expect(mockSwitchTenant).toHaveBeenCalledTimes(1);
    expect(mockSwitchTenant).toHaveBeenCalledWith('t2');
    expect(sessionStorage.getItem(TENANT_SELECTION_REQUIRED_KEY)).toBeNull();
    expect(JSON.parse(sessionStorage.getItem('selectedTenant') ?? '{}')).toMatchObject({
      tenantId: 't2',
      userId: 'user-1',
    });
  });
});
