import type { UserInfoResponse } from '@/api/auth-types';
import { logger } from '@/utils/logger';

/**
 * Attempt to detect dev bypass by calling /auth/me without prior login.
 * Returns claims when the server reports admin with wildcard tenants.
 */
export async function tryDevBypassLogin(): Promise<UserInfoResponse | null> {
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
    logger.debug(
      'Dev bypass bootstrap check failed; continuing with normal auth',
      { component: 'authBootstrap' },
      error instanceof Error ? error : undefined,
    );
    return null;
  }
}

