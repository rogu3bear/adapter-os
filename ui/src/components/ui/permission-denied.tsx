import React from 'react';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { useRBAC } from '@/hooks/security/useRBAC';
import { getRoleName, getPermissionDescription } from '@/utils/rbac';
import type { UserRole } from '@/api/types';
import { ShieldX, ArrowLeft } from 'lucide-react';
import { useNavigate } from 'react-router-dom';

interface PermissionDeniedProps {
  /** The permission that was required but missing */
  requiredPermission?: string;
  /** Roles that would grant access (optional, for display purposes) */
  requiredRoles?: UserRole[];
  /** Custom message override */
  message?: string;
  /** Show back button (default: true) */
  showBackButton?: boolean;
  /** Custom action button */
  actionButton?: React.ReactNode;
}

/**
 * Permission denied component that shows the user's actual role
 * and what permissions/roles are required for access.
 */
export function PermissionDenied({
  requiredPermission,
  requiredRoles,
  message,
  showBackButton = true,
  actionButton,
}: PermissionDeniedProps) {
  const { userRole, isAuthenticated } = useRBAC();
  const navigate = useNavigate();

  const roleName = userRole ? getRoleName(userRole) : 'Unknown';
  const permissionDesc = requiredPermission
    ? getPermissionDescription(requiredPermission)
    : undefined;

  const defaultMessage = !isAuthenticated()
    ? 'You must be logged in to access this page.'
    : requiredPermission
      ? `Your role (${roleName}) does not have the "${permissionDesc}" permission.`
      : `Your role (${roleName}) does not have access to this resource.`;

  const displayMessage = message || defaultMessage;

  return (
    <Alert variant="destructive" className="max-w-2xl mx-auto">
      <ShieldX className="h-5 w-5" />
      <AlertTitle className="ml-2">Access Denied</AlertTitle>
      <AlertDescription className="mt-3 space-y-4">
        <p>{displayMessage}</p>

        {requiredRoles && requiredRoles.length > 0 && (
          <div className="text-sm">
            <span className="font-medium">Required roles: </span>
            {requiredRoles.map(getRoleName).join(', ')}
          </div>
        )}

        {requiredPermission && (
          <div className="text-sm">
            <span className="font-medium">Required permission: </span>
            <code className="bg-muted px-1 py-0.5 rounded">{requiredPermission}</code>
          </div>
        )}

        {userRole && (
          <div className="text-sm text-muted-foreground">
            <span className="font-medium">Your current role: </span>
            {roleName}
          </div>
        )}

        <div className="flex gap-2 pt-2">
          {showBackButton && (
            <Button variant="outline" size="sm" onClick={() => navigate(-1)}>
              <ArrowLeft className="h-4 w-4 mr-2" />
              Go Back
            </Button>
          )}
          {actionButton}
        </div>
      </AlertDescription>
    </Alert>
  );
}

/**
 * Higher-order component for wrapping pages with permission checks.
 * Renders PermissionDenied if user lacks the required permission.
 */
export function withPermission<P extends object>(
  WrappedComponent: React.ComponentType<P>,
  requiredPermission: string,
  requiredRoles?: UserRole[]
) {
  return function PermissionGuardedComponent(props: P) {
    const { can } = useRBAC();

    if (!can(requiredPermission)) {
      return (
        <PermissionDenied
          requiredPermission={requiredPermission}
          requiredRoles={requiredRoles}
        />
      );
    }

    return <WrappedComponent {...props} />;
  };
}
