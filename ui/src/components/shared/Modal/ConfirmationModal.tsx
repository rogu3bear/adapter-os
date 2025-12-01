"use client";

import * as React from "react";
import * as DialogPrimitive from "@radix-ui/react-dialog";
import { AlertTriangleIcon, Loader2Icon, TrashIcon, CheckCircleIcon } from "lucide-react";

import {
  cn,
  MENU_ANIMATION_CLASSES,
  FROST_BACKGROUND,
  FROST_OVERLAY,
} from "@/components/ui/utils";
import { Button } from "@/components/ui/button";
import type { ConfirmationModalProps } from "./types";

/**
 * Default icons for each confirmation variant
 */
const VARIANT_ICONS: Record<string, React.ReactNode> = {
  destructive: <TrashIcon className="size-6 text-destructive" />,
  success: <CheckCircleIcon className="size-6 text-success" />,
  default: <AlertTriangleIcon className="size-6 text-amber-500" />,
};

/**
 * Confirmation modal for confirm/cancel actions.
 * Optimized for delete confirmations, destructive actions, and simple yes/no decisions.
 *
 * @example
 * ```tsx
 * <ConfirmationModal
 *   open={isOpen}
 *   onOpenChange={setIsOpen}
 *   title="Delete Item?"
 *   description="This action cannot be undone. This will permanently delete the item."
 *   confirmText="Delete"
 *   confirmVariant="destructive"
 *   onConfirm={handleDelete}
 *   isLoading={isDeleting}
 * />
 * ```
 */
export function ConfirmationModal({
  open,
  onOpenChange,
  title,
  description,
  confirmText = "Confirm",
  cancelText = "Cancel",
  confirmVariant = "default",
  onConfirm,
  onCancel,
  isLoading = false,
  icon,
  className,
  preventClose = false,
}: ConfirmationModalProps) {
  const handleConfirm = React.useCallback(async () => {
    await onConfirm();
    if (!preventClose) {
      onOpenChange(false);
    }
  }, [onConfirm, onOpenChange, preventClose]);

  const handleCancel = React.useCallback(() => {
    onCancel?.();
    onOpenChange(false);
  }, [onCancel, onOpenChange]);

  const handleOpenChange = React.useCallback(
    (open: boolean) => {
      if (preventClose && !open) {
        return;
      }
      if (!open) {
        onCancel?.();
      }
      onOpenChange(open);
    },
    [onOpenChange, onCancel, preventClose]
  );

  const handleEscapeKeyDown = React.useCallback(
    (event: KeyboardEvent) => {
      if (preventClose || isLoading) {
        event.preventDefault();
      }
    },
    [preventClose, isLoading]
  );

  const handlePointerDownOutside = React.useCallback(
    (event: CustomEvent) => {
      if (preventClose || isLoading) {
        event.preventDefault();
      }
    },
    [preventClose, isLoading]
  );

  const displayIcon = icon ?? VARIANT_ICONS[confirmVariant];

  return (
    <DialogPrimitive.Root open={open} onOpenChange={handleOpenChange}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay
          data-slot="confirmation-modal-overlay"
          className={cn(
            "data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 fixed inset-0 z-50",
            FROST_OVERLAY
          )}
        />
        <DialogPrimitive.Content
          data-slot="confirmation-modal-content"
          className={cn(
            FROST_BACKGROUND,
            MENU_ANIMATION_CLASSES,
            "fixed top-[50%] left-[50%] z-50 grid w-full max-w-[calc(100%-2rem)] translate-x-[-50%] translate-y-[-50%] gap-4 rounded-lg border p-6 shadow-lg duration-200 sm:max-w-md",
            className
          )}
          onEscapeKeyDown={handleEscapeKeyDown}
          onPointerDownOutside={handlePointerDownOutside}
        >
          {/* Icon and Header */}
          <div className="flex flex-col items-center gap-4 text-center sm:flex-row sm:items-start sm:text-left">
            {displayIcon && (
              <div className="flex size-12 shrink-0 items-center justify-center rounded-full bg-muted">
                {displayIcon}
              </div>
            )}
            <div className="flex flex-col gap-2">
              <DialogPrimitive.Title
                data-slot="confirmation-modal-title"
                className="text-lg font-semibold"
              >
                {title}
              </DialogPrimitive.Title>
              {description && (
                <DialogPrimitive.Description
                  data-slot="confirmation-modal-description"
                  className="text-muted-foreground text-sm"
                >
                  {description}
                </DialogPrimitive.Description>
              )}
            </div>
          </div>

          {/* Actions */}
          <div className="flex flex-col-reverse gap-2 sm:flex-row sm:justify-end">
            <Button
              variant="outline"
              onClick={handleCancel}
              disabled={isLoading}
              data-slot="confirmation-modal-cancel"
            >
              {cancelText}
            </Button>
            <Button
              variant={confirmVariant}
              onClick={handleConfirm}
              disabled={isLoading}
              data-slot="confirmation-modal-confirm"
            >
              {isLoading && <Loader2Icon className="size-4 animate-spin" />}
              {confirmText}
            </Button>
          </div>
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}

ConfirmationModal.displayName = "ConfirmationModal";
