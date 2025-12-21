/**
 * Shared variant icon and color configurations
 *
 * Consolidates duplicate icon/color mappings from Alert.tsx and Toast.tsx
 * into a single source of truth for feedback component styling.
 */

import { type LucideIcon, Info, CheckCircle, AlertCircle, AlertTriangle } from 'lucide-react';

export type FeedbackVariant = 'default' | 'success' | 'error' | 'warning' | 'info';

/**
 * Icon mapping for feedback variants
 */
export const VARIANT_ICONS: Record<FeedbackVariant, LucideIcon> = {
  default: Info,
  success: CheckCircle,
  error: AlertCircle,
  warning: AlertTriangle,
  info: Info,
};

/**
 * Icon color classes for feedback variants (supports dark mode)
 */
export const VARIANT_ICON_COLORS: Record<FeedbackVariant, string> = {
  default: 'text-muted-foreground',
  success: 'text-green-600 dark:text-green-400',
  error: 'text-destructive',
  warning: 'text-yellow-600 dark:text-yellow-400',
  info: 'text-blue-600 dark:text-blue-400',
};

/**
 * Helper to get icon component for a variant
 */
export function getVariantIcon(variant: FeedbackVariant): LucideIcon {
  return VARIANT_ICONS[variant] ?? VARIANT_ICONS.default;
}

/**
 * Helper to get icon color class for a variant
 */
export function getVariantIconColor(variant: FeedbackVariant): string {
  return VARIANT_ICON_COLORS[variant] ?? VARIANT_ICON_COLORS.default;
}
