import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "../../ui/utils";

const spinnerVariants = cva(
  "animate-spin text-current",
  {
    variants: {
      size: {
        xs: "h-3 w-3",
        sm: "h-4 w-4",
        md: "h-6 w-6",
        lg: "h-8 w-8",
        xl: "h-12 w-12",
      },
    },
    defaultVariants: {
      size: "md",
    },
  }
);

export interface LoadingSpinnerProps
  extends React.SVGAttributes<SVGSVGElement>,
    VariantProps<typeof spinnerVariants> {
  /** Screen reader label */
  label?: string;
}

/**
 * Animated loading spinner component with multiple size variants.
 * Uses Tailwind's animate-spin for smooth rotation.
 */
export function LoadingSpinner({
  className,
  size,
  label = "Loading",
  ...props
}: LoadingSpinnerProps) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      fill="none"
      viewBox="0 0 24 24"
      className={cn(spinnerVariants({ size }), className)}
      aria-label={label}
      role="status"
      {...props}
    >
      <circle
        className="opacity-25"
        cx="12"
        cy="12"
        r="10"
        stroke="currentColor"
        strokeWidth="4"
      />
      <path
        className="opacity-75"
        fill="currentColor"
        d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
      />
    </svg>
  );
}

export { spinnerVariants };
