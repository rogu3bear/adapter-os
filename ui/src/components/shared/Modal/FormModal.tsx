"use client";

import * as React from "react";
import * as DialogPrimitive from "@radix-ui/react-dialog";
import { XIcon, Loader2Icon } from "lucide-react";

import {
  cn,
  MENU_ANIMATION_CLASSES,
  FROST_BACKGROUND,
  FROST_OVERLAY,
  CLOSE_BUTTON_BASE,
} from "@/components/ui/utils";
import { Button } from "@/components/ui/button";
import type { FormModalProps, ModalSize } from "./types";
import { MODAL_SIZE_CLASSES } from "./types";

/**
 * Modal optimized for forms with built-in submission handling and validation support.
 *
 * @example
 * ```tsx
 * const { register, handleSubmit, formState } = useForm<FormData>();
 *
 * <FormModal
 *   open={isOpen}
 *   onOpenChange={setIsOpen}
 *   title="Create Item"
 *   description="Fill in the details below."
 *   onSubmit={handleSubmit(onSubmit)}
 *   isSubmitting={isSubmitting}
 *   isValid={formState.isValid}
 * >
 *   <FormField>
 *     <FormLabel>Name</FormLabel>
 *     <Input {...register("name")} />
 *   </FormField>
 * </FormModal>
 * ```
 */
export function FormModal<T = unknown>({
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
}: FormModalProps<T>) {
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
          data-slot="form-modal-overlay"
          className={cn(
            "data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 fixed inset-0 z-50",
            FROST_OVERLAY
          )}
        />
        <DialogPrimitive.Content
          data-slot="form-modal-content"
          className={cn(
            FROST_BACKGROUND,
            MENU_ANIMATION_CLASSES,
            "fixed top-[50%] left-[50%] z-50 grid w-full max-w-[calc(100%-2rem)] translate-x-[-50%] translate-y-[-50%] gap-4 rounded-lg border p-6 shadow-lg duration-200",
            MODAL_SIZE_CLASSES[size],
            className
          )}
          onEscapeKeyDown={handleEscapeKeyDown}
          onPointerDownOutside={handlePointerDownOutside}
          onKeyDown={handleKeyDown}
        >
          {/* Header */}
          <div className="flex flex-col gap-2 text-center sm:text-left">
            <DialogPrimitive.Title
              data-slot="form-modal-title"
              className="text-lg leading-none font-semibold"
            >
              {title}
            </DialogPrimitive.Title>
            {description && (
              <DialogPrimitive.Description
                data-slot="form-modal-description"
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
            data-slot="form-modal-form"
          >
            {/* Form Content */}
            <div className="flex flex-col gap-4" data-slot="form-modal-body">
              {children}
            </div>

            {/* Actions */}
            <div
              className="flex flex-col-reverse gap-2 sm:flex-row sm:justify-end"
              data-slot="form-modal-footer"
            >
              <Button
                type="button"
                variant="outline"
                onClick={handleCancel}
                disabled={isSubmitting}
                data-slot="form-modal-cancel"
              >
                {cancelText}
              </Button>
              <Button
                type="submit"
                disabled={isSubmitting || !isValid}
                data-slot="form-modal-submit"
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

/**
 * Wrapper component for using FormModal with react-hook-form
 *
 * @example
 * ```tsx
 * const form = useForm<FormData>();
 *
 * <FormModalWithHookForm
 *   open={isOpen}
 *   onOpenChange={setIsOpen}
 *   title="Create Item"
 *   form={form}
 *   onSubmit={onSubmit}
 * >
 *   <FormField
 *     control={form.control}
 *     name="name"
 *     render={({ field }) => (
 *       <FormItem>
 *         <FormLabel>Name</FormLabel>
 *         <FormControl>
 *           <Input {...field} />
 *         </FormControl>
 *       </FormItem>
 *     )}
 *   />
 * </FormModalWithHookForm>
 * ```
 */
export function FormModalWithHookForm<T extends Record<string, unknown>>({
  open,
  onOpenChange,
  title,
  description,
  children,
  submitText = "Submit",
  cancelText = "Cancel",
  onSubmit,
  onCancel,
  form,
  size = "lg",
  className,
  preventClose = false,
}: Omit<FormModalProps<T>, "isSubmitting" | "isValid" | "resetOnClose"> & {
  form: {
    handleSubmit: (
      onSubmit: (data: T) => void | Promise<void>
    ) => (event: React.FormEvent<HTMLFormElement>) => void;
    formState: { isSubmitting: boolean; isValid: boolean };
    reset: () => void;
  };
}) {
  const { isSubmitting, isValid } = form.formState;

  const handleOpenChange = React.useCallback(
    (open: boolean) => {
      if (preventClose && !open) {
        return;
      }
      if (!open) {
        onCancel?.();
        form.reset();
      }
      onOpenChange(open);
    },
    [onOpenChange, onCancel, preventClose, form]
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

  const handleCancel = React.useCallback(() => {
    onCancel?.();
    onOpenChange(false);
  }, [onCancel, onOpenChange]);

  return (
    <DialogPrimitive.Root open={open} onOpenChange={handleOpenChange}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay
          data-slot="form-modal-overlay"
          className={cn(
            "data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 fixed inset-0 z-50",
            FROST_OVERLAY
          )}
        />
        <DialogPrimitive.Content
          data-slot="form-modal-content"
          className={cn(
            FROST_BACKGROUND,
            MENU_ANIMATION_CLASSES,
            "fixed top-[50%] left-[50%] z-50 grid w-full max-w-[calc(100%-2rem)] translate-x-[-50%] translate-y-[-50%] gap-4 rounded-lg border p-6 shadow-lg duration-200",
            MODAL_SIZE_CLASSES[size],
            className
          )}
          onEscapeKeyDown={handleEscapeKeyDown}
          onPointerDownOutside={handlePointerDownOutside}
        >
          {/* Header */}
          <div className="flex flex-col gap-2 text-center sm:text-left">
            <DialogPrimitive.Title
              data-slot="form-modal-title"
              className="text-lg leading-none font-semibold"
            >
              {title}
            </DialogPrimitive.Title>
            {description && (
              <DialogPrimitive.Description
                data-slot="form-modal-description"
                className="text-muted-foreground text-sm"
              >
                {description}
              </DialogPrimitive.Description>
            )}
          </div>

          {/* Form */}
          <form
            onSubmit={form.handleSubmit(async (data) => {
              await onSubmit(data);
              if (!preventClose) {
                onOpenChange(false);
              }
            })}
            className="flex flex-col gap-4"
            data-slot="form-modal-form"
          >
            {/* Form Content */}
            <div className="flex flex-col gap-4" data-slot="form-modal-body">
              {children}
            </div>

            {/* Actions */}
            <div
              className="flex flex-col-reverse gap-2 sm:flex-row sm:justify-end"
              data-slot="form-modal-footer"
            >
              <Button
                type="button"
                variant="outline"
                onClick={handleCancel}
                disabled={isSubmitting}
                data-slot="form-modal-cancel"
              >
                {cancelText}
              </Button>
              <Button
                type="submit"
                disabled={isSubmitting || !isValid}
                data-slot="form-modal-submit"
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

FormModal.displayName = "FormModal";
FormModalWithHookForm.displayName = "FormModalWithHookForm";
