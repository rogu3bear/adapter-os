"use client";

import * as React from "react";

/**
 * Base props shared across all modal variants
 */
export interface ModalBaseProps {
  /** Whether the modal is currently open */
  open: boolean;
  /** Callback when the open state changes */
  onOpenChange: (open: boolean) => void;
  /** Optional CSS class for the modal content */
  className?: string;
  /** Prevents closing when clicking outside or pressing Escape */
  preventClose?: boolean;
}

/**
 * Props for the base Modal component
 */
export interface ModalProps extends ModalBaseProps {
  /** Modal title displayed in the header */
  title?: React.ReactNode;
  /** Optional description text below the title */
  description?: React.ReactNode;
  /** Content rendered in the modal body */
  children?: React.ReactNode;
  /** Content rendered in the modal footer */
  footer?: React.ReactNode;
  /** Optional trigger element that opens the modal */
  trigger?: React.ReactNode;
  /** Size variant of the modal */
  size?: ModalSize;
  /** Whether to show the close button in the header */
  showCloseButton?: boolean;
  /** Custom header content (replaces title/description) */
  header?: React.ReactNode;
}

/**
 * Props for the ConfirmationModal component
 */
export interface ConfirmationModalProps extends ModalBaseProps {
  /** Modal title */
  title: string;
  /** Description or message to display */
  description?: React.ReactNode;
  /** Text for the confirm button */
  confirmText?: string;
  /** Text for the cancel button */
  cancelText?: string;
  /** Variant style for the confirm button */
  confirmVariant?: "default" | "destructive" | "success";
  /** Callback when confirm is clicked */
  onConfirm: () => void | Promise<void>;
  /** Callback when cancel is clicked */
  onCancel?: () => void;
  /** Whether the confirm action is in progress */
  isLoading?: boolean;
  /** Custom icon to display */
  icon?: React.ReactNode;
}

/**
 * Props for the FormModal component
 */
export interface FormModalProps<T = unknown> extends ModalBaseProps {
  /** Modal title */
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
  /** Size variant of the modal */
  size?: ModalSize;
  /** Whether to reset form state when modal closes */
  resetOnClose?: boolean;
}

/**
 * Size variants for modals
 */
export type ModalSize = "sm" | "md" | "lg" | "xl" | "full";

/**
 * Modal state for the useModal hook
 */
export interface ModalState {
  /** Whether the modal is open */
  isOpen: boolean;
  /** Data associated with the modal */
  data?: unknown;
}

/**
 * Return type for the useModal hook
 */
export interface UseModalReturn<T = unknown> {
  /** Whether the modal is open */
  isOpen: boolean;
  /** Data passed to the modal when opened */
  data: T | undefined;
  /** Opens the modal, optionally with data */
  open: (data?: T) => void;
  /** Closes the modal */
  close: () => void;
  /** Toggles the modal open/closed state */
  toggle: () => void;
  /** Callback to pass to onOpenChange prop */
  onOpenChange: (open: boolean) => void;
}

/**
 * Size classes mapping for modal content
 */
export const MODAL_SIZE_CLASSES: Record<ModalSize, string> = {
  sm: "sm:max-w-sm",
  md: "sm:max-w-md",
  lg: "sm:max-w-lg",
  xl: "sm:max-w-xl",
  full: "sm:max-w-[calc(100vw-4rem)] sm:max-h-[calc(100vh-4rem)]",
};
