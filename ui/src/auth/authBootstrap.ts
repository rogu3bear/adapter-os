import type { UserInfoResponse } from '@/api/auth-types';
import { logger } from '@/utils/logger';
import { AUTH_STORAGE_KEYS, AUTH_DEFAULTS } from './constants';

export function isDevBypassEnabled(): boolean {
  const env = typeof import.meta !== 'undefined' ? import.meta.env : undefined;
  if (env?.PROD === true) return false;
  const devMode = env?.DEV === true;
  const explicitFlag = env?.VITE_ENABLE_DEV_BYPASS === 'true';
  return Boolean(devMode || explicitFlag);
}

function readSelectedTenantId(): string | null {
  if (typeof window === 'undefined') return null;
  try {
    const raw = sessionStorage.getItem(AUTH_STORAGE_KEYS.SELECTED_TENANT);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as { tenantId?: unknown };
    const tenantId = typeof parsed.tenantId === 'string' ? parsed.tenantId.trim() : '';
    return tenantId ? tenantId : null;
  } catch (error) {
    logger.warn('Failed to read selected tenant ID from session storage', { component: 'authBootstrap' }, error instanceof Error ? error : undefined);
    return null;
  }
}

/**
 * Record the timestamp when dev bypass was activated.
 * Used for enforcing the 1-hour session timeout.
 */
export function markDevBypassActivated(): void {
  try {
    localStorage.setItem(AUTH_STORAGE_KEYS.DEV_BYPASS_ACTIVATED_AT, Date.now().toString());
    logger.debug('Dev bypass activation timestamp recorded', { component: 'authBootstrap' });
  } catch (error) {
    logger.warn('Failed to record dev bypass activation timestamp', { component: 'authBootstrap' }, error instanceof Error ? error : undefined);
  }
}

/**
 * Clear the dev bypass activation timestamp.
 * Called on logout or when session expires.
 */
export function clearDevBypassTimestamp(): void {
  try {
    localStorage.removeItem(AUTH_STORAGE_KEYS.DEV_BYPASS_ACTIVATED_AT);
  } catch (error) {
    logger.warn('Failed to clear dev bypass activation timestamp', { component: 'authBootstrap' }, error instanceof Error ? error : undefined);
  }
}

/**
 * Check if the dev bypass session has expired (1 hour timeout).
 * Returns true if expired or if no timestamp exists.
 */
export function isDevBypassExpired(): boolean {
  try {
    const activatedAt = localStorage.getItem(AUTH_STORAGE_KEYS.DEV_BYPASS_ACTIVATED_AT);
    if (!activatedAt) {
      return true; // No timestamp means not activated or cleared
    }
    const elapsed = Date.now() - parseInt(activatedAt, 10);
    return elapsed > AUTH_DEFAULTS.DEV_BYPASS_TIMEOUT_MS;
  } catch (error) {
    logger.warn('Failed to check dev bypass expiration status', { component: 'authBootstrap' }, error instanceof Error ? error : undefined);
    return true; // Treat storage errors as expired
  }
}

/**
 * Get the remaining time in milliseconds before dev bypass expires.
 * Returns 0 if expired or no timestamp exists.
 */
export function getDevBypassRemainingMs(): number {
  try {
    const activatedAt = localStorage.getItem(AUTH_STORAGE_KEYS.DEV_BYPASS_ACTIVATED_AT);
    if (!activatedAt) {
      return 0;
    }
    const elapsed = Date.now() - parseInt(activatedAt, 10);
    const remaining = AUTH_DEFAULTS.DEV_BYPASS_TIMEOUT_MS - elapsed;
    return Math.max(0, remaining);
  } catch (error) {
    logger.warn('Failed to get dev bypass remaining time', { component: 'authBootstrap' }, error instanceof Error ? error : undefined);
    return 0;
  }
}

/**
 * Attempt to detect dev bypass by calling /auth/me without prior login.
 * Returns claims when the server reports admin with wildcard tenants.
 */
export async function tryDevBypassLogin(): Promise<UserInfoResponse | null> {
  const devBypassEnabled = isDevBypassEnabled();
  logger.debug(`[DEV-BYPASS] isDevBypassEnabled: ${devBypassEnabled}`, { component: 'authBootstrap' });

  if (!devBypassEnabled) {
    logger.debug('Dev bypass disabled by env; skipping bootstrap', { component: 'authBootstrap' });
    return null;
  }

  try {
    const tenantId = readSelectedTenantId();
    logger.debug('[DEV-BYPASS] Fetching /api/v1/auth/me...', { component: 'authBootstrap' });
    const res = await fetch('/api/v1/auth/me', {
      credentials: 'include',
      ...(tenantId ? { headers: { 'X-Tenant-Id': tenantId } } : {}),
    });
    logger.debug(`[DEV-BYPASS] Response status: ${res.status}, ok: ${res.ok}`, { component: 'authBootstrap' });

    if (!res.ok) {
      logger.debug('[DEV-BYPASS] Response not OK, returning null', { component: 'authBootstrap' });
      return null;
    }

    const claims = (await res.json()) as UserInfoResponse;
    logger.debug(`[DEV-BYPASS] Claims: ${JSON.stringify(claims, null, 2)}`, { component: 'authBootstrap' });
    const { role, admin_tenants } = claims;

    const isDevBypass =
      typeof role === 'string' &&
      role.toLowerCase() === 'admin' &&
      Array.isArray(admin_tenants) &&
      admin_tenants.includes('*');

    logger.debug(`[DEV-BYPASS] Check result: ${JSON.stringify({ role, admin_tenants, isDevBypass })}`, { component: 'authBootstrap' });

    if (isDevBypass) {
      logger.debug('Dev bypass bootstrap activated', { component: 'authBootstrap' });
      logger.debug('[DEV-BYPASS] ✓ Returning claims for dev bypass', { component: 'authBootstrap' });
      return claims;
    }

    logger.debug('[DEV-BYPASS] ✗ Not a dev bypass response', { component: 'authBootstrap' });
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
