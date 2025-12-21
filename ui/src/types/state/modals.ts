/**
 * Modal and Dialog State Types
 *
 * Generic modal/dialog state management with typed data.
 *
 * Citations:
 * - ui/src/hooks/chat/useChatModals.ts - Chat modal state
 * - ui/src/hooks/ui/useDialogManager.ts - Generic dialog manager
 */

/**
 * Generic modal state with optional typed data
 */
export interface ModalState<T = unknown> {
  /** Whether modal is open */
  open: boolean;
  /** Modal data payload */
  data: T | null;
}

/**
 * Modal actions
 */
export interface ModalActions<T = unknown> {
  /** Open modal with optional data */
  openModal: (data?: T) => void;
  /** Close modal and clear data */
  closeModal: () => void;
  /** Update modal data without closing */
  updateModalData: (data: Partial<T>) => void;
}

/**
 * Complete modal state with actions
 */
export interface ModalStateWithActions<T = unknown>
  extends ModalState<T>,
    ModalActions<T> {}

/**
 * Dialog state (similar to modal but with confirmation semantics)
 */
export interface DialogState<T = unknown> {
  /** Whether dialog is open */
  open: boolean;
  /** Dialog data payload */
  data: T | null;
  /** Whether dialog action is in progress */
  isPending?: boolean;
}

/**
 * Dialog actions
 */
export interface DialogActions<T = unknown> {
  /** Open dialog with optional data */
  openDialog: (data?: T) => void;
  /** Close dialog and clear data */
  closeDialog: () => void;
  /** Confirm dialog action */
  confirmDialog: () => void | Promise<void>;
  /** Cancel dialog action */
  cancelDialog: () => void;
}

/**
 * Complete dialog state with actions
 */
export interface DialogStateWithActions<T = unknown>
  extends DialogState<T>,
    DialogActions<T> {}

/**
 * Confirmation dialog data
 */
export interface ConfirmationDialogData {
  /** Dialog title */
  title: string;
  /** Dialog message/description */
  message: string;
  /** Confirm button text */
  confirmText?: string;
  /** Cancel button text */
  cancelText?: string;
  /** Severity level */
  severity?: 'info' | 'warning' | 'error' | 'success';
  /** Whether action is destructive */
  destructive?: boolean;
  /** Additional metadata */
  metadata?: Record<string, unknown>;
}

/**
 * Confirmation dialog state
 */
export interface ConfirmationDialogState extends DialogState<ConfirmationDialogData> {
  /** Callback on confirm */
  onConfirm?: () => void | Promise<void>;
  /** Callback on cancel */
  onCancel?: () => void;
}

/**
 * Multi-step dialog state
 */
export interface MultiStepDialogState<T = unknown> extends DialogState<T> {
  /** Current step index */
  currentStep: number;
  /** Total number of steps */
  totalSteps: number;
  /** Whether can go to next step */
  canGoNext: boolean;
  /** Whether can go to previous step */
  canGoPrevious: boolean;
  /** Whether can submit/finish */
  canSubmit: boolean;
}

/**
 * Multi-step dialog actions
 */
export interface MultiStepDialogActions<T = unknown> extends DialogActions<T> {
  /** Go to next step */
  nextStep: () => void;
  /** Go to previous step */
  previousStep: () => void;
  /** Go to specific step */
  goToStep: (step: number) => void;
  /** Complete dialog */
  completeDialog: () => void | Promise<void>;
}

/**
 * Drawer state (side panel)
 */
export interface DrawerState<T = unknown> {
  /** Whether drawer is open */
  open: boolean;
  /** Drawer position */
  position?: 'left' | 'right' | 'top' | 'bottom';
  /** Drawer data payload */
  data: T | null;
}

/**
 * Drawer actions
 */
export interface DrawerActions<T = unknown> {
  /** Open drawer with optional data */
  openDrawer: (data?: T, position?: 'left' | 'right' | 'top' | 'bottom') => void;
  /** Close drawer and clear data */
  closeDrawer: () => void;
  /** Update drawer data without closing */
  updateDrawerData: (data: Partial<T>) => void;
}

/**
 * Complete drawer state with actions
 */
export interface DrawerStateWithActions<T = unknown>
  extends DrawerState<T>,
    DrawerActions<T> {}

/**
 * Toast/Notification state
 */
export interface ToastState {
  /** Unique toast ID */
  id: string;
  /** Toast message */
  message: string;
  /** Toast type */
  type: 'info' | 'success' | 'warning' | 'error';
  /** Duration in ms (0 for persistent) */
  duration?: number;
  /** Whether toast is dismissible */
  dismissible?: boolean;
  /** Action button config */
  action?: {
    label: string;
    onClick: () => void;
  };
}

/**
 * Toast manager
 */
export interface ToastManager {
  /** Active toasts */
  toasts: ToastState[];
  /** Show toast */
  showToast: (message: string, type?: ToastState['type'], duration?: number) => string;
  /** Dismiss specific toast */
  dismissToast: (id: string) => void;
  /** Dismiss all toasts */
  dismissAll: () => void;
}

/**
 * Panel state (collapsible side panel)
 */
export interface PanelState {
  /** Whether panel is open */
  open: boolean;
  /** Whether panel is collapsed */
  collapsed: boolean;
  /** Panel width (when not collapsed) */
  width?: number;
}

/**
 * Panel actions
 */
export interface PanelActions {
  /** Toggle panel open/closed */
  togglePanel: () => void;
  /** Toggle panel collapsed/expanded */
  toggleCollapsed: () => void;
  /** Set panel width */
  setWidth: (width: number) => void;
  /** Open panel */
  openPanel: () => void;
  /** Close panel */
  closePanel: () => void;
}

/**
 * Complete panel state with actions
 */
export interface PanelStateWithActions extends PanelState, PanelActions {}
