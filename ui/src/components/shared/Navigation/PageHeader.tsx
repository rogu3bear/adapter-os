import React from 'react';
import { LucideIcon, ArrowLeft } from 'lucide-react';
import { useNavigate } from 'react-router-dom';

import { cn } from '../../ui/utils';
import { Breadcrumbs, BreadcrumbItemConfig } from './Breadcrumbs';
import { ActionBar, ActionConfig } from './ActionBar';
import { Button } from '../../ui/button';

export interface PageHeaderProps {
  /** Page title */
  title: string;
  /** Optional page description */
  description?: string;
  /** Optional icon to display before title */
  icon?: LucideIcon;
  /** Breadcrumb items for navigation trail */
  breadcrumbs?: BreadcrumbItemConfig[];
  /** Show breadcrumbs (default: true if items provided) */
  showBreadcrumbs?: boolean;
  /** Primary action buttons */
  actions?: ActionConfig[];
  /** Secondary/dropdown actions */
  secondaryActions?: ActionConfig[];
  /** Custom content for the right side (replaces actions) */
  rightContent?: React.ReactNode;
  /** Show back button */
  showBackButton?: boolean;
  /** Custom back navigation path */
  backPath?: string;
  /** Back button label */
  backLabel?: string;
  /** Additional CSS classes */
  className?: string;
  /** Children rendered below title/description */
  children?: React.ReactNode;
  /** Sticky header behavior */
  sticky?: boolean;
}

/**
 * PageHeader - Unified page header component
 *
 * Combines breadcrumbs, title, description, and action buttons
 * into a consistent page header layout.
 *
 * @example
 * ```tsx
 * <PageHeader
 *   title="Adapter Details"
 *   description="View and manage adapter configuration"
 *   icon={Package}
 *   breadcrumbs={[
 *     { id: 'adapters', label: 'Adapters', href: '/adapters' },
 *     { id: 'detail', label: 'my-adapter' }
 *   ]}
 *   actions={[
 *     { id: 'save', label: 'Save', onClick: handleSave, variant: 'default' },
 *     { id: 'delete', label: 'Delete', onClick: handleDelete, variant: 'destructive' }
 *   ]}
 *   showBackButton
 * />
 * ```
 */
export function PageHeader({
  title,
  description,
  icon: Icon,
  breadcrumbs,
  showBreadcrumbs = true,
  actions,
  secondaryActions,
  rightContent,
  showBackButton = false,
  backPath,
  backLabel,
  className,
  children,
  sticky = false,
}: PageHeaderProps) {
  const navigate = useNavigate();
  const hasBreadcrumbs = showBreadcrumbs && breadcrumbs && breadcrumbs.length > 0;
  const hasActions = (actions && actions.length > 0) || (secondaryActions && secondaryActions.length > 0);

  const handleBack = () => {
    if (backPath) {
      navigate(backPath);
    } else {
      navigate(-1);
    }
  };

  return (
    <header
      className={cn(
        "space-y-4",
        sticky && "sticky top-0 z-10 bg-background/95 backdrop-blur-sm pb-4 border-b",
        className
      )}
    >
      {/* Breadcrumbs row */}
      {hasBreadcrumbs && (
        <Breadcrumbs items={breadcrumbs} className="mb-2" />
      )}

      {/* Main header row */}
      <div className="flex items-start justify-between gap-4">
        {/* Left side: back button, icon, title, description */}
        <div className="flex items-start gap-3 min-w-0 flex-1">
          {showBackButton && (
            <Button
              variant="ghost"
              size="sm"
              onClick={handleBack}
              className="mt-1"
            >
              <ArrowLeft className="h-4 w-4 mr-1" />
              {backLabel ?? 'Back'}
            </Button>
          )}

          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2">
              {Icon && (
                <Icon className="h-6 w-6 text-muted-foreground shrink-0" />
              )}
              <h1 className="text-2xl font-semibold tracking-tight truncate">
                {title}
              </h1>
            </div>

            {description && (
              <p className="mt-1 text-sm text-muted-foreground">
                {description}
              </p>
            )}
          </div>
        </div>

        {/* Right side: actions or custom content */}
        <div className="flex items-center gap-2 shrink-0">
          {rightContent ?? (
            hasActions && (
              <ActionBar
                actions={actions}
                secondaryActions={secondaryActions}
              />
            )
          )}
        </div>
      </div>

      {/* Children content */}
      {children && (
        <div className="pt-2">
          {children}
        </div>
      )}
    </header>
  );
}

/**
 * PageHeaderSkeleton - Loading skeleton for PageHeader
 */
export function PageHeaderSkeleton({
  showBreadcrumbs = true,
  showDescription = true,
  showActions = true,
  className,
}: {
  showBreadcrumbs?: boolean;
  showDescription?: boolean;
  showActions?: boolean;
  className?: string;
}) {
  return (
    <header className={cn("space-y-4 animate-pulse", className)}>
      {showBreadcrumbs && (
        <div className="flex items-center gap-2 mb-2">
          <div className="h-4 w-4 bg-muted rounded" />
          <div className="h-4 w-2 bg-muted rounded" />
          <div className="h-4 w-20 bg-muted rounded" />
          <div className="h-4 w-2 bg-muted rounded" />
          <div className="h-4 w-24 bg-muted rounded" />
        </div>
      )}

      <div className="flex items-start justify-between gap-4">
        <div className="space-y-2">
          <div className="h-8 w-48 bg-muted rounded" />
          {showDescription && (
            <div className="h-4 w-72 bg-muted rounded" />
          )}
        </div>

        {showActions && (
          <div className="flex items-center gap-2">
            <div className="h-9 w-20 bg-muted rounded" />
            <div className="h-9 w-24 bg-muted rounded" />
          </div>
        )}
      </div>
    </header>
  );
}
