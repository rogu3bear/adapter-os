import { useEffect, useRef } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import { SESSION_EXPIRED_FLAG_KEY, SESSION_EXPIRED_EVENT, consumeSessionExpiredFlag, markSessionExpired } from '@/auth/session';
import { useToastQueue } from '@/components/toast/ToastProvider';
import { useAuth } from '@/providers/CoreProviders';
import { logger, toError } from '@/utils/logger';

const POST_LOGIN_REDIRECT_KEY = 'postLoginRedirect';
const SESSION_EXPIRED_TOAST = 'Session expired. Please log in again.';

/**
 * Listens for session expiration signals and routes the user back to login
 * with a persistent toast and preserved deep link.
 */
export function useSessionExpiryHandler(): void {
  const { logout } = useAuth();
  const navigate = useNavigate();
  const location = useLocation();
  const { enqueue } = useToastQueue();
  const handlingRef = useRef(false);

  useEffect(() => {
    if (typeof window === 'undefined') {
      return;
    }

    const handleExpiry = () => {
      if (handlingRef.current) {
        return;
      }
      handlingRef.current = true;

      // Ensure flag is present for login screen messaging
      markSessionExpired();

      enqueue({ title: SESSION_EXPIRED_TOAST, variant: 'warning', persist: true });
      logger.warn('Session expired; redirecting to login', {
        component: 'useSessionExpiryHandler',
        path: location.pathname,
      });

      try {
        sessionStorage.setItem(
          POST_LOGIN_REDIRECT_KEY,
          `${location.pathname}${location.search || ''}`,
        );
      } catch {
        // ignore storage errors; best-effort redirect hint
      }

      logout()
        .catch((err) => {
          logger.error(
            'Logout after session expiry failed',
            { component: 'useSessionExpiryHandler' },
            toError(err),
          );
        })
        .finally(() => {
          navigate('/login', { replace: true });
        });
    };

    const checkFlag = () => {
      try {
        if (sessionStorage.getItem(SESSION_EXPIRED_FLAG_KEY) === '1') {
          handleExpiry();
          return;
        }
      } catch {
        // ignore sessionStorage errors; fall back to memory flag check
      }

      if (consumeSessionExpiredFlag()) {
        handleExpiry();
      }
    };

    const onExpired = () => handleExpiry();
    const onVisibility = () => {
      if (document.visibilityState === 'visible') {
        checkFlag();
      }
    };

    checkFlag();

    window.addEventListener(SESSION_EXPIRED_EVENT, onExpired);
    document.addEventListener('visibilitychange', onVisibility);

    return () => {
      window.removeEventListener(SESSION_EXPIRED_EVENT, onExpired);
      document.removeEventListener('visibilitychange', onVisibility);
      handlingRef.current = false;
    };
  }, [enqueue, location.pathname, location.search, logout, navigate]);
}

