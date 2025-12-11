export const SESSION_EXPIRED_FLAG_KEY = 'aos-session-expired';
export const SESSION_EXPIRED_EVENT = 'aos:session-expired';

let memoryFlag = false;

export function markSessionExpired(): void {
  if (typeof sessionStorage !== 'undefined') {
    try {
      sessionStorage.setItem(SESSION_EXPIRED_FLAG_KEY, '1');
      if (typeof window !== 'undefined') {
        window.dispatchEvent(new Event(SESSION_EXPIRED_EVENT));
      }
      return;
    } catch {
      // fall back to in-memory flag when storage is unavailable
    }
  }
  memoryFlag = true;
  if (typeof window !== 'undefined') {
    window.dispatchEvent(new Event(SESSION_EXPIRED_EVENT));
  }
}

export function clearSessionExpiredFlag(): void {
  if (typeof sessionStorage !== 'undefined') {
    try {
      sessionStorage.removeItem(SESSION_EXPIRED_FLAG_KEY);
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
      expired = sessionStorage.getItem(SESSION_EXPIRED_FLAG_KEY) === '1';
      if (expired) {
        sessionStorage.removeItem(SESSION_EXPIRED_FLAG_KEY);
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

