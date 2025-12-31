/**
 * @deprecated Use ErrorBoundary from '@/components/ui/error-boundary' with scope="modal"
 * @example
 * import { ErrorBoundary } from '@/components/ui/error-boundary';
 * <ErrorBoundary scope="modal" onClose={handleClose}>...</ErrorBoundary>
 */
import React from 'react';
import { ErrorBoundary } from './error-boundary';
import { UIErrorContext } from '@/lib/logUIError';

interface ModalErrorBoundaryProps {
  children: React.ReactNode;
  context?: Pick<UIErrorContext, 'route' | 'pageKey' | 'component'>;
  onClose?: () => void;
}

/**
 * @deprecated Use ErrorBoundary with scope="modal" instead
 */
export function ModalErrorBoundary({ children, context, onClose }: ModalErrorBoundaryProps) {
  return (
    <ErrorBoundary
      scope="modal"
      onClose={onClose}
      context={context}
      name={context?.component}
    >
      {children}
    </ErrorBoundary>
  );
}

export default ModalErrorBoundary;

