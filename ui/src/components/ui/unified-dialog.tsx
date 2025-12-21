"use client";

import * as React from "react";
import * as DialogPrimitive from "@radix-ui/react-dialog";
import { XIcon, Loader2Icon, AlertTriangleIcon, CheckCircleIcon, TrashIcon } from "lucide-react";
import { cva, type VariantProps } from "class-variance-authority";

import {
  cn,
  MENU_ANIMATION_CLASSES,
  FROST_BACKGROUND,
  FROST_OVERLAY,
  CLOSE_BUTTON_BASE,
} from "@/lib/utils";
import { Button } from "./button";

/**
 * Dialog size variants
 */
export type UnifiedDialogSize = "sm" | "md" | "lg" | "xl" | "full";

/**
 * Dialog visual variants
 */
export type UnifiedDialogVariant = "default" | "destructive" | "form";

/**
 * Size classes mapping for dialog content
 */
const DIALOG_SIZE_CLASSES: Record<UnifiedDialogSize, string> = {
  sm: "sm:max-w-sm",
  md: "sm:max-w-md",
  lg: "sm:max-w-lg",
  xl: "sm:max-w-xl",
  full: "sm:max-w-[calc(100vw-4rem)] sm:max-h-[calc(100vh-4rem)]",
};

/**
 * Default icons for each variant
 */
const VARIANT_ICONS: Record<string, React.ReactNode> = {
  destructive: <TrashIcon className="size-6 text-destructive" />,
  success: <CheckCircleIcon className="size-6 text-success" />,
  warning: <AlertTriangleIcon className="size-6 text-amber-500" />,
};

/**
 * Variant-specific button configurations
 */
const VARIANT_BUTTON_STYLES: Record<UnifiedDialogVariant, string> = {
  default: "default",
  destructive: "destructive",
  form: "default",
};

/**
 * Base props shared across all dialog configurations
 */
export interface UnifiedDialogBaseProps {
  /** Whether the dialog is currently open */
  open: boolean;
  /** Callback when the open state changes */
  onOpenChange: (open: boolean) => void;
  /** Optional CSS class for the dialog content */
  className?: string;
  /** Prevents closing when clicking outside or pressing Escape */
  preventClose?: boolean;
}

/**
 * Core props for the UnifiedDialog component
 */
export interface UnifiedDialogProps extends UnifiedDialogBaseProps {
  /** Dialog title displayed in the header */
  title?: React.ReactNode;
  /** Optional description text below the title */
  description?: React.ReactNode;
  /** Content rendered in the dialog body */
  children?: React.ReactNode;
  /** Content rendered in the dialog footer */
  footer?: React.ReactNode;
  /** Optional trigger element that opens the dialog */
  trigger?: React.ReactNode;
  /** Size variant of the dialog */
  size?: UnifiedDialogSize;
  /** Visual variant of the dialog */
  variant?: UnifiedDialogVariant;
  /** Whether to show the close button in the header */
  showCloseButton?: boolean;
  /** Custom header content (replaces title/description) */
  header?: React.ReactNode;
  /** Optional icon to display (for confirmation/alert dialogs) */
  icon?: React.ReactNode;
  /** Icon variant (uses predefined icons if not custom) */
  iconVariant?: "destructive" | "success" | "warning";
  /** Whether to show icon in a circular background */
  showIconBackground?: boolean;
}

/**
 * Props for confirmation-style dialogs
 */
export interface UnifiedDialogConfirmationProps extends UnifiedDialogBaseProps {
  /** Dialog title */
  title: string;
  /** Description or message to display */
  description?: React.ReactNode;
  /** Text for the confirm button */
  confirmText?: string;
  /** Text for the cancel button */
  cancelText?: string;
  /** Visual variant for the confirm button */
  confirmVariant?: "default" | "destructive" | "success";
  /** Callback when confirm is clicked */
  onConfirm: () => void | Promise<void>;
  /** Callback when cancel is clicked */
  onCancel?: () => void;
  /** Whether the confirm action is in progress */
  isLoading?: boolean;
  /** Custom icon to display */
  icon?: React.ReactNode;
  /** Icon variant (uses predefined icons if not custom) */
  iconVariant?: "destructive" | "success" | "warning";
  /** Dialog size */
  size?: UnifiedDialogSize;
}

/**
 * Props for form-style dialogs
 */
export interface UnifiedDialogFormProps<T = unknown> extends UnifiedDialogBaseProps {
  /** Dialog title */
  title: string;
  /** Optional description */
  description?: React.ReactNode;
  /** Form content */
  children: React.ReactNode;
  /** Text for the submit button */
  submitText?: string;
  /** Text for the cancel button */
  cancelText?: string;
  /** Callback when form is submitted */
  onSubmit: (data: T) => void | Promise<void>;
  /** Callback when cancel is clicked */
  onCancel?: () => void;
  /** Whether the form submission is in progress */
  isSubmitting?: boolean;
  /** Whether the form is currently valid */
  isValid?: boolean;
  /** Dialog size */
  size?: UnifiedDialogSize;
  /** Whether to reset form state when dialog closes */
  resetOnClose?: boolean;
}

