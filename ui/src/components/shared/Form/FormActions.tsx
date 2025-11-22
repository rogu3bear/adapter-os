"use client";

import * as React from "react";
import { Loader2 } from "lucide-react";

import { cn } from "../../ui/utils";
import { Button } from "../../ui/button";
import type { FormActionsProps } from "./types";

/**
 * FormActions - Button group for form submission, cancellation, and reset actions.
 *
 * Automatically handles loading states and disables buttons appropriately.
 *
 * @example
 * ```tsx
 * <FormActions
 *   isSubmitting={form.formState.isSubmitting}
 *   isDirty={form.formState.isDirty}
 *   isValid={form.formState.isValid}
 *   onCancel={() => router.back()}
 *   onReset={() => form.reset()}
 *   showCancel
 *   showReset
 * />
 * ```
 *
 * @example Minimal usage
 * ```tsx
 * <FormActions
 *   submitText="Save Changes"
 *   isSubmitting={isSubmitting}
 * />
 * ```
 */
export function FormActions({
  submitText = "Submit",
  cancelText = "Cancel",
  resetText = "Reset",
  isSubmitting = false,
  isDirty = true,
  isValid = true,
  onCancel,
  onReset,
  showCancel = false,
  showReset = false,
  className,
  align = "right",
  size = "default",
}: FormActionsProps) {
  const alignmentClasses = {
    left: "justify-start",
    center: "justify-center",
    right: "justify-end",
    between: "justify-between",
  };

  const handleCancel = (event: React.MouseEvent<HTMLButtonElement>) => {
    event.preventDefault();
    onCancel?.();
  };

  const handleReset = (event: React.MouseEvent<HTMLButtonElement>) => {
    event.preventDefault();
    onReset?.();
  };

  return (
    <div
      className={cn(
        "flex flex-wrap items-center gap-3 pt-4",
        alignmentClasses[align],
        className,
      )}
    >
      {/* Left-side buttons (Cancel, Reset) when align is "between" */}
      {align === "between" && (showCancel || showReset) && (
        <div className="flex items-center gap-3">
          {showCancel && onCancel && (
            <Button
              type="button"
              variant="outline"
              size={size}
              onClick={handleCancel}
              disabled={isSubmitting}
            >
              {cancelText}
            </Button>
          )}
          {showReset && onReset && (
            <Button
              type="button"
              variant="ghost"
              size={size}
              onClick={handleReset}
              disabled={isSubmitting || !isDirty}
            >
              {resetText}
            </Button>
          )}
        </div>
      )}

      {/* Standard layout for non-between alignment */}
      {align !== "between" && (
        <>
          {showReset && onReset && (
            <Button
              type="button"
              variant="ghost"
              size={size}
              onClick={handleReset}
              disabled={isSubmitting || !isDirty}
            >
              {resetText}
            </Button>
          )}
          {showCancel && onCancel && (
            <Button
              type="button"
              variant="outline"
              size={size}
              onClick={handleCancel}
              disabled={isSubmitting}
            >
              {cancelText}
            </Button>
          )}
        </>
      )}

      {/* Submit button */}
      <Button
        type="submit"
        size={size}
        disabled={isSubmitting || !isValid}
        className={cn(
          "min-w-[100px]",
          isSubmitting && "cursor-not-allowed",
        )}
      >
        {isSubmitting ? (
          <>
            <Loader2 className="mr-2 h-4 w-4 animate-spin" />
            <span>Submitting...</span>
          </>
        ) : (
          submitText
        )}
      </Button>
    </div>
  );
}

export default FormActions;
