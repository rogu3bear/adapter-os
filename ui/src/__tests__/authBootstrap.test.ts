import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { UserInfoResponse } from '@/api/auth-types';
import { tryDevBypassLogin } from '@/auth/authBootstrap';
import { logger } from '@/utils/logger';

describe('tryDevBypassLogin', () => {
  const originalFetch = global.fetch;

  const baseClaims: UserInfoResponse = {
    schema_version: '1',
    user_id: 'user-1',
    email: 'user@example.com',
    role: 'admin',
    created_at: '2024-01-01T00:00:00Z',
    admin_tenants: ['*'],
  };

  beforeEach(() => {
    vi.resetAllMocks();
  });

  afterEach(() => {
    global.fetch = originalFetch!;
  });

  it('returns claims when admin with wildcard tenants', async () => {
    const claims = { ...baseClaims };
    const debugSpy = vi.spyOn(logger, 'debug').mockImplementation(() => {});

    global.fetch = vi.fn().mockResolvedValue({
      ok: true,
      json: vi.fn().mockResolvedValue(claims),
    });

    const result = await tryDevBypassLogin();

    expect(result).toEqual(claims);
    expect(global.fetch).toHaveBeenCalledWith('/api/v1/auth/me', { credentials: 'include' });
    expect(debugSpy).toHaveBeenCalledWith(
      'Dev bypass bootstrap activated',
      { component: 'authBootstrap' },
    );
  });

  it('returns null when admin_tenants lacks wildcard', async () => {
    const claims = { ...baseClaims, admin_tenants: ['tenant-1'] };
    const debugSpy = vi.spyOn(logger, 'debug').mockImplementation(() => {});
    global.fetch = vi.fn().mockResolvedValue({
      ok: true,
      json: vi.fn().mockResolvedValue(claims),
    });

    const result = await tryDevBypassLogin();

    expect(result).toBeNull();
    expect(debugSpy).not.toHaveBeenCalledWith(
      'Dev bypass bootstrap activated',
      { component: 'authBootstrap' },
    );
  });

  it('returns null when role is not admin even with wildcard tenants', async () => {
    const claims = { ...baseClaims, role: 'user' };
    const debugSpy = vi.spyOn(logger, 'debug').mockImplementation(() => {});
    global.fetch = vi.fn().mockResolvedValue({
      ok: true,
      json: vi.fn().mockResolvedValue(claims),
    });

    const result = await tryDevBypassLogin();

    expect(result).toBeNull();
    expect(debugSpy).not.toHaveBeenCalledWith(
      'Dev bypass bootstrap activated',
      { component: 'authBootstrap' },
    );
  });

  it('returns null when response is not ok', async () => {
    const debugSpy = vi.spyOn(logger, 'debug').mockImplementation(() => {});
    global.fetch = vi.fn().mockResolvedValue({
      ok: false,
      json: vi.fn(),
    });

    const result = await tryDevBypassLogin();

    expect(result).toBeNull();
    expect(debugSpy).not.toHaveBeenCalledWith(
      'Dev bypass bootstrap activated',
      { component: 'authBootstrap' },
    );
  });

  it('returns null when fetch rejects', async () => {
    const debugSpy = vi.spyOn(logger, 'debug').mockImplementation(() => {});
    global.fetch = vi.fn().mockRejectedValue(new Error('network down'));

    const result = await tryDevBypassLogin();

    expect(result).toBeNull();
    expect(debugSpy).not.toHaveBeenCalledWith(
      'Dev bypass bootstrap activated',
      { component: 'authBootstrap' },
    );
  });
});