/**
 * UnifiedDialog - A comprehensive dialog component that handles multiple use cases:
 * - Basic content dialogs
 * - Confirmation/alert dialogs
 * - Form dialogs
 *
 * Built on Radix UI Dialog primitives with consistent styling and behavior.
 *
 * @example Basic dialog
 * ```tsx
 * <UnifiedDialog
 *   open={isOpen}
 *   onOpenChange={setIsOpen}
 *   title="Settings"
 *   description="Manage your preferences"
 *   footer={
 *     <Button onClick={() => setIsOpen(false)}>Close</Button>
 *   }
 * >
 *   <div>Settings content...</div>
 * </UnifiedDialog>
 * ```
 *
 * @example Confirmation dialog
 * ```tsx
 * <UnifiedDialog.Confirmation
 *   open={isOpen}
 *   onOpenChange={setIsOpen}
 *   title="Delete Item?"
 *   description="This action cannot be undone."
 *   confirmText="Delete"
 *   confirmVariant="destructive"
 *   onConfirm={handleDelete}
 *   isLoading={isDeleting}
 * />
 * ```
 *
 * @example Form dialog
 * ```tsx
 * <UnifiedDialog.Form
 *   open={isOpen}
 *   onOpenChange={setIsOpen}
 *   title="Create Item"
 *   onSubmit={handleSubmit}
 *   isSubmitting={isSubmitting}
 *   isValid={isValid}
 * >
 *   <input name="title" />
 * </UnifiedDialog.Form>
 * ```
 */
