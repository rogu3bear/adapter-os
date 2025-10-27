import React from 'react';
import { useAuth } from '@/layout/LayoutProvider';
import type { UserRole } from '@/api/types';

interface RoleGuardProps {
  allowedRoles: UserRole[];
  children: React.ReactNode;
  fallback?: React.ReactNode;
}

/**
 * RoleGuard - Progressive disclosure component for role-based visibility
 * 
 * Hides or shows content based on user's role.
 * Used throughout the UI to implement role-based access control.
 * 
 * @example
 * <RoleGuard allowedRoles={['Admin', 'Operator']}>
 *   <SensitiveContent />
 * </RoleGuard>
 */
export function RoleGuard({ allowedRoles, children, fallback = null }: RoleGuardProps) {
  const { user } = useAuth();
  
  if (!user || !allowedRoles.includes(user.role)) {
    return <>{fallback}</>;
  }
  
  return <>{children}</>;
}

