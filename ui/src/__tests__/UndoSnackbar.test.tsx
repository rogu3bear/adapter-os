import React from 'react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { UndoSnackbar } from '@/components/workbench/controls/UndoSnackbar';

const mockClearUndoAction = vi.hoisted(() => vi.fn());
const mockMutateAsync = vi.hoisted(() => vi.fn(async () => {}));
const mockSetStackSelection = vi.hoisted(() => vi.fn());
const mockClearStackSelection = vi.hoisted(() => vi.fn());
const mockToast = vi.hoisted(() => ({
  success: vi.fn(),
  error: vi.fn(),
}));

vi.mock('@/contexts/WorkbenchContext', () => ({
  useWorkbench: () => ({
    undoAction: {
      type: 'detach_all',
      previousStackId: 'stack-global-1',
      previousAdapterOverrides: {},
      previousScope: {
        selectedStackId: 'stack-session-1',
        stackName: 'Session Stack',
      },
      expiresAt: Date.now() + 10_000,
    },
    clearUndoAction: mockClearUndoAction,
  }),
}));

vi.mock('@/hooks/admin/useAdmin', () => ({
  useActivateAdapterStack: () => ({
    mutateAsync: mockMutateAsync,
  }),
}));

vi.mock('@/hooks/chat/useSessionScope', () => ({
  useSessionScope: () => ({
    setStackSelection: mockSetStackSelection,
    clearStackSelection: mockClearStackSelection,
  }),
}));

vi.mock('sonner', () => ({
  toast: mockToast,
}));

describe('UndoSnackbar', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('calls onRestoreStack when provided', async () => {
    const user = userEvent.setup();
    const onRestoreStack = vi.fn();

    render(<UndoSnackbar sessionId="session-1" onRestoreStack={onRestoreStack} />);

    await user.click(screen.getByTestId('undo-button'));

    await waitFor(() => expect(onRestoreStack).toHaveBeenCalledWith('stack-session-1'));
    expect(mockSetStackSelection).not.toHaveBeenCalled();
    expect(mockClearStackSelection).not.toHaveBeenCalled();
    expect(mockClearUndoAction).toHaveBeenCalled();
  });
});

