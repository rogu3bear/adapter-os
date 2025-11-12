"use client";

import * as React from "react";
import { CheckIcon, CircleIcon } from 'lucide-react';

/**
 * Shared checkbox indicator wrapper for menu items
 * 【2025-11-07†refactor(ui)†extract-menu-indicators】
 * 
 * Replaces duplicate checkbox indicator patterns across:
 * - DropdownMenuCheckboxItem
 * - MenubarCheckboxItem  
 * - ContextMenuCheckboxItem
 * 
 * Usage:
 * <ItemIndicator>
 *   <CheckboxIndicator />
 * </ItemIndicator>
 */
export function CheckboxIndicator() {
  return <CheckIcon className="size-4" />;
}

/**
 * Shared radio indicator wrapper for menu items
 * 【2025-11-07†refactor(ui)†extract-menu-indicators】
 * 
 * Replaces duplicate radio indicator patterns across:
 * - DropdownMenuRadioItem
 * - MenubarRadioItem
 * - ContextMenuRadioItem
 * 
 * Usage:
 * <ItemIndicator>
 *   <RadioIndicator />
 * </ItemIndicator>
 */
export function RadioIndicator() {
  return <CircleIcon className="size-2 fill-current" />;
}

/**
 * Shared wrapper span for menu item indicators
 * 【2025-11-07†refactor(ui)†extract-menu-indicators】
 */
export function IndicatorWrapper({ children }: { children: React.ReactNode }) {
  return (
    <span className="pointer-events-none absolute left-2 flex size-3.5 items-center justify-center">
      {children}
    </span>
  );
}

