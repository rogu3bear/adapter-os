import * as React from "react"
import { cn } from "@/components/ui/utils"

/**
 * Grid System Presets
 *
 * Standard breakpoints:
 * - mobile: < 640px (sm)
 * - tablet: 640px - 1023px (sm to lg)
 * - desktop: >= 1024px (lg+)
 *
 * Usage Guide:
 * - KpiGrid: Use for dashboards with 4+ metric cards (memory, CPU, latency, etc.)
 * - ContentGrid: Use for side-by-side content sections (logs + details, chart + table)
 * - FormGrid: Use for forms with label/input pairs that should align
 * - ActionGrid: Use for button groups or action cards (quick actions, shortcuts)
 */

interface GridProps extends React.HTMLAttributes<HTMLDivElement> {
  children: React.ReactNode
}

/**
 * KpiGrid - For KPI/metric cards
 * 4 columns desktop | 2 columns tablet | 1 column mobile
 */
export function KpiGrid({ className, children, ...props }: GridProps) {
  return (
    <div
      className={cn(
        "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-6",
        className
      )}
      {...props}
    >
      {children}
    </div>
  )
}

/**
 * ContentGrid - For content cards (2-column layout)
 * 2 columns desktop | 1 column mobile
 */
export function ContentGrid({ className, children, ...props }: GridProps) {
  return (
    <div
      className={cn(
        "grid grid-cols-1 lg:grid-cols-2 gap-6",
        className
      )}
      {...props}
    >
      {children}
    </div>
  )
}

/**
 * FormGrid - For form layouts
 * 2 columns for field pairs | Full width for single fields
 */
export function FormGrid({ className, children, ...props }: GridProps) {
  return (
    <div
      className={cn(
        "grid grid-cols-1 sm:grid-cols-2 gap-6",
        className
      )}
      {...props}
    >
      {children}
    </div>
  )
}

/**
 * FormField - Full-width form field wrapper
 * Use inside FormGrid for fields that should span full width
 */
export function FormFieldFull({ className, children, ...props }: GridProps) {
  return (
    <div
      className={cn("sm:col-span-2", className)}
      {...props}
    >
      {children}
    </div>
  )
}

/**
 * ActionGrid - For action buttons
 * 4 columns desktop | 2 columns tablet | 1 column mobile
 */
export function ActionGrid({ className, children, ...props }: GridProps) {
  return (
    <div
      className={cn(
        "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-6",
        className
      )}
      {...props}
    >
      {children}
    </div>
  )
}
