"use client";

import * as React from "react";
import { ChevronDown } from "lucide-react";

import { cn } from "../../ui/utils";
import type { FormSectionProps } from "./types";

/**
 * FormSection - Groups related form fields with an optional title and description.
 *
 * Supports collapsible sections for complex forms.
 *
 * @example
 * ```tsx
 * <FormSection
 *   title="Personal Information"
 *   description="Please provide your contact details."
 * >
 *   <FormField form={form} name="firstName" label="First Name" />
 *   <FormField form={form} name="lastName" label="Last Name" />
 * </FormSection>
 * ```
 *
 * @example Collapsible section
 * ```tsx
 * <FormSection
 *   title="Advanced Settings"
 *   description="Optional configuration options."
 *   collapsible
 *   defaultCollapsed
 * >
 *   <FormField form={form} name="timeout" label="Timeout (ms)" type="number" />
 * </FormSection>
 * ```
 */
export function FormSection({
  title,
  description,
  children,
  className,
  divider = false,
  collapsible = false,
  defaultCollapsed = false,
}: FormSectionProps) {
  const [isCollapsed, setIsCollapsed] = React.useState(defaultCollapsed);
  const contentId = React.useId();

  const handleToggle = () => {
    if (collapsible) {
      setIsCollapsed(!isCollapsed);
    }
  };

  const handleKeyDown = (event: React.KeyboardEvent) => {
    if (collapsible && (event.key === "Enter" || event.key === " ")) {
      event.preventDefault();
      handleToggle();
    }
  };

  const hasHeader = title || description;

  return (
    <div
      className={cn(
        "space-y-4",
        divider && "pb-6 border-b border-border",
        className,
      )}
    >
      {hasHeader && (
        <div
          className={cn(
            "space-y-1",
            collapsible && "cursor-pointer select-none",
          )}
          onClick={collapsible ? handleToggle : undefined}
          onKeyDown={collapsible ? handleKeyDown : undefined}
          role={collapsible ? "button" : undefined}
          tabIndex={collapsible ? 0 : undefined}
          aria-expanded={collapsible ? !isCollapsed : undefined}
          aria-controls={collapsible ? contentId : undefined}
        >
          <div className="flex items-center justify-between">
            {title && (
              <h3 className="text-lg font-medium leading-6 text-foreground">
                {title}
              </h3>
            )}
            {collapsible && (
              <ChevronDown
                className={cn(
                  "h-5 w-5 text-muted-foreground transition-transform duration-200",
                  isCollapsed && "-rotate-90",
                )}
                aria-hidden="true"
              />
            )}
          </div>
          {description && (
            <p className="text-sm text-muted-foreground">
              {description}
            </p>
          )}
        </div>
      )}

      {collapsible ? (
        <div
          id={contentId}
          className={cn(
            "grid gap-4 transition-all duration-200",
            isCollapsed && "hidden",
          )}
          aria-hidden={isCollapsed}
        >
          {children}
        </div>
      ) : (
        <div className="grid gap-4">
          {children}
        </div>
      )}
    </div>
  );
}

export default FormSection;
