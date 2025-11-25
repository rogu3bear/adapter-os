import { useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import { useAuth } from '@/providers/CoreProviders';
import { logger } from '@/utils/logger';

const FIRST_RUN_KEY = 'aos-first-login-completed';

/**
 * Hook to handle first-run redirect for admin users.
 * On first login, admin users are redirected to /owner page.
 * Subsequent logins redirect to /dashboard as normal.
 */
export function useFirstRunRedirect() {
  const navigate = useNavigate();
  const { user } = useAuth();

  useEffect(() => {
    // Only redirect if user is authenticated and is an admin
    if (user?.role === 'admin') {
      try {
        const hasCompletedFirstRun = localStorage.getItem(FIRST_RUN_KEY);

        if (!hasCompletedFirstRun) {
          // Mark first run as completed before navigating
          localStorage.setItem(FIRST_RUN_KEY, 'true');

          logger.info('First-run redirect for admin user', {
            component: 'useFirstRunRedirect',
            user_id: user.id,
          });

          navigate('/owner', { replace: true });
        }
      } catch (error) {
        // If localStorage fails, log warning but don't break the flow
        logger.warn('Failed to check/set first-run flag', {
          component: 'useFirstRunRedirect',
        });
      }
    }
  }, [user, navigate]);
}
