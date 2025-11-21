import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter, Routes, Route } from 'react-router-dom';
import { RouteGuard } from '../components/route-guard';
import { routes } from '../config/routes';
import * as CoreProviders from '../providers/CoreProviders';

// Mock useAuth hook
vi.mock('../providers/CoreProviders', () => ({
  useAuth: vi.fn(),
}));

// Mock PageSkeleton
vi.mock('../components/ui/page-skeleton', () => ({
  PageSkeleton: ({ variant }: { variant: string }) => (
    <div data-testid="page-skeleton" data-variant={variant}>Loading...</div>
  ),
}));

const mockUseAuth = CoreProviders.useAuth as ReturnType<typeof vi.fn>;

const TestComponent = () => <div>Protected Content</div>;

describe('RouteGuard', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('Authentication', () => {
    it('should show loading state while auth is being verified', () => {
      mockUseAuth.mockReturnValue({
        user: null,
        isLoading: true,
      });

      const testRoute = {
        path: '/test',
        component: TestComponent,
        requiresAuth: true,
      };

      render(
        <MemoryRouter initialEntries={['/test']}>
          <Routes>
            <Route path="/test" element={<RouteGuard route={testRoute} />} />
          </Routes>
        </MemoryRouter>
      );

      // Should show loading spinner
      expect(screen.getByRole('status') || screen.getByTestId('page-skeleton')).toBeTruthy();
    });

    it('should redirect to login when not authenticated', () => {
      mockUseAuth.mockReturnValue({
        user: null,
        isLoading: false,
      });

      const testRoute = {
        path: '/test',
        component: TestComponent,
        requiresAuth: true,
      };

      render(
        <MemoryRouter initialEntries={['/test']}>
          <Routes>
            <Route path="/test" element={<RouteGuard route={testRoute} />} />
            <Route path="/login" element={<div>Login Page</div>} />
          </Routes>
        </MemoryRouter>
      );

      expect(screen.getByText('Login Page')).toBeTruthy();
    });

    it('should render protected content when authenticated', async () => {
      mockUseAuth.mockReturnValue({
        user: {
          user_id: 'test-user',
          email: 'test@example.com',
          display_name: 'Test User',
          role: 'admin',
        },
        isLoading: false,
      });

      const testRoute = {
        path: '/test',
        component: TestComponent,
        requiresAuth: true,
      };

      render(
        <MemoryRouter initialEntries={['/test']}>
          <Routes>
            <Route path="/test" element={<RouteGuard route={testRoute} />} />
          </Routes>
        </MemoryRouter>
      );

      await waitFor(() => {
        expect(screen.getByText('Protected Content')).toBeTruthy();
      });
    });

    it('should allow access to public routes without authentication', async () => {
      mockUseAuth.mockReturnValue({
        user: null,
        isLoading: false,
      });

      const publicRoute = {
        path: '/public',
        component: TestComponent,
        requiresAuth: false,
      };

      render(
        <MemoryRouter initialEntries={['/public']}>
          <Routes>
            <Route path="/public" element={<RouteGuard route={publicRoute} />} />
          </Routes>
        </MemoryRouter>
      );

      await waitFor(() => {
        expect(screen.getByText('Protected Content')).toBeTruthy();
      });
    });
  });

  describe('Role-Based Access Control', () => {
    it('should allow admin to access admin-only routes', async () => {
      mockUseAuth.mockReturnValue({
        user: {
          user_id: 'admin-user',
          email: 'admin@example.com',
          display_name: 'Admin User',
          role: 'admin',
        },
        isLoading: false,
      });

      const adminRoute = {
        path: '/admin',
        component: TestComponent,
        requiresAuth: true,
        requiredRoles: ['admin'] as const,
      };

      render(
        <MemoryRouter initialEntries={['/admin']}>
          <Routes>
            <Route path="/admin" element={<RouteGuard route={adminRoute} />} />
          </Routes>
        </MemoryRouter>
      );

      await waitFor(() => {
        expect(screen.getByText('Protected Content')).toBeTruthy();
      });
    });

    it('should redirect non-admin from admin routes', () => {
      mockUseAuth.mockReturnValue({
        user: {
          user_id: 'viewer-user',
          email: 'viewer@example.com',
          display_name: 'Viewer User',
          role: 'viewer',
        },
        isLoading: false,
      });

      const adminRoute = {
        path: '/admin',
        component: TestComponent,
        requiresAuth: true,
        requiredRoles: ['admin'] as const,
      };

      render(
        <MemoryRouter initialEntries={['/admin']}>
          <Routes>
            <Route path="/admin" element={<RouteGuard route={adminRoute} />} />
            <Route path="/dashboard" element={<div>Dashboard</div>} />
          </Routes>
        </MemoryRouter>
      );

      expect(screen.getByText('Dashboard')).toBeTruthy();
    });

    it('should allow operator to access operator routes', async () => {
      mockUseAuth.mockReturnValue({
        user: {
          user_id: 'operator-user',
          email: 'operator@example.com',
          display_name: 'Operator User',
          role: 'operator',
        },
        isLoading: false,
      });

      const operatorRoute = {
        path: '/operations',
        component: TestComponent,
        requiresAuth: true,
        requiredRoles: ['operator', 'admin'] as const,
      };

      render(
        <MemoryRouter initialEntries={['/operations']}>
          <Routes>
            <Route path="/operations" element={<RouteGuard route={operatorRoute} />} />
          </Routes>
        </MemoryRouter>
      );

      await waitFor(() => {
        expect(screen.getByText('Protected Content')).toBeTruthy();
      });
    });

    it('should allow SRE role to access routes without role restrictions', async () => {
      mockUseAuth.mockReturnValue({
        user: {
          user_id: 'sre-user',
          email: 'sre@example.com',
          display_name: 'SRE User',
          role: 'sre',
        },
        isLoading: false,
      });

      const unrestrrictedRoute = {
        path: '/metrics',
        component: TestComponent,
        requiresAuth: true,
      };

      render(
        <MemoryRouter initialEntries={['/metrics']}>
          <Routes>
            <Route path="/metrics" element={<RouteGuard route={unrestrrictedRoute} />} />
          </Routes>
        </MemoryRouter>
      );

      await waitFor(() => {
        expect(screen.getByText('Protected Content')).toBeTruthy();
      });
    });

    it('should allow Compliance role to access routes without role restrictions', async () => {
      mockUseAuth.mockReturnValue({
        user: {
          user_id: 'compliance-user',
          email: 'compliance@example.com',
          display_name: 'Compliance User',
          role: 'compliance',
        },
        isLoading: false,
      });

      const unrestrrictedRoute = {
        path: '/audit',
        component: TestComponent,
        requiresAuth: true,
      };

      render(
        <MemoryRouter initialEntries={['/audit']}>
          <Routes>
            <Route path="/audit" element={<RouteGuard route={unrestrrictedRoute} />} />
          </Routes>
        </MemoryRouter>
      );

      await waitFor(() => {
        expect(screen.getByText('Protected Content')).toBeTruthy();
      });
    });

    it('should prevent SRE role from accessing admin-only routes', () => {
      mockUseAuth.mockReturnValue({
        user: {
          user_id: 'sre-user',
          email: 'sre@example.com',
          display_name: 'SRE User',
          role: 'sre',
        },
        isLoading: false,
      });

      const adminRoute = {
        path: '/admin',
        component: TestComponent,
        requiresAuth: true,
        requiredRoles: ['admin'] as const,
      };

      render(
        <MemoryRouter initialEntries={['/admin']}>
          <Routes>
            <Route path="/admin" element={<RouteGuard route={adminRoute} />} />
            <Route path="/dashboard" element={<div>Dashboard</div>} />
          </Routes>
        </MemoryRouter>
      );

      expect(screen.getByText('Dashboard')).toBeTruthy();
    });

    it('should prevent Compliance role from accessing admin-only routes', () => {
      mockUseAuth.mockReturnValue({
        user: {
          user_id: 'compliance-user',
          email: 'compliance@example.com',
          display_name: 'Compliance User',
          role: 'compliance',
        },
        isLoading: false,
      });

      const adminRoute = {
        path: '/admin',
        component: TestComponent,
        requiresAuth: true,
        requiredRoles: ['admin'] as const,
      };

      render(
        <MemoryRouter initialEntries={['/admin']}>
          <Routes>
            <Route path="/admin" element={<RouteGuard route={adminRoute} />} />
            <Route path="/dashboard" element={<div>Dashboard</div>} />
          </Routes>
        </MemoryRouter>
      );

      expect(screen.getByText('Dashboard')).toBeTruthy();
    });
  });

  describe('Route Configuration', () => {
    it('should have proper skeleton variants configured for dashboard routes', () => {
      const dashboardRoute = routes.find(r => r.path === '/dashboard');
      expect(dashboardRoute?.skeletonVariant).toBe('dashboard');
    });

    it('should have proper skeleton variants configured for table routes', () => {
      const adaptersRoute = routes.find(r => r.path === '/adapters');
      expect(adaptersRoute?.skeletonVariant).toBe('table');
    });

    it('should have proper skeleton variants configured for form routes', () => {
      const trainerRoute = routes.find(r => r.path === '/trainer');
      expect(trainerRoute?.skeletonVariant).toBe('form');
    });

    it('should have admin routes requiring admin role', () => {
      const adminRoute = routes.find(r => r.path === '/admin');
      expect(adminRoute?.requiredRoles).toContain('admin');
    });

    it('should have tenants route requiring admin role', () => {
      const tenantsRoute = routes.find(r => r.path === '/tenants');
      expect(tenantsRoute?.requiredRoles).toContain('admin');
    });
  });

  describe('Lazy Loading', () => {
    it('should show skeleton while lazy component loads', async () => {
      mockUseAuth.mockReturnValue({
        user: {
          user_id: 'test-user',
          email: 'test@example.com',
          display_name: 'Test User',
          role: 'admin',
        },
        isLoading: false,
      });

      // Get a route with lazy component
      const dashboardRoute = routes.find(r => r.path === '/dashboard');
      if (dashboardRoute) {
        render(
          <MemoryRouter initialEntries={['/dashboard']}>
            <Routes>
              <Route path="/dashboard" element={<RouteGuard route={dashboardRoute} />} />
            </Routes>
          </MemoryRouter>
        );

        // Should show skeleton while loading
        expect(screen.getByTestId('page-skeleton')).toBeTruthy();
        expect(screen.getByTestId('page-skeleton').getAttribute('data-variant')).toBe('dashboard');
      }
    });
  });
});

describe('canAccessRoute helper', () => {
  it('should return true for routes without role requirements', async () => {
    const { canAccessRoute } = await import('../config/routes');
    const publicRoute = {
      path: '/public',
      component: TestComponent,
      requiresAuth: false,
    };

    expect(canAccessRoute(publicRoute, undefined)).toBe(true);
    expect(canAccessRoute(publicRoute, 'viewer')).toBe(true);
  });

  it('should return true when user has required role', async () => {
    const { canAccessRoute } = await import('../config/routes');
    const adminRoute = {
      path: '/admin',
      component: TestComponent,
      requiresAuth: true,
      requiredRoles: ['admin'] as const,
    };

    expect(canAccessRoute(adminRoute, 'admin')).toBe(true);
  });

  it('should return false when user lacks required role', async () => {
    const { canAccessRoute } = await import('../config/routes');
    const adminRoute = {
      path: '/admin',
      component: TestComponent,
      requiresAuth: true,
      requiredRoles: ['admin'] as const,
    };

    expect(canAccessRoute(adminRoute, 'viewer')).toBe(false);
    expect(canAccessRoute(adminRoute, 'operator')).toBe(false);
  });
});
