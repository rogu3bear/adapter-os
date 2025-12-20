/**
 * Session Expiration Utilities
 *
 * Manages session expired state for cross-page communication.
 * Uses sessionStorage with in-memory fallback.
 */

import { AUTH_STORAGE_KEYS, AUTH_EVENTS } from './constants';

// Re-export for backward compatibility
export const SESSION_EXPIRED_FLAG_KEY = AUTH_STORAGE_KEYS.SESSION_EXPIRED;
export const SESSION_EXPIRED_EVENT = AUTH_EVENTS.SESSION_EXPIRED;

let memoryFlag = false;

export function markSessionExpired(): void {
  if (typeof sessionStorage !== 'undefined') {
    try {
      sessionStorage.setItem(AUTH_STORAGE_KEYS.SESSION_EXPIRED, '1');
      if (typeof window !== 'undefined') {
        window.dispatchEvent(new Event(AUTH_EVENTS.SESSION_EXPIRED));
      }
      return;
    } catch {
      // fall back to in-memory flag when storage is unavailable
    }
  }
  memoryFlag = true;
  if (typeof window !== 'undefined') {
    window.dispatchEvent(new Event(AUTH_EVENTS.SESSION_EXPIRED));
  }
}

export function clearSessionExpiredFlag(): void {
  if (typeof sessionStorage !== 'undefined') {
    try {
      sessionStorage.removeItem(AUTH_STORAGE_KEYS.SESSION_EXPIRED);
    } catch {
      // ignore storage errors; use in-memory flag as best effort
    }
  }
  memoryFlag = false;
}

/**
 * Check and clear the session-expired marker.
 * Returns a user-friendly message when a previous session was invalidated.
 */
export function consumeSessionExpiredFlag(): string | null {
  let expired = false;

  if (typeof sessionStorage !== 'undefined') {
    try {
      expired = sessionStorage.getItem(AUTH_STORAGE_KEYS.SESSION_EXPIRED) === '1';
      if (expired) {
        sessionStorage.removeItem(AUTH_STORAGE_KEYS.SESSION_EXPIRED);
      }
    } catch {
      // fall back to memory flag if sessionStorage is not accessible
    }
  }

  if (!expired && memoryFlag) {
    expired = true;
    memoryFlag = false;
  }

  if (expired) {
    return 'Session expired. Please log in again.';
  }

  return null;
}
