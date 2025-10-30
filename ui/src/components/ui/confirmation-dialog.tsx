//! Confirmation Dialog Component
//!
//! Provides consistent confirmation patterns for destructive actions.
//!
//! Citations:
//! - docs/Smashing Design Techniques.md L300-L350 - Error recovery UX patterns
//! - ui/src/components/Adapters.tsx L200-L250 - Current confirmation patterns

import React from 'react';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from './alert-dialog';
import { AlertTriangle } from 'lucide-react';

export interface ConfirmationOptions {
  title: string;
  description: string;
  confirmText?: string;
  cancelText?: string;
  variant?: 'default' | 'destructive';
}

interface ConfirmationDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onConfirm: () => void | Promise<void>;
  options: ConfirmationOptions;
}

export function ConfirmationDialog({
  open,
  onOpenChange,
  onConfirm,
  options
}: ConfirmationDialogProps) {
  const handleConfirm = async () => {
    try {
      await onConfirm();
      onOpenChange(false);
    } catch (error) {
      // Error handling is done in parent component
    }
  };

  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <div className="flex items-center gap-3">
            {options.variant === 'destructive' && (
              <AlertTriangle className="h-5 w-5 text-destructive" />
            )}
            <AlertDialogTitle>{options.title}</AlertDialogTitle>
          </div>
          <AlertDialogDescription className="text-left">
            {options.description}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>{options.cancelText || 'Cancel'}</AlertDialogCancel>
          <AlertDialogAction
            onClick={handleConfirm}
            className={options.variant === 'destructive' ? 'bg-destructive text-destructive-foreground hover:bg-destructive/90' : ''}
          >
            {options.confirmText || 'Confirm'}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

export default ConfirmationDialog;
