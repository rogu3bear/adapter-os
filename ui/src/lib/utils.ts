import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

/**
 * Stable sort implementation for deterministic ordering
 */
export function stableSort<T>(
  items: T[],
  keys: (keyof T)[]
): T[] {
  return [...items].sort((a, b) => {
    for (const key of keys) {
      const aVal = a[key];
      const bVal = b[key];
      if (aVal < bVal) return -1;
      if (aVal > bVal) return 1;
    }
    return 0;
  });
}

/**
 * Generate canonical key for React list items
 */
export function canonicalKey(obj: { hash?: string; id?: string; hash_b3?: string } | Record<string, unknown>): string {
  const o = obj as { hash?: string; id?: string; hash_b3?: string };
  return o.hash || o.id || o.hash_b3 || JSON.stringify(obj);
}


// Shared styling constants for UI overlay components

/**
 * Animation classes for menu overlays
 * Used for consistent enter/exit animations across dropdowns, menus, popovers, etc.
 */
export const MENU_ANIMATION_CLASSES =
  "data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2";

/**
 * Frost glass styling for popover overlays
 */
export const FROST_POPOVER = "bg-popover/95 backdrop-blur-md text-popover-foreground";

/**
 * Frost glass styling for background overlays (dialogs, sheets)
 */
export const FROST_BACKGROUND = "bg-background/95 backdrop-blur-md";

/**
 * Frost glass styling for overlay backdrops
 */
export const FROST_OVERLAY = "bg-black/50 backdrop-blur-sm";

/**
 * Frost glass styling for tooltips
 */
export const FROST_TOOLTIP = "bg-popover/90 backdrop-blur-md border border-border/40 text-popover-foreground shadow-lg";

/**
 * Base classes for menu items (dropdown, context menu, menubar)
 */
export const MENU_ITEM_BASE =
  "focus:bg-accent focus:text-accent-foreground data-[variant=destructive]:text-destructive data-[variant=destructive]:focus:bg-destructive/10 dark:data-[variant=destructive]:focus:bg-destructive/20 data-[variant=destructive]:focus:text-destructive data-[variant=destructive]:*:[svg]:!text-destructive [&_svg:not([class*='text-'])]:text-muted-foreground relative flex cursor-default items-center gap-2 rounded-sm px-2 py-1.5 text-sm outline-hidden select-none data-[disabled]:pointer-events-none data-[disabled]:opacity-50 data-[inset]:pl-8 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4";

/**
 * Base classes for checkbox and radio menu items
 */
export const MENU_CHECKBOX_RADIO_BASE =
  "focus:bg-accent focus:text-accent-foreground relative flex cursor-default items-center gap-2 rounded-sm py-1.5 pr-2 pl-8 text-sm outline-hidden select-none data-[disabled]:pointer-events-none data-[disabled]:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4";

/**
 * Variant of MENU_CHECKBOX_RADIO_BASE with rounded-xs instead of rounded-sm
 * Used by Menubar components
 */
export const MENU_CHECKBOX_RADIO_BASE_XS =
  "focus:bg-accent focus:text-accent-foreground relative flex cursor-default items-center gap-2 rounded-xs py-1.5 pr-2 pl-8 text-sm outline-hidden select-none data-[disabled]:pointer-events-none data-[disabled]:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4";

/**
 * Separator classes for menu components
 */
export const MENU_SEPARATOR = "bg-border -mx-1 my-1 h-px";

/**
 * Base classes for close buttons in dialogs/sheets
 */
export const CLOSE_BUTTON_BASE =
  "ring-offset-background focus:ring-ring absolute top-4 right-4 rounded-xs opacity-70 transition-opacity hover:opacity-100 focus:ring-2 focus:ring-offset-2 focus:outline-hidden disabled:pointer-events-none";
