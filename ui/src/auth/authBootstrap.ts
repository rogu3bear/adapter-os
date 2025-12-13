import type { UserInfoResponse } from '@/api/auth-types';
import { logger } from '@/utils/logger';

export function isDevBypassEnabled(): boolean {
  // Dev bypass environment matrix: see docs/AUTHENTICATION.md (Dev bypass policy)
  const env = typeof import.meta !== 'undefined' ? import.meta.env : undefined;
  const devMode = env?.DEV === true;
  const explicitFlag = env?.VITE_ENABLE_DEV_BYPASS === 'true';
  return Boolean(devMode || explicitFlag);
}

/**
 * Attempt to detect dev bypass by calling /auth/me without prior login.
 * Returns claims when the server reports admin with wildcard tenants.
 */
export async function tryDevBypassLogin(): Promise<UserInfoResponse | null> {
  if (!isDevBypassEnabled()) {
    logger.debug('Dev bypass disabled by env; skipping bootstrap', { component: 'authBootstrap' });
    return null;
  }

  try {
    const res = await fetch('/api/v1/auth/me', { credentials: 'include' });
    if (!res.ok) {
      return null;
    }

    const claims = (await res.json()) as UserInfoResponse;
    const { role, admin_tenants } = claims;

    const isDevBypass =
      typeof role === 'string' &&
      role.toLowerCase() === 'admin' &&
      Array.isArray(admin_tenants) &&
      admin_tenants.includes('*');

    if (isDevBypass) {
      logger.debug('Dev bypass bootstrap activated', { component: 'authBootstrap' });
      return claims;
    }

    return null;
  } catch (error) {
    logger.error(
      'Dev bypass bootstrap check failed; continuing with normal auth',
      { component: 'authBootstrap' },
      error instanceof Error ? error : undefined,
    );
    return null;
  }
}

