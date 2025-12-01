import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/components/ui/utils";

const skeletonVariants = cva(
  "bg-muted animate-pulse rounded-md",
  {
    variants: {
      variant: {
        /** Default skeleton shape */
        default: "",
        /** Text line skeleton */
        text: "h-4 w-full",
        /** Heading skeleton */
        heading: "h-6 w-3/4",
        /** Title skeleton (larger) */
        title: "h-8 w-1/2",
        /** Avatar/circular skeleton */
        avatar: "rounded-full aspect-square",
        /** Button skeleton */
        button: "h-9 w-24",
        /** Image placeholder skeleton */
        image: "aspect-video w-full",
        /** Badge skeleton */
        badge: "h-5 w-16 rounded-full",
      },
      size: {
        sm: "",
        md: "",
        lg: "",
      },
    },
    compoundVariants: [
      { variant: "avatar", size: "sm", className: "h-8 w-8" },
      { variant: "avatar", size: "md", className: "h-10 w-10" },
      { variant: "avatar", size: "lg", className: "h-12 w-12" },
      { variant: "text", size: "sm", className: "h-3" },
      { variant: "text", size: "md", className: "h-4" },
      { variant: "text", size: "lg", className: "h-5" },
    ],
    defaultVariants: {
      variant: "default",
      size: "md",
    },
  }
);

export interface SkeletonProps
  extends React.HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof skeletonVariants> {
  /** Width override (CSS value) */
  width?: string | number;
  /** Height override (CSS value) */
  height?: string | number;
}

/**
 * Skeleton placeholder component for loading states.
 * Provides various preset variants for common UI elements.
 */
export function Skeleton({
  className,
  variant,
  size,
  width,
  height,
  style,
  ...props
}: SkeletonProps) {
  return (
    <div
      data-slot="skeleton"
      className={cn(skeletonVariants({ variant, size }), className)}
      style={{
        width: typeof width === "number" ? `${width}px` : width,
        height: typeof height === "number" ? `${height}px` : height,
        ...style,
      }}
      aria-hidden="true"
      {...props}
    />
  );
}

/**
 * Skeleton text component that renders multiple lines.
 */
export interface SkeletonTextProps extends Omit<SkeletonProps, "variant"> {
  /** Number of lines to render */
  lines?: number;
  /** Line spacing */
  gap?: "sm" | "md" | "lg";
  /** Last line width (percentage or CSS value) */
  lastLineWidth?: string;
}

const gapClasses = {
  sm: "space-y-1.5",
  md: "space-y-2",
  lg: "space-y-3",
};

export function SkeletonText({
  lines = 3,
  gap = "md",
  lastLineWidth = "60%",
  className,
  ...props
}: SkeletonTextProps) {
  return (
    <div className={cn(gapClasses[gap], className)} aria-hidden="true">
      {Array.from({ length: lines }).map((_, index) => (
        <Skeleton
          key={index}
          variant="text"
          style={{
            width: index === lines - 1 ? lastLineWidth : undefined,
          }}
          {...props}
        />
      ))}
    </div>
  );
}

export { skeletonVariants };
