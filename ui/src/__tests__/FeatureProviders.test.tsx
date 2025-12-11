import React from 'react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, waitFor } from '@testing-library/react';
import { FeatureProviders, useTenant } from '@/providers/FeatureProviders';

const mockListUserTenants = vi.hoisted(() => vi.fn());
const mockSwitchTenant = vi.hoisted(() => vi.fn());
const mockRefreshUser = vi.hoisted(() => vi.fn());

vi.mock('@/api/client', () => ({
  apiClient: {
    listUserTenants: (...args: unknown[]) => mockListUserTenants(...args),
    switchTenant: (...args: unknown[]) => mockSwitchTenant(...args),
  },
}));

const TENANT_SELECTION_REQUIRED_KEY = vi.hoisted(() => 'aos-tenant-selection-required');

vi.mock('@/providers/CoreProviders', () => ({
  useAuth: () => ({
    user: { id: 'user-1', tenant_id: 't1' },
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

describe('FeatureProviders / TenantProvider', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    sessionStorage.clear();
  });

  it('avoids switch POST when selecting the already active tenant and clears selection-required flag', async () => {
    localStorage.setItem('selectedTenant', 't1');
    mockListUserTenants.mockResolvedValue([
      { id: 't1', name: 'Tenant One' },
      { id: 't2', name: 'Tenant Two' },
    ]);
    mockSwitchTenant.mockResolvedValue({ tenant_id: 't1', tenants: [] });

    let ctx: ReturnType<typeof useTenant> | null = null;
    render(
      <FeatureProviders>
        <TenantHarness onReady={(value) => { ctx = value; }} />
      </FeatureProviders>
    );

    await waitFor(() => expect(mockListUserTenants).toHaveBeenCalled());
    await waitFor(() => expect(ctx).not.toBeNull());

    const ok = await ctx!.setSelectedTenant('t1');

    expect(ok).toBe(true);
    expect(mockSwitchTenant).not.toHaveBeenCalled();
    expect(sessionStorage.getItem(TENANT_SELECTION_REQUIRED_KEY)).toBeNull();
    expect(localStorage.getItem('selectedTenant')).toBe('t1');
  });

  it('switches tenants once for a different tenant and clears selection-required flag', async () => {
    localStorage.setItem('selectedTenant', 't1');
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
    render(
      <FeatureProviders>
        <TenantHarness onReady={(value) => { ctx = value; }} />
      </FeatureProviders>
    );

    await waitFor(() => expect(mockListUserTenants).toHaveBeenCalled());
    await waitFor(() => expect(ctx).not.toBeNull());

    const ok = await ctx!.setSelectedTenant('t2');

    expect(ok).toBe(true);
    expect(mockSwitchTenant).toHaveBeenCalledTimes(1);
    expect(mockSwitchTenant).toHaveBeenCalledWith('t2');
    expect(sessionStorage.getItem(TENANT_SELECTION_REQUIRED_KEY)).toBeNull();
    expect(localStorage.getItem('selectedTenant')).toBe('t2');
  });
});

