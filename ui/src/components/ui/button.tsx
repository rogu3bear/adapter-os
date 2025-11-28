import * as React from "react";
import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";

import { cn } from "./utils";

const buttonVariants = cva(
  "inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-md text-sm font-medium transition-all disabled:pointer-events-none disabled:opacity-50 [&_svg]:pointer-events-none [&_svg:not([class*='size-'])]:size-4 shrink-0 [&_svg]:shrink-0 outline-hidden focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px] aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 aria-invalid:border-destructive",
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
        xs: "h-7 text-xs px-2 py-1 gap-1 has-[>svg]:px-1.5 [&_svg:not([class*='size-'])]:size-3",
        sm: "h-8 text-sm px-3 py-1.5 gap-1.5 has-[>svg]:px-2.5 [&_svg:not([class*='size-'])]:size-3.5 min-h-[44px] sm:min-h-0",
        default: "h-10 text-sm px-4 py-2 gap-2 has-[>svg]:px-3 [&_svg:not([class*='size-'])]:size-4 min-h-[44px] sm:min-h-0",
        md: "h-10 text-sm px-4 py-2 gap-2 has-[>svg]:px-3 [&_svg:not([class*='size-'])]:size-4 min-h-[44px] sm:min-h-0",
        lg: "h-12 text-base px-6 py-3 gap-2.5 has-[>svg]:px-4 [&_svg:not([class*='size-'])]:size-5 min-h-[44px] sm:min-h-0",
        xl: "h-14 text-lg px-8 py-4 gap-3 has-[>svg]:px-6 [&_svg:not([class*='size-'])]:size-6",
        icon: "size-10 rounded-md min-h-[44px] sm:min-h-0",
        "icon-xs": "size-7 rounded-md [&_svg:not([class*='size-'])]:size-3",
        "icon-sm": "size-8 rounded-md min-h-[44px] sm:min-h-0 [&_svg:not([class*='size-'])]:size-3.5",
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
      className={cn(
        "inline-flex items-center justify-center whitespace-nowrap rounded-[var(--radius-button)] text-sm font-medium ring-offset-background transition-colors focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50",
        "h-[calc(var(--button-padding-y)_*__2_+_var(--font-body))]", // ~36px
        "px-[var(--button-padding-x)] py-[var(--button-padding-y)]",
        {
          'bg-[var(--accent-500)] text-primary-foreground hover:bg-[var(--accent-600)]': variant === "default",
          'bg-[var(--success)] text-primary-foreground hover:bg-[color-mix(in_srgb,var(--success)_20%,black)]': variant === "success",
          // ... other variants with tokens
        },
        className
      )}
      {...props}
    />
  );
});

Button.displayName = "Button";

export { Button, buttonVariants };
