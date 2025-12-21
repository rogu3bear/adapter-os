import { cn } from "@/lib/utils";

function Skeleton({
  className,
  "aria-label": ariaLabel,
  ...props
}: React.ComponentProps<"div"> & { "aria-label"?: string }) {
  return (
    <div
      data-slot="skeleton"
      className={cn("bg-accent animate-pulse rounded-md", className)}
      role="status"
      aria-label={ariaLabel || "Loading"}
      aria-live="polite"
      {...props}
    />
  );
}

export { Skeleton };
