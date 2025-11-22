import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "../../ui/utils";
import { LoadingSpinner } from "./LoadingSpinner";

const pageLoaderVariants = cva(
  "flex flex-col items-center justify-center",
  {
    variants: {
      variant: {
        /** Full viewport loader */
        fullscreen: "fixed inset-0 z-50 bg-background",
        /** Full height of parent container */
        fill: "absolute inset-0 bg-background",
        /** Minimum height loader */
        minHeight: "min-h-[400px] bg-background",
        /** Inline centered loader */
        inline: "py-20",
      },
    },
    defaultVariants: {
      variant: "fullscreen",
    },
  }
);

export interface PageLoaderProps
  extends React.HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof pageLoaderVariants> {
  /** Application logo component or image URL */
  logo?: React.ReactNode | string;
  /** Logo alt text (for image URLs) */
  logoAlt?: string;
  /** Loading title */
  title?: string;
  /** Loading description/message */
  description?: string;
  /** Show progress bar */
  showProgress?: boolean;
  /** Progress percentage (0-100) */
  progress?: number;
  /** Hide spinner (logo only mode) */
  hideSpinner?: boolean;
}

/**
 * Full page loading state component with optional logo and progress.
 * Ideal for application initialization and page transitions.
 */
export function PageLoader({
  className,
  variant,
  logo,
  logoAlt = "Loading",
  title,
  description,
  showProgress = false,
  progress = 0,
  hideSpinner = false,
  ...props
}: PageLoaderProps) {
  const renderLogo = () => {
    if (!logo) return null;

    if (typeof logo === "string") {
      return (
        <img
          src={logo}
          alt={logoAlt}
          className="h-12 w-auto mb-6 animate-pulse"
        />
      );
    }

    return <div className="mb-6">{logo}</div>;
  };

  return (
    <div
      className={cn(pageLoaderVariants({ variant }), className)}
      role="status"
      aria-live="polite"
      aria-busy="true"
      {...props}
    >
      {/* Logo */}
      {renderLogo()}

      {/* Spinner */}
      {!hideSpinner && (
        <LoadingSpinner
          size="xl"
          className={cn("text-primary", logo && "mb-4")}
        />
      )}

      {/* Title */}
      {title && (
        <h2 className="mt-4 text-lg font-semibold text-foreground">
          {title}
        </h2>
      )}

      {/* Description */}
      {description && (
        <p className="mt-2 text-sm text-muted-foreground max-w-md text-center">
          {description}
        </p>
      )}

      {/* Progress bar */}
      {showProgress && (
        <div className="mt-6 w-64 max-w-full">
          <div className="h-1.5 w-full bg-muted rounded-full overflow-hidden">
            <div
              className="h-full bg-primary rounded-full transition-all duration-300 ease-out"
              style={{ width: `${Math.min(100, Math.max(0, progress))}%` }}
            />
          </div>
          <p className="mt-2 text-xs text-muted-foreground text-center">
            {Math.round(progress)}%
          </p>
        </div>
      )}
    </div>
  );
}

/**
 * Animated logo loader with pulsing effect.
 * Use for branded loading experiences.
 */
export interface LogoLoaderProps extends Omit<PageLoaderProps, "hideSpinner"> {
  /** Pulsing animation intensity */
  pulseIntensity?: "subtle" | "medium" | "strong";
}

const pulseClasses = {
  subtle: "animate-pulse",
  medium: "animate-[pulse_1.5s_ease-in-out_infinite]",
  strong: "animate-[pulse_1s_ease-in-out_infinite]",
};

export function LogoLoader({
  logo,
  pulseIntensity = "medium",
  className,
  ...props
}: LogoLoaderProps) {
  const enhancedLogo = logo ? (
    <div className={cn(pulseClasses[pulseIntensity])}>
      {typeof logo === "string" ? (
        <img src={logo} alt={props.logoAlt || "Loading"} className="h-16 w-auto" />
      ) : (
        logo
      )}
    </div>
  ) : null;

  return (
    <PageLoader
      logo={enhancedLogo}
      hideSpinner
      className={className}
      {...props}
    />
  );
}

export { pageLoaderVariants };
