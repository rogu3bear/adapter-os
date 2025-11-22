import React from 'react';
import { useNavigate, useLocation } from 'react-router-dom';
import { ArrowLeft, ChevronLeft } from 'lucide-react';

import { cn } from '../../ui/utils';
import { Button } from '../../ui/button';

export interface BackButtonProps {
  /** Custom navigation path (overrides browser history) */
  path?: string;
  /** Button label (default: "Back") */
  label?: string;
  /** Show label (default: false, icon only) */
  showLabel?: boolean;
  /** Icon variant */
  iconVariant?: 'arrow' | 'chevron';
  /** Button variant */
  variant?: 'default' | 'outline' | 'ghost' | 'link';
  /** Button size */
  size?: 'default' | 'sm' | 'lg' | 'icon';
  /** Fallback path if no history */
  fallbackPath?: string;
  /** Additional CSS classes */
  className?: string;
  /** Callback before navigation */
  onBeforeNavigate?: () => boolean | void;
}

/**
 * BackButton - Navigation back button component
 *
 * Provides consistent back navigation with support for custom paths,
 * browser history, and fallback behavior.
 *
 * @example
 * ```tsx
 * // Simple back button (uses browser history)
 * <BackButton />
 *
 * // With custom path
 * <BackButton path="/adapters" label="Back to Adapters" showLabel />
 *
 * // With confirmation
 * <BackButton
 *   onBeforeNavigate={() => {
 *     if (hasUnsavedChanges) {
 *       return confirm('Discard changes?');
 *     }
 *     return true;
 *   }}
 * />
 * ```
 */
export function BackButton({
  path,
  label = 'Back',
  showLabel = false,
  iconVariant = 'arrow',
  variant = 'ghost',
  size = 'sm',
  fallbackPath = '/',
  className,
  onBeforeNavigate,
}: BackButtonProps) {
  const navigate = useNavigate();
  const location = useLocation();

  const Icon = iconVariant === 'chevron' ? ChevronLeft : ArrowLeft;

  const handleClick = () => {
    // Call onBeforeNavigate if provided
    if (onBeforeNavigate) {
      const shouldNavigate = onBeforeNavigate();
      if (shouldNavigate === false) {
        return;
      }
    }

    // Navigate to specific path if provided
    if (path) {
      navigate(path);
      return;
    }

    // Try to go back in history
    // Check if there's history to go back to
    if (window.history.length > 1) {
      navigate(-1);
    } else {
      // Fallback to default path
      navigate(fallbackPath);
    }
  };

  // Determine if icon-only button
  const isIconOnly = !showLabel;
  const buttonSize = isIconOnly ? 'icon' : size;

  return (
    <Button
      variant={variant}
      size={buttonSize}
      onClick={handleClick}
      className={cn(
        isIconOnly && "h-8 w-8",
        className
      )}
      aria-label={label}
    >
      <Icon className={cn("h-4 w-4", showLabel && "mr-1")} />
      {showLabel && <span>{label}</span>}
    </Button>
  );
}

/**
 * BackLink - Link-style back navigation
 *
 * Renders as a text link rather than a button.
 */
export function BackLink({
  path,
  label = 'Back',
  iconVariant = 'chevron',
  fallbackPath = '/',
  className,
  onBeforeNavigate,
}: Omit<BackButtonProps, 'showLabel' | 'variant' | 'size'>) {
  const navigate = useNavigate();
  const Icon = iconVariant === 'chevron' ? ChevronLeft : ArrowLeft;

  const handleClick = (e: React.MouseEvent) => {
    e.preventDefault();

    if (onBeforeNavigate) {
      const shouldNavigate = onBeforeNavigate();
      if (shouldNavigate === false) {
        return;
      }
    }

    if (path) {
      navigate(path);
      return;
    }

    if (window.history.length > 1) {
      navigate(-1);
    } else {
      navigate(fallbackPath);
    }
  };

  return (
    <a
      href={path ?? '#'}
      onClick={handleClick}
      className={cn(
        "inline-flex items-center gap-1 text-sm text-muted-foreground",
        "hover:text-foreground transition-colors",
        className
      )}
    >
      <Icon className="h-4 w-4" />
      <span>{label}</span>
    </a>
  );
}

/**
 * useBackNavigation - Hook for programmatic back navigation
 *
 * @param options - Configuration options
 * @returns Navigation functions
 *
 * @example
 * ```tsx
 * const { goBack, canGoBack } = useBackNavigation({
 *   fallbackPath: '/dashboard',
 * });
 *
 * // In event handler
 * if (canGoBack) {
 *   goBack();
 * }
 * ```
 */
export function useBackNavigation(options: {
  fallbackPath?: string;
  onBeforeNavigate?: () => boolean | void;
} = {}) {
  const { fallbackPath = '/', onBeforeNavigate } = options;
  const navigate = useNavigate();
  const location = useLocation();

  const canGoBack = window.history.length > 1;

  const goBack = React.useCallback((customPath?: string) => {
    if (onBeforeNavigate) {
      const shouldNavigate = onBeforeNavigate();
      if (shouldNavigate === false) {
        return;
      }
    }

    if (customPath) {
      navigate(customPath);
      return;
    }

    if (canGoBack) {
      navigate(-1);
    } else {
      navigate(fallbackPath);
    }
  }, [navigate, canGoBack, fallbackPath, onBeforeNavigate]);

  const goTo = React.useCallback((path: string) => {
    if (onBeforeNavigate) {
      const shouldNavigate = onBeforeNavigate();
      if (shouldNavigate === false) {
        return;
      }
    }
    navigate(path);
  }, [navigate, onBeforeNavigate]);

  return {
    goBack,
    goTo,
    canGoBack,
    currentPath: location.pathname,
  };
}
