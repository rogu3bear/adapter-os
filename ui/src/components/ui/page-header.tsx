import React from 'react';
import { Button } from './button';
import { Badge } from './badge';
import { GlossaryTooltip } from './glossary-tooltip';
import { cn } from './utils';
import { LucideIcon, ChevronRight } from 'lucide-react';
import { useDensity } from '@/contexts/DensityContext';

// 【2025-01-21†unification†page_header_component】

export interface PageHeaderAction {
  label: string;
  icon?: LucideIcon;
  onClick: () => void;
  variant?: 'default' | 'outline' | 'secondary' | 'destructive' | 'ghost';
  size?: 'default' | 'sm' | 'lg' | 'icon';
  loading?: boolean;
  disabled?: boolean;
}

export interface PageHeaderBadge {
  label: string;
  variant?: 'default' | 'secondary' | 'destructive' | 'outline' | 'success' | 'warning' | 'error' | 'info' | 'neutral';
}

export interface PageHeaderBreadcrumb {
  label: string;
  href?: string;
  onClick?: () => void;
}

export interface PageHeaderProps {
  title: string;
  description?: string;
  termId?: string;
  brief?: string;
  primaryAction?: PageHeaderAction;
  secondaryActions?: PageHeaderAction[];
  badges?: PageHeaderBadge[];
  breadcrumbs?: PageHeaderBreadcrumb[];
  className?: string;
  children?: React.ReactNode;
}

export function PageHeader({
  title,
  description,
  termId,
  brief,
  primaryAction,
  secondaryActions,
  badges,
  breadcrumbs,
  className,
  children
}: PageHeaderProps) {
  // Try to use density context, fall back to defaults if not available
  let spacing = {
    cardPadding: 'p-4',
    sectionGap: 'space-y-4',
    gridGap: 'gap-4',
    buttonGap: 'gap-2',
    formFieldGap: 'space-y-3',
    tableCellPadding: 'px-3 py-2',
    modalPadding: 'p-6'
  };
  let textSizes = { title: 'text-xl', subtitle: 'text-base', body: 'text-sm', caption: 'text-xs' };

  try {
    const densityContext = useDensity();
    spacing = densityContext.spacing;
    textSizes = densityContext.textSizes;
  } catch {
    // Not within DensityProvider, use defaults
  }

  const PrimaryIcon = primaryAction?.icon;

  return (
    <div className={cn("w-full", className)}>
      {/* Breadcrumbs */}
      {breadcrumbs && breadcrumbs.length > 0 && (
        <nav className="flex items-center text-sm text-muted-foreground mb-4">
          {breadcrumbs.map((crumb, index) => (
            <React.Fragment key={index}>
              {index > 0 && (
                <ChevronRight className="h-4 w-4 mx-2 flex-shrink-0" />
              )}
              {crumb.href || crumb.onClick ? (
                <button
                  onClick={crumb.onClick}
                  className="hover:text-foreground transition-colors"
                >
                  {crumb.label}
                </button>
              ) : (
                <span className={index === breadcrumbs.length - 1 ? 'text-foreground font-medium' : ''}>
                  {crumb.label}
                </span>
              )}
            </React.Fragment>
          ))}
        </nav>
      )}

      {/* Main header content */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        {/* Left side: Title + Description */}
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <h1 className={cn(textSizes.title, "font-bold tracking-tight truncate")}>
              {title}
            </h1>
            {(termId || brief) && (
              <GlossaryTooltip termId={termId} brief={brief} />
            )}
            {badges && badges.length > 0 && (
              <div className="hidden sm:flex items-center gap-2 ml-2">
                {badges.map((badge, index) => (
                  <Badge key={index} variant={badge.variant || 'secondary'}>
                    {badge.label}
                  </Badge>
                ))}
              </div>
            )}
          </div>
          {description && (
            <p className={cn(textSizes.caption, "text-muted-foreground mt-1")}>
              {description}
            </p>
          )}
          {/* Mobile badges */}
          {badges && badges.length > 0 && (
            <div className="flex sm:hidden items-center gap-2 mt-2">
              {badges.map((badge, index) => (
                <Badge key={index} variant={badge.variant || 'secondary'}>
                  {badge.label}
                </Badge>
              ))}
            </div>
          )}
        </div>

        {/* Right side: Actions */}
        <div className={cn("flex items-center flex-shrink-0", spacing.buttonGap)}>
          {/* Secondary actions */}
          {secondaryActions && secondaryActions.map((action, index) => {
            const SecondaryIcon = action.icon;
            return (
              <Button
                key={index}
                onClick={action.onClick}
                variant={action.variant || 'outline'}
                size={action.size || 'default'}
                disabled={action.disabled || action.loading}
              >
                {SecondaryIcon && (
                  <SecondaryIcon className={cn(
                    "mr-2 h-4 w-4",
                    action.loading && "animate-spin"
                  )} />
                )}
                {action.label}
              </Button>
            );
          })}

          {/* Primary action */}
          {primaryAction && (
            <Button
              onClick={primaryAction.onClick}
              variant={primaryAction.variant || 'default'}
              size={primaryAction.size || 'default'}
              disabled={primaryAction.disabled || primaryAction.loading}
            >
              {PrimaryIcon && (
                <PrimaryIcon className={cn(
                  "mr-2 h-4 w-4",
                  primaryAction.loading && "animate-spin"
                )} />
              )}
              {primaryAction.label}
            </Button>
          )}

          {/* Additional children (custom actions) */}
          {children}
        </div>
      </div>
    </div>
  );
}

export default PageHeader;
