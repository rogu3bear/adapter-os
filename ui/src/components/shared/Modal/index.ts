// Modal components and hooks
// Built on Radix Dialog primitives

// Main components
export { Modal, ModalOverlay, ModalHeader, ModalTitle, ModalDescription, ModalBody, ModalFooter, ModalTrigger, ModalClose } from "./Modal";
export { ConfirmationModal } from "./ConfirmationModal";
export { FormModal, FormModalWithHookForm } from "./FormModal";

// Hooks (moved to @/hooks/ui/useModal - import from there or @/hooks/useDialogManager)
// export { useModal, useModalManager, useConfirmation } from "./useModal";

// Types
export type {
  ModalBaseProps,
  ModalProps,
  ConfirmationModalProps,
  FormModalProps,
  ModalSize,
  ModalState,
  UseModalReturn,
} from "./types";
export { MODAL_SIZE_CLASSES } from "./types";

// ============================================================================
// Unified Dialog Management API (Recommended)
// ============================================================================
// Re-export from @/hooks/useDialogManager for convenience

/**
 * Unified dialog management system
 *
 * @see {@link import('@/hooks/useDialogManager')} for full documentation
 *
 * @example
 * ```typescript
 * import { useAdapterDialogs } from '@/components/shared/Modal';
 *
 * const dialogs = useAdapterDialogs();
 * dialogs.openDialog('delete', { adapterId: '123', adapterName: 'My Adapter' });
 * ```
 */
export {
  createDialogManager,
  useAdapterDialogs,
  useChatDialogs,
  useTrainingDialogs,
  useDocumentDialogs,
} from "@/hooks/ui/useDialogManager";
