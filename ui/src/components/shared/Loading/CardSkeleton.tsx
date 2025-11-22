import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "../../ui/utils";
import { Skeleton, SkeletonText } from "./Skeleton";

const cardSkeletonVariants = cva(
  "bg-card text-card-foreground rounded-xl border animate-pulse",
  {
    variants: {
      variant: {
        /** Standard card with header and content */
        default: "p-6",
        /** Compact card for dense layouts */
        compact: "p-4",
        /** Card with image/media header */
        media: "",
        /** Stats card layout */
        stats: "p-4",
        /** Profile card layout */
        profile: "p-6",
      },
    },
    defaultVariants: {
      variant: "default",
    },
  }
);

export interface CardSkeletonProps
  extends React.HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof cardSkeletonVariants> {
  /** Show title skeleton */
  showTitle?: boolean;
  /** Show description skeleton */
  showDescription?: boolean;
  /** Show action buttons skeleton */
  showActions?: boolean;
  /** Show avatar/icon skeleton */
  showAvatar?: boolean;
  /** Show image/media skeleton */
  showMedia?: boolean;
  /** Number of content lines */
  contentLines?: number;
}

/**
 * Skeleton placeholder for card components.
 * Supports various card layouts and configurations.
 */
export function CardSkeleton({
  className,
  variant = "default",
  showTitle = true,
  showDescription = true,
  showActions = false,
  showAvatar = false,
  showMedia = false,
  contentLines = 0,
  ...props
}: CardSkeletonProps) {
  if (variant === "media") {
    return (
      <div
        className={cn(cardSkeletonVariants({ variant }), className)}
        role="status"
        aria-label="Loading card"
        {...props}
      >
        {/* Media header */}
        <Skeleton className="w-full h-40 rounded-t-xl rounded-b-none" />

        {/* Content */}
        <div className="p-4 space-y-3">
          {showTitle && <Skeleton variant="heading" className="w-3/4" />}
          {showDescription && <SkeletonText lines={2} gap="sm" lastLineWidth="80%" />}
          {showActions && (
            <div className="flex gap-2 pt-2">
              <Skeleton variant="button" className="w-20" />
              <Skeleton variant="button" className="w-20" />
            </div>
          )}
        </div>
      </div>
    );
  }

  if (variant === "stats") {
    return (
      <div
        className={cn(cardSkeletonVariants({ variant }), className)}
        role="status"
        aria-label="Loading stats card"
        {...props}
      >
        <div className="flex items-center justify-between mb-2">
          <Skeleton className="h-4 w-24" />
          {showAvatar && <Skeleton variant="avatar" size="sm" />}
        </div>
        <Skeleton className="h-8 w-20 mb-1" />
        <Skeleton className="h-3 w-16" />
      </div>
    );
  }

  if (variant === "profile") {
    return (
      <div
        className={cn(cardSkeletonVariants({ variant }), className)}
        role="status"
        aria-label="Loading profile card"
        {...props}
      >
        <div className="flex items-center gap-4 mb-4">
          <Skeleton variant="avatar" size="lg" />
          <div className="flex-1 space-y-2">
            <Skeleton variant="heading" className="w-32" />
            <Skeleton variant="text" className="w-48" size="sm" />
          </div>
        </div>
        {contentLines > 0 && <SkeletonText lines={contentLines} gap="sm" />}
        {showActions && (
          <div className="flex gap-2 mt-4">
            <Skeleton variant="button" className="flex-1" />
            <Skeleton variant="button" className="flex-1" />
          </div>
        )}
      </div>
    );
  }

  // Default and compact variants
  return (
    <div
      className={cn(cardSkeletonVariants({ variant }), className)}
      role="status"
      aria-label="Loading card"
      {...props}
    >
      {/* Header */}
      <div className={cn("flex items-start gap-3", variant === "compact" ? "mb-3" : "mb-4")}>
        {showAvatar && <Skeleton variant="avatar" size={variant === "compact" ? "sm" : "md"} />}
        <div className="flex-1 space-y-2">
          {showTitle && <Skeleton variant="heading" />}
          {showDescription && <Skeleton variant="text" className="w-2/3" size="sm" />}
        </div>
      </div>

      {/* Content */}
      {contentLines > 0 && (
        <div className={cn(variant === "compact" ? "mb-3" : "mb-4")}>
          <SkeletonText lines={contentLines} gap="sm" />
        </div>
      )}

      {/* Actions */}
      {showActions && (
        <div className="flex gap-2">
          <Skeleton variant="button" />
          <Skeleton variant="button" className="w-20" />
        </div>
      )}
    </div>
  );
}

/**
 * Grid of card skeletons for list layouts.
 */
export interface CardSkeletonGridProps extends React.HTMLAttributes<HTMLDivElement> {
  /** Number of cards to render */
  count?: number;
  /** Grid columns configuration */
  columns?: 1 | 2 | 3 | 4;
  /** Card variant to use */
  cardVariant?: CardSkeletonProps["variant"];
  /** Props to pass to each card skeleton */
  cardProps?: Omit<CardSkeletonProps, "variant">;
}

const columnClasses = {
  1: "grid-cols-1",
  2: "grid-cols-1 sm:grid-cols-2",
  3: "grid-cols-1 sm:grid-cols-2 lg:grid-cols-3",
  4: "grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4",
};

export function CardSkeletonGrid({
  count = 6,
  columns = 3,
  cardVariant = "default",
  cardProps,
  className,
  ...props
}: CardSkeletonGridProps) {
  return (
    <div
      className={cn("grid gap-4", columnClasses[columns], className)}
      {...props}
    >
      {Array.from({ length: count }).map((_, index) => (
        <CardSkeleton key={index} variant={cardVariant} {...cardProps} />
      ))}
    </div>
  );
}

export { cardSkeletonVariants };
