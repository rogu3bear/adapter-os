import { cn } from "@/lib/utils";
import { useReducedMotion } from "@/hooks/ui/useReducedMotion";

function Skeleton({
  className,
  "aria-label": ariaLabel,
  ...props
}: React.ComponentProps<"div"> & { "aria-label"?: string }) {
  const prefersReducedMotion = useReducedMotion();

  return (
    <div
      data-slot="skeleton"
      className={cn("bg-accent rounded-md", !prefersReducedMotion && "animate-pulse", className)}
      role="status"
      aria-label={ariaLabel || "Loading"}
      aria-live="polite"
      {...props}
    />
  );
}

export { Skeleton };