export function UnifiedDialog({
  open,
  onOpenChange,
  title,
  description,
  children,
  footer,
  trigger,
  size = "lg",
  variant = "default",
  showCloseButton = true,
  header,
  icon,
  iconVariant,
  showIconBackground = true,
  className,
  preventClose = false,
}: UnifiedDialogProps) {
  const handleOpenChange = React.useCallback(
    (open: boolean) => {
      if (preventClose && !open) {
        return;
      }
      onOpenChange(open);
    },
    [onOpenChange, preventClose]
  );

  const handleEscapeKeyDown = React.useCallback(
    (event: KeyboardEvent) => {
      if (preventClose) {
        event.preventDefault();
      }
    },
    [preventClose]
  );

  const handlePointerDownOutside = React.useCallback(
    (event: CustomEvent) => {
      if (preventClose) {
        event.preventDefault();
      }
    },
    [preventClose]
  );

  const displayIcon = icon ?? (iconVariant ? VARIANT_ICONS[iconVariant] : null);
  const hasIcon = !!displayIcon;

  return (
    <DialogPrimitive.Root open={open} onOpenChange={handleOpenChange}>
      {trigger && (
        <DialogPrimitive.Trigger asChild>{trigger}</DialogPrimitive.Trigger>
      )}
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay
          data-slot="unified-dialog-overlay"
          className={cn(
            "data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 fixed inset-0 z-50",
            FROST_OVERLAY
          )}
        />
        <DialogPrimitive.Content
          data-slot="unified-dialog-content"
          data-variant={variant}
          className={cn(
            FROST_BACKGROUND,
            MENU_ANIMATION_CLASSES,
            "fixed top-[50%] left-[50%] z-50 grid w-full max-w-[calc(100%-2rem)] translate-x-[-50%] translate-y-[-50%] gap-4 rounded-lg border p-6 shadow-lg duration-200",
            DIALOG_SIZE_CLASSES[size],
            className
          )}
          onEscapeKeyDown={handleEscapeKeyDown}
          onPointerDownOutside={handlePointerDownOutside}
        >
          {/* Header */}
          {(header || title || description || hasIcon) && (
            <div
              data-slot="unified-dialog-header"
              className={cn(
                "flex flex-col gap-2",
                hasIcon
                  ? "flex-col items-center gap-4 text-center sm:flex-row sm:items-start sm:text-left"
                  : "text-center sm:text-left"
              )}
            >
              {hasIcon && showIconBackground && (
                <div className="flex size-12 shrink-0 items-center justify-center rounded-full bg-muted">
                  {displayIcon}
                </div>
              )}
              {hasIcon && !showIconBackground && (
                <div className="flex shrink-0 items-center justify-center">
                  {displayIcon}
                </div>
              )}
              <div className="flex flex-col gap-2">
                {header || (
                  <>
                    {title && (
                      <DialogPrimitive.Title
                        data-slot="unified-dialog-title"
                        className="text-lg leading-none font-semibold"
                      >
                        {title}
                      </DialogPrimitive.Title>
                    )}
                    {description && (
                      <DialogPrimitive.Description
                        data-slot="unified-dialog-description"
                        className="text-muted-foreground text-sm"
                      >
                        {description}
                      </DialogPrimitive.Description>
                    )}
                  </>
                )}
              </div>
            </div>
          )}

          {/* Body */}
          {children && (
            <div data-slot="unified-dialog-body" className="flex-1 overflow-auto">
              {children}
            </div>
          )}

          {/* Footer */}
          {footer && (
            <div
              data-slot="unified-dialog-footer"
              className="flex flex-col-reverse gap-2 sm:flex-row sm:justify-end"
            >
              {footer}
            </div>
          )}

          {/* Close Button */}
          {showCloseButton && !preventClose && (
            <DialogPrimitive.Close
              className={cn(
                CLOSE_BUTTON_BASE,
                "data-[state=open]:bg-accent data-[state=open]:text-muted-foreground [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4"
              )}
            >
              <XIcon />
              <span className="sr-only">Close</span>
            </DialogPrimitive.Close>
          )}
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}

/**
 * UnifiedDialog.Confirmation - Optimized for confirm/cancel actions
 *
 * @example
 * ```tsx
 * <UnifiedDialog.Confirmation
 *   open={isOpen}
 *   onOpenChange={setIsOpen}
 *   title="Delete Item?"
 *   description="This action cannot be undone."
 *   confirmText="Delete"
 *   confirmVariant="destructive"
 *   onConfirm={handleDelete}
 *   isLoading={isDeleting}
 *   iconVariant="destructive"
 * />
 * ```
 */
export function UnifiedDialogConfirmation({
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
  iconVariant,
  size = "md",
  className,
  preventClose = false,
}: UnifiedDialogConfirmationProps) {
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

  // Use iconVariant matching confirmVariant if no explicit icon/iconVariant provided
  const effectiveIconVariant = iconVariant ?? (confirmVariant === "destructive" ? "destructive" : confirmVariant === "success" ? "success" : "warning");
  const displayIcon = icon ?? VARIANT_ICONS[effectiveIconVariant];

  return (
    <DialogPrimitive.Root open={open} onOpenChange={handleOpenChange}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay
          data-slot="unified-dialog-confirmation-overlay"
          className={cn(
            "data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 fixed inset-0 z-50",
            FROST_OVERLAY
          )}
        />
        <DialogPrimitive.Content
          data-slot="unified-dialog-confirmation-content"
          className={cn(
            FROST_BACKGROUND,
            MENU_ANIMATION_CLASSES,
            "fixed top-[50%] left-[50%] z-50 grid w-full max-w-[calc(100%-2rem)] translate-x-[-50%] translate-y-[-50%] gap-4 rounded-lg border p-6 shadow-lg duration-200",
            DIALOG_SIZE_CLASSES[size],
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
                data-slot="unified-dialog-confirmation-title"
                className="text-lg font-semibold"
              >
                {title}
              </DialogPrimitive.Title>
              {description && (
                <DialogPrimitive.Description
                  data-slot="unified-dialog-confirmation-description"
                  className="text-muted-foreground text-sm"
                >
                  {description}
                </DialogPrimitive.Description>
              )}
            </div>
          </div>

          {/* Actions */}
          <div
            data-slot="unified-dialog-confirmation-footer"
            className="flex flex-col-reverse gap-2 sm:flex-row sm:justify-end"
          >
            <Button
              variant="outline"
              onClick={handleCancel}
              disabled={isLoading}
              data-slot="unified-dialog-confirmation-cancel"
            >
              {cancelText}
            </Button>
            <Button
              variant={confirmVariant}
              onClick={handleConfirm}
              disabled={isLoading}
              data-slot="unified-dialog-confirmation-confirm"
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

/**
 * UnifiedDialog.Form - Optimized for forms with built-in submission handling
 *
 * @example
 * ```tsx
 * <UnifiedDialog.Form
 *   open={isOpen}
 *   onOpenChange={setIsOpen}
 *   title="Create Item"
 *   description="Fill in the details below."
 *   onSubmit={handleSubmit}
 *   isSubmitting={isSubmitting}
 *   isValid={isValid}
 * >
 *   <input name="title" />
 *   <textarea name="description" />
 * </UnifiedDialog.Form>
 * ```
 */
export function UnifiedDialogForm<T = unknown>({
  open,
  onOpenChange,
  title,
  description,
  children,
  submitText = "Submit",
  cancelText = "Cancel",
  onSubmit,
  onCancel,
  isSubmitting = false,
  isValid = true,
  size = "lg",
  className,
  preventClose = false,
  resetOnClose = true,
}: UnifiedDialogFormProps<T>) {
  const formRef = React.useRef<HTMLFormElement>(null);

  const handleSubmit = React.useCallback(
    async (event: React.FormEvent<HTMLFormElement>) => {
      event.preventDefault();

      // Get form data
      const formData = new FormData(event.currentTarget);
      const data = Object.fromEntries(formData.entries()) as T;

      await onSubmit(data);

      if (!preventClose) {
        onOpenChange(false);
      }
    },
    [onSubmit, onOpenChange, preventClose]
  );

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
        if (resetOnClose && formRef.current) {
          formRef.current.reset();
        }
      }
      onOpenChange(open);
    },
    [onOpenChange, onCancel, preventClose, resetOnClose]
  );

  const handleEscapeKeyDown = React.useCallback(
    (event: KeyboardEvent) => {
      if (preventClose || isSubmitting) {
        event.preventDefault();
      }
    },
    [preventClose, isSubmitting]
  );

  const handlePointerDownOutside = React.useCallback(
    (event: CustomEvent) => {
      if (preventClose || isSubmitting) {
        event.preventDefault();
      }
    },
    [preventClose, isSubmitting]
  );

  // Handle keyboard shortcut for submit (Cmd/Ctrl + Enter)
  const handleKeyDown = React.useCallback(
    (event: React.KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
        if (isValid && !isSubmitting) {
          formRef.current?.requestSubmit();
        }
      }
    },
    [isValid, isSubmitting]
  );

  return (
    <DialogPrimitive.Root open={open} onOpenChange={handleOpenChange}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay
          data-slot="unified-dialog-form-overlay"
          className={cn(
            "data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 fixed inset-0 z-50",
            FROST_OVERLAY
          )}
        />
        <DialogPrimitive.Content
          data-slot="unified-dialog-form-content"
          className={cn(
            FROST_BACKGROUND,
            MENU_ANIMATION_CLASSES,
            "fixed top-[50%] left-[50%] z-50 grid w-full max-w-[calc(100%-2rem)] translate-x-[-50%] translate-y-[-50%] gap-4 rounded-lg border p-6 shadow-lg duration-200",
            DIALOG_SIZE_CLASSES[size],
            className
          )}
          onEscapeKeyDown={handleEscapeKeyDown}
          onPointerDownOutside={handlePointerDownOutside}
          onKeyDown={handleKeyDown}
        >
          {/* Header */}
          <div className="flex flex-col gap-2 text-center sm:text-left">
            <DialogPrimitive.Title
              data-slot="unified-dialog-form-title"
              className="text-lg leading-none font-semibold"
            >
              {title}
            </DialogPrimitive.Title>
            {description && (
              <DialogPrimitive.Description
                data-slot="unified-dialog-form-description"
                className="text-muted-foreground text-sm"
              >
                {description}
              </DialogPrimitive.Description>
            )}
          </div>

          {/* Form */}
          <form
            ref={formRef}
            onSubmit={handleSubmit}
            className="flex flex-col gap-4"
            data-slot="unified-dialog-form-form"
          >
            {/* Form Content */}
            <div className="flex flex-col gap-4" data-slot="unified-dialog-form-body">
              {children}
            </div>

            {/* Actions */}
            <div
              className="flex flex-col-reverse gap-2 sm:flex-row sm:justify-end"
              data-slot="unified-dialog-form-footer"
            >
              <Button
                type="button"
                variant="outline"
                onClick={handleCancel}
                disabled={isSubmitting}
                data-slot="unified-dialog-form-cancel"
              >
                {cancelText}
              </Button>
              <Button
                type="submit"
                disabled={isSubmitting || !isValid}
                data-slot="unified-dialog-form-submit"
              >
                {isSubmitting && <Loader2Icon className="size-4 animate-spin" />}
                {submitText}
              </Button>
            </div>
          </form>

          {/* Close Button */}
          {!preventClose && (
            <DialogPrimitive.Close
              className={cn(
                CLOSE_BUTTON_BASE,
                "data-[state=open]:bg-accent data-[state=open]:text-muted-foreground [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4"
              )}
              disabled={isSubmitting}
            >
              <XIcon />
              <span className="sr-only">Close</span>
            </DialogPrimitive.Close>
          )}
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}

// Attach sub-components to main component
UnifiedDialog.Confirmation = UnifiedDialogConfirmation;
UnifiedDialog.Form = UnifiedDialogForm;

// Display names
UnifiedDialog.displayName = "UnifiedDialog";
UnifiedDialogConfirmation.displayName = "UnifiedDialog.Confirmation";
UnifiedDialogForm.displayName = "UnifiedDialog.Form";
