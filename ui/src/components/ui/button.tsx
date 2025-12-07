import * as React from "react";
import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";

import { cn } from "./utils";

const buttonVariants = cva(
  "inline-flex items-center justify-center gap-[var(--space-2)] whitespace-nowrap rounded-md text-sm font-medium transition-all disabled:pointer-events-none disabled:opacity-50 [&_svg]:pointer-events-none [&_svg:not([class*='size-'])]:size-4 shrink-0 [&_svg]:shrink-0 outline-hidden focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[calc(var(--base-unit)*0.75)] aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 aria-invalid:border-destructive",
  {
    variants: {
      variant: {
        default: "bg-primary text-primary-foreground hover:bg-primary/90",
        destructive:
          "bg-destructive text-white hover:bg-destructive/90 focus-visible:ring-destructive/20 dark:focus-visible:ring-destructive/40 dark:bg-destructive/60",
        outline:
          "border bg-background text-foreground hover:bg-accent hover:text-accent-foreground dark:bg-input/30 dark:border-input dark:hover:bg-input/50",
        secondary:
          "bg-secondary text-secondary-foreground hover:bg-secondary/80",
        ghost:
          "hover:bg-accent hover:text-accent-foreground dark:hover:bg-accent/50",
        link: "text-primary underline-offset-4 hover:underline",
        success:
          "bg-success text-white hover:bg-[color-mix(in_srgb,var(--success)_90%,black)] focus-visible:ring-success/40",
      },
      size: {
        xs: "h-[calc(var(--base-unit)*7)] text-xs px-[var(--space-2)] py-[var(--space-1)] gap-[var(--space-1)] has-[>svg]:px-[calc(var(--base-unit)*1.5)] [&_svg:not([class*='size-'])]:size-3",
        sm: "h-[var(--space-8)] text-sm px-[var(--space-3)] py-[calc(var(--base-unit)*1.5)] gap-[calc(var(--base-unit)*1.5)] has-[>svg]:px-[calc(var(--base-unit)*2.5)] [&_svg:not([class*='size-'])]:size-3.5 min-h-[calc(var(--base-unit)*11)] sm:min-h-0",
        default: "h-[calc(var(--base-unit)*10)] text-sm px-[var(--space-4)] py-[var(--space-2)] gap-[var(--space-2)] has-[>svg]:px-[var(--space-3)] [&_svg:not([class*='size-'])]:size-4 min-h-[calc(var(--base-unit)*11)] sm:min-h-0",
        md: "h-[calc(var(--base-unit)*10)] text-sm px-[var(--space-4)] py-[var(--space-2)] gap-[var(--space-2)] has-[>svg]:px-[var(--space-3)] [&_svg:not([class*='size-'])]:size-4 min-h-[calc(var(--base-unit)*11)] sm:min-h-0",
        lg: "h-[calc(var(--base-unit)*12)] text-base px-[var(--space-6)] py-[var(--space-3)] gap-[calc(var(--base-unit)*2.5)] has-[>svg]:px-[var(--space-4)] [&_svg:not([class*='size-'])]:size-5 min-h-[calc(var(--base-unit)*11)] sm:min-h-0",
        xl: "h-[calc(var(--base-unit)*14)] text-lg px-[var(--space-8)] py-[var(--space-4)] gap-[var(--space-3)] has-[>svg]:px-[var(--space-6)] [&_svg:not([class*='size-'])]:size-6",
        icon: "size-10 rounded-md min-h-[calc(var(--base-unit)*11)] sm:min-h-0",
        "icon-xs": "size-7 rounded-md [&_svg:not([class*='size-'])]:size-3",
        "icon-sm": "size-8 rounded-md min-h-[calc(var(--base-unit)*11)] sm:min-h-0 [&_svg:not([class*='size-'])]:size-3.5",
        "icon-lg": "size-12 rounded-md [&_svg:not([class*='size-'])]:size-5",
        "icon-xl": "size-14 rounded-md [&_svg:not([class*='size-'])]:size-6",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  },
);

const Button = React.forwardRef<
  HTMLButtonElement,
  React.ComponentProps<"button"> &
    VariantProps<typeof buttonVariants> & {
      asChild?: boolean;
    }
>(({ className, variant, size, asChild = false, ...props }, ref) => {
  const Comp = asChild ? Slot : "button";

  return (
    <Comp
      ref={ref}
      data-slot="button"
      className={cn(buttonVariants({ variant, size, className }))}
      {...props}
    />
  );
});

Button.displayName = "Button";

export { Button, buttonVariants };
