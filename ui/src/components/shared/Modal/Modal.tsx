"use client";

import * as React from "react";
import * as DialogPrimitive from "@radix-ui/react-dialog";
import { XIcon } from "lucide-react";

import {
  cn,
  MENU_ANIMATION_CLASSES,
  FROST_BACKGROUND,
  FROST_OVERLAY,
  CLOSE_BUTTON_BASE,
} from "@/components/ui/utils";
import type { ModalProps, ModalSize } from "./types";
import { MODAL_SIZE_CLASSES } from "./types";

/**
 * Base Modal component built on Radix Dialog primitives.
 * Provides a flexible modal with header, body, and footer slots.
 *
 * @example
 * ```tsx
 * <Modal
 *   open={isOpen}
 *   onOpenChange={setIsOpen}
 *   title="Edit Item"
 *   description="Make changes to your item below."
 *   footer={
 *     <>
 *       <Button variant="outline" onClick={() => setIsOpen(false)}>Cancel</Button>
 *       <Button onClick={handleSave}>Save</Button>
 *     </>
 *   }
 * >
 *   <form>...</form>
 * </Modal>
 * ```
 */
export function Modal({
  open,
  onOpenChange,
  title,
  description,
  children,
  footer,
  trigger,
  size = "lg",
  showCloseButton = true,
  header,
  className,
  preventClose = false,
}: ModalProps) {
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

  return (
    <DialogPrimitive.Root open={open} onOpenChange={handleOpenChange}>
      {trigger && (
        <DialogPrimitive.Trigger asChild>{trigger}</DialogPrimitive.Trigger>
      )}
      <DialogPrimitive.Portal>
        <ModalOverlay />
        <DialogPrimitive.Content
          data-slot="modal-content"
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
          {(header || title || description) && (
            <ModalHeader>
              {header || (
                <>
                  {title && <ModalTitle>{title}</ModalTitle>}
                  {description && (
                    <ModalDescription>{description}</ModalDescription>
                  )}
                </>
              )}
            </ModalHeader>
          )}

          {/* Body */}
          {children && <ModalBody>{children}</ModalBody>}

          {/* Footer */}
          {footer && <ModalFooter>{footer}</ModalFooter>}

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
 * Modal overlay backdrop
 */
export function ModalOverlay({
  className,
  ...props
}: React.ComponentProps<typeof DialogPrimitive.Overlay>) {
  return (
    <DialogPrimitive.Overlay
      data-slot="modal-overlay"
      className={cn(
        "data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 fixed inset-0 z-50",
        FROST_OVERLAY,
        className
      )}
      {...props}
    />
  );
}

/**
 * Modal header container
 */
export function ModalHeader({
  className,
  ...props
}: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="modal-header"
      className={cn("flex flex-col gap-2 text-center sm:text-left", className)}
      {...props}
    />
  );
}

/**
 * Modal title component
 */
export function ModalTitle({
  className,
  ...props
}: React.ComponentProps<typeof DialogPrimitive.Title>) {
  return (
    <DialogPrimitive.Title
      data-slot="modal-title"
      className={cn("text-lg leading-none font-semibold", className)}
      {...props}
    />
  );
}

/**
 * Modal description component
 */
export function ModalDescription({
  className,
  ...props
}: React.ComponentProps<typeof DialogPrimitive.Description>) {
  return (
    <DialogPrimitive.Description
      data-slot="modal-description"
      className={cn("text-muted-foreground text-sm", className)}
      {...props}
    />
  );
}

/**
 * Modal body container
 */
export function ModalBody({
  className,
  ...props
}: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="modal-body"
      className={cn("flex-1 overflow-auto", className)}
      {...props}
    />
  );
}

/**
 * Modal footer container
 */
export function ModalFooter({
  className,
  ...props
}: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="modal-footer"
      className={cn(
        "flex flex-col-reverse gap-2 sm:flex-row sm:justify-end",
        className
      )}
      {...props}
    />
  );
}

/**
 * Modal trigger component (wraps children)
 */
export const ModalTrigger = DialogPrimitive.Trigger;

/**
 * Modal close component
 */
export const ModalClose = DialogPrimitive.Close;

Modal.displayName = "Modal";
ModalOverlay.displayName = "ModalOverlay";
ModalHeader.displayName = "ModalHeader";
ModalTitle.displayName = "ModalTitle";
ModalDescription.displayName = "ModalDescription";
ModalBody.displayName = "ModalBody";
ModalFooter.displayName = "ModalFooter";
