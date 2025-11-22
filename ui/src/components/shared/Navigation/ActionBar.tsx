import React from 'react';
import { LucideIcon, MoreHorizontal, ChevronDown } from 'lucide-react';

import { cn } from '../../ui/utils';
import { Button } from '../../ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '../../ui/dropdown-menu';

export interface ActionConfig {
  /** Unique identifier */
  id: string;
  /** Display label */
  label: string;
  /** Optional icon */
  icon?: LucideIcon;
  /** Click handler */
  onClick?: () => void;
  /** Navigate to path on click */
  href?: string;
  /** Button variant */
  variant?: 'default' | 'destructive' | 'outline' | 'secondary' | 'ghost' | 'link';
  /** Button size */
  size?: 'default' | 'sm' | 'lg' | 'icon';
  /** Disabled state */
  disabled?: boolean;
  /** Loading state */
  loading?: boolean;
  /** Show in dropdown only (for secondary actions) */
  dropdownOnly?: boolean;
  /** Keyboard shortcut hint */
  shortcut?: string;
  /** Separator before this item (in dropdown) */
  separatorBefore?: boolean;
}

export interface ActionBarProps {
  /** Primary action buttons (displayed inline) */
  actions?: ActionConfig[];
  /** Secondary actions (displayed in dropdown) */
  secondaryActions?: ActionConfig[];
  /** Show secondary actions in overflow menu */
  showOverflow?: boolean;
  /** Maximum inline actions before overflow */
  maxInlineActions?: number;
  /** Dropdown menu label */
  overflowLabel?: string;
  /** Alignment of actions */
  align?: 'start' | 'center' | 'end';
  /** Gap between action buttons */
  gap?: 'sm' | 'md' | 'lg';
  /** Additional CSS classes */
  className?: string;
}

/**
 * ActionBar - Page-level action buttons component
 *
 * Renders primary actions as inline buttons and secondary actions
 * in an overflow dropdown menu.
 *
 * @example
 * ```tsx
 * <ActionBar
 *   actions={[
 *     { id: 'save', label: 'Save', onClick: handleSave, variant: 'default', icon: Save },
 *     { id: 'preview', label: 'Preview', onClick: handlePreview, variant: 'outline' },
 *   ]}
 *   secondaryActions={[
 *     { id: 'export', label: 'Export', onClick: handleExport, icon: Download },
 *     { id: 'delete', label: 'Delete', onClick: handleDelete, variant: 'destructive', separatorBefore: true },
 *   ]}
 * />
 * ```
 */
export function ActionBar({
  actions = [],
  secondaryActions = [],
  showOverflow = true,
  maxInlineActions = 3,
  overflowLabel = 'More actions',
  align = 'end',
  gap = 'md',
  className,
}: ActionBarProps) {
  // Split actions into inline and overflow
  const inlineActions = actions.filter(a => !a.dropdownOnly).slice(0, maxInlineActions);
  const overflowActions = [
    ...actions.filter(a => a.dropdownOnly),
    ...actions.filter(a => !a.dropdownOnly).slice(maxInlineActions),
    ...secondaryActions,
  ];

  const hasOverflow = showOverflow && overflowActions.length > 0;

  const gapClass = {
    sm: 'gap-1',
    md: 'gap-2',
    lg: 'gap-3',
  }[gap];

  const alignClass = {
    start: 'justify-start',
    center: 'justify-center',
    end: 'justify-end',
  }[align];

  const renderAction = (action: ActionConfig) => {
    const Icon = action.icon;

    return (
      <Button
        key={action.id}
        variant={action.variant ?? 'outline'}
        size={action.size ?? 'default'}
        onClick={action.onClick}
        disabled={action.disabled || action.loading}
        className={cn(action.loading && "cursor-wait")}
      >
        {action.loading ? (
          <span className="animate-spin h-4 w-4 border-2 border-current border-t-transparent rounded-full" />
        ) : Icon ? (
          <Icon className="h-4 w-4" />
        ) : null}
        <span>{action.label}</span>
      </Button>
    );
  };

  const renderDropdownItem = (action: ActionConfig, index: number) => {
    const Icon = action.icon;
    const showSeparator = action.separatorBefore && index > 0;

    return (
      <React.Fragment key={action.id}>
        {showSeparator && <DropdownMenuSeparator />}
        <DropdownMenuItem
          onClick={action.onClick}
          disabled={action.disabled || action.loading}
          className={cn(
            action.variant === 'destructive' && "text-destructive focus:text-destructive"
          )}
        >
          {action.loading ? (
            <span className="animate-spin h-4 w-4 border-2 border-current border-t-transparent rounded-full mr-2" />
          ) : Icon ? (
            <Icon className="h-4 w-4 mr-2" />
          ) : null}
          <span className="flex-1">{action.label}</span>
          {action.shortcut && (
            <span className="ml-auto text-xs text-muted-foreground">
              {action.shortcut}
            </span>
          )}
        </DropdownMenuItem>
      </React.Fragment>
    );
  };

  if (inlineActions.length === 0 && !hasOverflow) {
    return null;
  }

  return (
    <div className={cn("flex items-center", gapClass, alignClass, className)}>
      {/* Inline actions */}
      {inlineActions.map(renderAction)}

      {/* Overflow dropdown */}
      {hasOverflow && (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="outline" size="icon" aria-label={overflowLabel}>
              <MoreHorizontal className="h-4 w-4" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-56">
            {overflowActions.map((action, index) => renderDropdownItem(action, index))}
          </DropdownMenuContent>
        </DropdownMenu>
      )}
    </div>
  );
}

/**
 * ActionGroup - Group of related actions with optional label
 */
export function ActionGroup({
  label,
  actions,
  variant = 'buttons',
  className,
}: {
  label?: string;
  actions: ActionConfig[];
  variant?: 'buttons' | 'dropdown';
  className?: string;
}) {
  if (variant === 'dropdown') {
    return (
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant="outline" className={className}>
            {label ?? 'Actions'}
            <ChevronDown className="h-4 w-4 ml-2" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-56">
          {actions.map((action, index) => {
            const Icon = action.icon;
            return (
              <React.Fragment key={action.id}>
                {action.separatorBefore && index > 0 && <DropdownMenuSeparator />}
                <DropdownMenuItem
                  onClick={action.onClick}
                  disabled={action.disabled}
                  className={cn(
                    action.variant === 'destructive' && "text-destructive focus:text-destructive"
                  )}
                >
                  {Icon && <Icon className="h-4 w-4 mr-2" />}
                  <span>{action.label}</span>
                </DropdownMenuItem>
              </React.Fragment>
            );
          })}
        </DropdownMenuContent>
      </DropdownMenu>
    );
  }

  return (
    <div className={cn("flex items-center gap-2", className)}>
      {label && (
        <span className="text-sm text-muted-foreground mr-1">{label}:</span>
      )}
      {actions.map((action) => {
        const Icon = action.icon;
        return (
          <Button
            key={action.id}
            variant={action.variant ?? 'outline'}
            size={action.size ?? 'sm'}
            onClick={action.onClick}
            disabled={action.disabled}
          >
            {Icon && <Icon className="h-4 w-4" />}
            <span>{action.label}</span>
          </Button>
        );
      })}
    </div>
  );
}
