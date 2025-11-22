// Modal components and hooks
// Built on Radix Dialog primitives

// Main components
export { Modal, ModalOverlay, ModalHeader, ModalTitle, ModalDescription, ModalBody, ModalFooter, ModalTrigger, ModalClose } from "./Modal";
export { ConfirmationModal } from "./ConfirmationModal";
export { FormModal, FormModalWithHookForm } from "./FormModal";

// Hooks
export { useModal, useModalManager, useConfirmation } from "./useModal";

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
