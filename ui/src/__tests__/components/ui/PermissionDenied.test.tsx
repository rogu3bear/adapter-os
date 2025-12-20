import { describe, it, expect, vi, beforeEach } from 'vitest';
import React from 'react';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import { PermissionDenied } from '@/components/ui/permission-denied';
import * as useRBACModule from '@/hooks/security/useRBAC';

// Mock useRBAC hook
vi.mock('@/hooks/security/useRBAC', () => ({
  useRBAC: vi.fn(),
}));

// Mock rbac utils
vi.mock('@/utils/rbac', () => ({
  getRoleName: (role: string) => {
    const names: Record<string, string> = {
      admin: 'Administrator',
      operator: 'Operator',
      viewer: 'Viewer',
      auditor: 'Auditor',
    };
    return names[role] || role;
  },
  getPermissionDescription: (permission: string) => permission,
}));

// Mock navigate
const mockNavigate = vi.fn();
vi.mock('react-router-dom', async () => {
  const actual = await vi.importActual('react-router-dom');
  return {
    ...actual,
    useNavigate: () => mockNavigate,
  };
});

const mockUseRBAC = useRBACModule.useRBAC as ReturnType<typeof vi.fn>;

describe('PermissionDenied', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('when user is authenticated', () => {
    beforeEach(() => {
      mockUseRBAC.mockReturnValue({
        userRole: 'viewer',
        isAuthenticated: () => true,
        can: vi.fn(),
        hasRole: vi.fn(),
      });
    });

    it('should render access denied title', () => {
      render(
        <MemoryRouter>
          <PermissionDenied />
        </MemoryRouter>
      );

      expect(screen.getByText('Access Denied')).toBeTruthy();
    });

    it('should show user role in message when no permission specified', () => {
      render(
        <MemoryRouter>
          <PermissionDenied />
        </MemoryRouter>
      );

      expect(screen.getByText(/Your role \(Viewer\) does not have access to this resource/)).toBeTruthy();
    });

    it('should show required permission when specified', () => {
      render(
        <MemoryRouter>
          <PermissionDenied requiredPermission="audit:view" />
        </MemoryRouter>
      );

      // Multiple elements contain audit:view (in message and code block)
      expect(screen.getAllByText(/audit:view/).length).toBeGreaterThan(0);
    });

    it('should show required roles when specified', () => {
      render(
        <MemoryRouter>
          <PermissionDenied
            requiredPermission="audit:view"
            requiredRoles={['admin', 'operator']}
          />
        </MemoryRouter>
      );

      expect(screen.getByText(/Required roles:/)).toBeTruthy();
      expect(screen.getByText(/Administrator, Operator/)).toBeTruthy();
    });

    it('should show current role', () => {
      render(
        <MemoryRouter>
          <PermissionDenied requiredPermission="audit:view" />
        </MemoryRouter>
      );

      expect(screen.getByText(/Your current role:/)).toBeTruthy();
      // Viewer appears in multiple places (in message and current role display)
      expect(screen.getAllByText(/Viewer/).length).toBeGreaterThan(0);
    });

    it('should render Go Back button by default', () => {
      render(
        <MemoryRouter>
          <PermissionDenied />
        </MemoryRouter>
      );

      expect(screen.getByRole('button', { name: /Go Back/i })).toBeTruthy();
    });

    it('should navigate back when Go Back button is clicked', async () => {
      const user = userEvent.setup();

      render(
        <MemoryRouter>
          <PermissionDenied />
        </MemoryRouter>
      );

      await user.click(screen.getByRole('button', { name: /Go Back/i }));

      expect(mockNavigate).toHaveBeenCalledWith(-1);
    });

    it('should hide Go Back button when showBackButton is false', () => {
      render(
        <MemoryRouter>
          <PermissionDenied showBackButton={false} />
        </MemoryRouter>
      );

      expect(screen.queryByRole('button', { name: /Go Back/i })).toBeNull();
    });

    it('should render custom message when provided', () => {
      render(
        <MemoryRouter>
          <PermissionDenied message="Custom access denied message" />
        </MemoryRouter>
      );

      expect(screen.getByText('Custom access denied message')).toBeTruthy();
    });

    it('should render custom action button when provided', () => {
      render(
        <MemoryRouter>
          <PermissionDenied
            actionButton={<button data-testid="custom-action">Custom Action</button>}
          />
        </MemoryRouter>
      );

      expect(screen.getByTestId('custom-action')).toBeTruthy();
    });
  });

  describe('when user is not authenticated', () => {
    beforeEach(() => {
      mockUseRBAC.mockReturnValue({
        userRole: null,
        isAuthenticated: () => false,
        can: vi.fn(),
        hasRole: vi.fn(),
      });
    });

    it('should show login required message', () => {
      render(
        <MemoryRouter>
          <PermissionDenied />
        </MemoryRouter>
      );

      expect(screen.getByText('You must be logged in to access this page.')).toBeTruthy();
    });
  });

  describe('different roles', () => {
    it('should display admin role correctly', () => {
      mockUseRBAC.mockReturnValue({
        userRole: 'admin',
        isAuthenticated: () => true,
        can: vi.fn(),
        hasRole: vi.fn(),
      });

      render(
        <MemoryRouter>
          <PermissionDenied requiredPermission="special:permission" />
        </MemoryRouter>
      );

      // Administrator appears in multiple places
      expect(screen.getAllByText(/Administrator/).length).toBeGreaterThan(0);
    });

    it('should display operator role correctly', () => {
      mockUseRBAC.mockReturnValue({
        userRole: 'operator',
        isAuthenticated: () => true,
        can: vi.fn(),
        hasRole: vi.fn(),
      });

      render(
        <MemoryRouter>
          <PermissionDenied requiredPermission="special:permission" />
        </MemoryRouter>
      );

      // Operator appears in multiple places
      expect(screen.getAllByText(/Operator/).length).toBeGreaterThan(0);
    });
  });
});
