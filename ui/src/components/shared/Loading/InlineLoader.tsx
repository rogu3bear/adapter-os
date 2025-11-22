import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "../../ui/utils";
import { LoadingSpinner } from "./LoadingSpinner";

const inlineLoaderVariants = cva(
  "inline-flex items-center gap-2",
  {
    variants: {
      variant: {
        /** Default inline loader */
        default: "",
        /** Button-style loader */
        button: "justify-center",
        /** Text replacement loader */
        text: "text-muted-foreground",
      },
      size: {
        xs: "text-xs",
        sm: "text-sm",
        md: "text-base",
        lg: "text-lg",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "sm",
    },
  }
);

const spinnerSizeMap = {
  xs: "xs" as const,
  sm: "xs" as const,
  md: "sm" as const,
  lg: "md" as const,
};

export interface InlineLoaderProps
  extends React.HTMLAttributes<HTMLSpanElement>,
    VariantProps<typeof inlineLoaderVariants> {
  /** Loading text to display */
  text?: string;
  /** Position of spinner relative to text */
  spinnerPosition?: "left" | "right";
  /** Hide the text, show only spinner */
  spinnerOnly?: boolean;
}

/**
 * Inline loading indicator for use within buttons, text, or other inline contexts.
 * Compact and unobtrusive for small loading states.
 */
export function InlineLoader({
  className,
  variant,
  size,
  text = "Loading",
  spinnerPosition = "left",
  spinnerOnly = false,
  ...props
}: InlineLoaderProps) {
  const spinner = (
    <LoadingSpinner
      size={spinnerSizeMap[size || "sm"]}
      className="text-current"
    />
  );

  if (spinnerOnly) {
    return (
      <span
        className={cn(inlineLoaderVariants({ variant, size }), className)}
        role="status"
        aria-label={text}
        {...props}
      >
        {spinner}
      </span>
    );
  }

  return (
    <span
      className={cn(inlineLoaderVariants({ variant, size }), className)}
      role="status"
      {...props}
    >
      {spinnerPosition === "left" && spinner}
      <span>{text}</span>
      {spinnerPosition === "right" && spinner}
    </span>
  );
}

/**
 * Button content replacement for loading state.
 * Maintains button dimensions while showing loading indicator.
 */
export interface ButtonLoaderProps
  extends Omit<InlineLoaderProps, "variant" | "spinnerOnly"> {
  /** Original button content to measure/preserve dimensions */
  children?: React.ReactNode;
  /** Whether loading is active */
  loading?: boolean;
}

export function ButtonLoader({
  children,
  loading = false,
  text = "Loading",
  size = "sm",
  className,
  ...props
}: ButtonLoaderProps) {
  if (!loading) {
    return <>{children}</>;
  }

  return (
    <span className={cn("inline-flex items-center gap-2", className)} {...props}>
      <LoadingSpinner size={spinnerSizeMap[size]} className="text-current" />
      {text && <span>{text}</span>}
    </span>
  );
}

/**
 * Dots loading indicator (three bouncing dots).
 * Alternative to spinner for certain contexts.
 */
export interface DotsLoaderProps extends React.HTMLAttributes<HTMLSpanElement> {
  /** Dot size */
  size?: "sm" | "md" | "lg";
  /** Screen reader label */
  label?: string;
}

const dotSizeClasses = {
  sm: "h-1 w-1",
  md: "h-1.5 w-1.5",
  lg: "h-2 w-2",
};

const dotGapClasses = {
  sm: "gap-0.5",
  md: "gap-1",
  lg: "gap-1.5",
};

export function DotsLoader({
  size = "md",
  label = "Loading",
  className,
  ...props
}: DotsLoaderProps) {
  return (
    <span
      className={cn("inline-flex items-center", dotGapClasses[size], className)}
      role="status"
      aria-label={label}
      {...props}
    >
      {[0, 1, 2].map((index) => (
        <span
          key={index}
          className={cn(
            "rounded-full bg-current animate-bounce",
            dotSizeClasses[size]
          )}
          style={{
            animationDelay: `${index * 150}ms`,
            animationDuration: "600ms",
          }}
          aria-hidden="true"
        />
      ))}
    </span>
  );
}

export { inlineLoaderVariants };
