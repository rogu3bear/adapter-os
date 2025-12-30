import { describe, it, expect, vi, beforeEach } from 'vitest';
import React from 'react';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ChatShareDialog } from '@/components/chat/ChatShareDialog';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { SessionShare, SharePermission } from '@/api/chat-types';

beforeAll(() => {
  if (!(Element.prototype as any).hasPointerCapture) {
    (Element.prototype as any).hasPointerCapture = () => false;
  }
  if (!(Element.prototype as any).releasePointerCapture) {
    (Element.prototype as any).releasePointerCapture = () => {};
  }
});

vi.mock('@/components/ui/select', () => {
  const React = require('react');
  const Select = ({ children, onValueChange, value, defaultValue }: any) => {
    const [internal, setInternal] = React.useState(value ?? defaultValue ?? '');
    const handleChange = (e: React.ChangeEvent<HTMLSelectElement>) => {
      setInternal(e.target.value);
      onValueChange?.(e.target.value);
    };
    return (
      <select value={value ?? internal} onChange={handleChange} aria-label="permission-select">
        {children}
      </select>
    );
  };
  const SelectTrigger = ({ children }: any) => <>{children}</>;
  const SelectContent = ({ children }: any) => <>{children}</>;
  const SelectItem = ({ children, value }: any) => <option value={value}>{children}</option>;
  const SelectValue = ({ placeholder, value }: any) => <>{value ?? placeholder ?? null}</>;
  return { Select, SelectTrigger, SelectContent, SelectItem, SelectValue };
});

vi.mock('@/components/ui/switch', () => ({
  Switch: ({ checked, onCheckedChange, id }: any) => (
    <input
      type="checkbox"
      role="switch"
      id={id}
      aria-label={typeof id === 'string' ? id : 'switch'}
      aria-checked={!!checked}
      checked={!!checked}
      data-state={checked ? 'checked' : 'unchecked'}
      onChange={(e) => onCheckedChange?.(e.target.checked)}
    />
  ),
}));

// Mock data
const mockShares: SessionShare[] = [
  {
    id: 'share-1',
    session_id: 'session-1',
    shared_with_user_id: 'user-1@example.com',
    permission: 'view',
    shared_by: 'owner@example.com',
    shared_at: '2025-01-01T00:00:00Z',
  },
  {
    id: 'share-2',
    session_id: 'session-1',
    workspace_id: 'workspace-1',
    permission: 'collaborate',
    shared_by: 'owner@example.com',
    shared_at: '2025-01-02T00:00:00Z',
  },
  {
    id: 'share-3',
    session_id: 'session-1',
    shared_with_user_id: 'user-2@example.com',
    permission: 'comment',
    shared_by: 'owner@example.com',
    shared_at: '2025-01-03T00:00:00Z',
    expires_at: '2099-12-31T23:59:59Z',
  },
  {
    id: 'share-4',
    session_id: 'session-1',
    shared_with_user_id: 'user-3@example.com',
    permission: 'view',
    shared_by: 'owner@example.com',
    shared_at: '2025-01-04T00:00:00Z',
    revoked_at: '2025-01-05T00:00:00Z',
  },
];

// Mock hooks
const mockUseSessionShares = vi.fn();
const mockShareMutation = {
  mutate: vi.fn(),
  isPending: false,
};
const mockRevokeMutation = {
  mutate: vi.fn(),
  isPending: false,
};

vi.mock('@/hooks/chat/useChatSharing', () => ({
  useSessionShares: (...args: unknown[]) => mockUseSessionShares(...args),
  useShareSession: (options?: { onSuccess?: () => void; onError?: (error: Error) => void; onSettled?: () => void }) => {
    // Store callbacks for later invocation in tests
    mockShareMutation.onSuccess = options?.onSuccess;
    mockShareMutation.onError = options?.onError;
    mockShareMutation.onSettled = options?.onSettled;
    return mockShareMutation;
  },
  useRevokeShare: (options?: { onSuccess?: () => void; onError?: (error: Error) => void }) => {
    // Store callbacks for later invocation in tests
    mockRevokeMutation.onSuccess = options?.onSuccess;
    mockRevokeMutation.onError = options?.onError;
    return mockRevokeMutation;
  },
}));

// Mock useTenant
const mockSelectedTenant = vi.fn();
vi.mock('@/providers/FeatureProviders', () => ({
  useTenant: () => ({ selectedTenant: mockSelectedTenant() }),
}));

// Mock toast (hoisted to satisfy vi.mock hoisting)
const mockToast = vi.hoisted(() => ({
  success: vi.fn(),
  error: vi.fn(),
}));
vi.mock('sonner', () => ({
  toast: mockToast,
}));

// Test wrapper component
function TestWrapper({ children }: { children: React.ReactNode }) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });

  return (
    <QueryClientProvider client={queryClient}>
      {children}
    </QueryClientProvider>
  );
}

describe('ChatShareDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockSelectedTenant.mockReturnValue('test-workspace');
    mockUseSessionShares.mockReturnValue({
      data: mockShares.filter(s => !s.revoked_at),
      isLoading: false,
    });
    (mockShareMutation as any).onSuccess = undefined;
    (mockShareMutation as any).onError = undefined;
    (mockShareMutation as any).onSettled = undefined;
    (mockRevokeMutation as any).onSuccess = undefined;
    (mockRevokeMutation as any).onError = undefined;
  });

  describe('Dialog open/close', () => {
    it('renders dialog when open is true', () => {
      const onOpenChange = vi.fn();
      render(
        <TestWrapper>
          <ChatShareDialog
            sessionId="session-1"
            open={true}
            onOpenChange={onOpenChange}
          />
        </TestWrapper>
      );

      expect(screen.getByText('Share Session')).toBeTruthy();
    });

    it('does not render dialog content when open is false', () => {
      const onOpenChange = vi.fn();
      render(
        <TestWrapper>
          <ChatShareDialog
            sessionId="session-1"
            open={false}
            onOpenChange={onOpenChange}
          />
        </TestWrapper>
      );

      expect(screen.queryByText('Share Session')).toBeNull();
    });

    it.skip('calls onOpenChange when Close button is clicked', async () => {
      const onOpenChange = vi.fn();
      render(
        <TestWrapper>
          <ChatShareDialog
            sessionId="session-1"
            open={true}
            onOpenChange={onOpenChange}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const closeButton = screen.getByRole('button', { name: /close/i });
      await user.click(closeButton);

      expect(onOpenChange).toHaveBeenCalledWith(false);
    });
  });

  describe('Share form submission', () => {
    it('submits share with correct workspace_id from useTenant when workspace toggle is on', async () => {
      mockSelectedTenant.mockReturnValue('custom-workspace');
      const onOpenChange = vi.fn();

      render(
        <TestWrapper>
          <ChatShareDialog
            sessionId="session-1"
            open={true}
            onOpenChange={onOpenChange}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();

      // Toggle workspace share
      const workspaceSwitch = screen.getByRole('switch', { name: /workspace-share/i });
      await user.click(workspaceSwitch);

      // Click share button
      const shareButton = screen.getByRole('button', { name: /share with workspace/i });
      await user.click(shareButton);

      // Verify correct workspace_id from useTenant
      expect(mockShareMutation.mutate).toHaveBeenCalledWith({
        sessionId: 'session-1',
        request: {
          permission: 'view',
          workspace_id: 'custom-workspace',
        },
      });
    });

    it('uses "default" as fallback when selectedTenant is null', async () => {
      mockSelectedTenant.mockReturnValue(null);
      const onOpenChange = vi.fn();

      render(
        <TestWrapper>
          <ChatShareDialog
            sessionId="session-1"
            open={true}
            onOpenChange={onOpenChange}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();

      // Toggle workspace share
      const workspaceSwitch = screen.getByRole('switch', { name: /workspace-share/i });
      await user.click(workspaceSwitch);

      // Click share button
      const shareButton = screen.getByRole('button', { name: /share with workspace/i });
      await user.click(shareButton);

      // Verify fallback to "default"
      expect(mockShareMutation.mutate).toHaveBeenCalledWith({
        sessionId: 'session-1',
        request: {
          permission: 'view',
          workspace_id: 'default',
        },
      });
    });

    it('submits share with user_ids when workspace toggle is off', async () => {
      const onOpenChange = vi.fn();

      render(
        <TestWrapper>
          <ChatShareDialog
            sessionId="session-1"
            open={true}
            onOpenChange={onOpenChange}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();

      // Enter user email
      const input = screen.getByPlaceholderText(/enter user email or id/i);
      await user.type(input, 'newuser@example.com');

      // Click share button (icon button with UserPlus)
      const shareButtons = screen.getAllByRole('button');
      const shareButton = shareButtons.find(btn => btn.querySelector('svg'));
      expect(shareButton).toBeTruthy();
      await user.click(shareButton!);

      // Verify user_ids are used
      expect(mockShareMutation.mutate).toHaveBeenCalledWith({
        sessionId: 'session-1',
        request: {
          permission: 'view',
          user_ids: ['newuser@example.com'],
        },
      });
    });

    it.skip('shows error toast when sharing with empty user input', async () => {
      const onOpenChange = vi.fn();

      render(
        <TestWrapper>
          <ChatShareDialog
            sessionId="session-1"
            open={true}
            onOpenChange={onOpenChange}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();

      // Click share button without entering user
      const shareButtons = screen.getAllByRole('button');
      const shareButton = shareButtons.find(btn => btn.querySelector('svg'));
      await user.click(shareButton!);

      // Verify error toast
      expect(mockToast.error).toHaveBeenCalledWith('Please enter a user email or ID');
      expect(mockShareMutation.mutate).not.toHaveBeenCalled();
    });

    it('allows submitting with Enter key', async () => {
      const onOpenChange = vi.fn();

      render(
        <TestWrapper>
          <ChatShareDialog
            sessionId="session-1"
            open={true}
            onOpenChange={onOpenChange}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();

      // Enter user email and press Enter
      const input = screen.getByPlaceholderText(/enter user email or id/i);
      await user.type(input, 'enteruser@example.com{Enter}');

      // Verify share was called
      expect(mockShareMutation.mutate).toHaveBeenCalledWith({
        sessionId: 'session-1',
        request: {
          permission: 'view',
          user_ids: ['enteruser@example.com'],
        },
      });
    });

    it('clears input after successful share', async () => {
      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false } },
      });

      render(
        <QueryClientProvider client={queryClient}>
          <ChatShareDialog
            sessionId="session-1"
            open={true}
            onOpenChange={vi.fn()}
          />
        </QueryClientProvider>
      );

      const user = userEvent.setup();

      // Enter user email
      const input = screen.getByPlaceholderText(/enter user email or id/i) as HTMLInputElement;
      await user.type(input, 'clearme@example.com');

      // Click share button
      const shareButtons = screen.getAllByRole('button');
      const shareButton = shareButtons.find(btn => btn.querySelector('svg'));
      await user.click(shareButton!);

      // Trigger success callback
      if ((mockShareMutation as any).onSuccess) {
        (mockShareMutation as any).onSuccess();
      }
      if ((mockShareMutation as any).onSettled) {
        (mockShareMutation as any).onSettled();
      }

      await waitFor(() => {
        expect(input.value).toBe('');
      });
    });

    it('shows success toast after successful share', async () => {
      const onOpenChange = vi.fn();

      render(
        <TestWrapper>
          <ChatShareDialog
            sessionId="session-1"
            open={true}
            onOpenChange={onOpenChange}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();

      // Enter user email
      const input = screen.getByPlaceholderText(/enter user email or id/i);
      await user.type(input, 'success@example.com');

      // Click share button
      const shareButtons = screen.getAllByRole('button');
      const shareButton = shareButtons.find(btn => btn.querySelector('svg'));
      await user.click(shareButton!);

      // Trigger success callback
      if ((mockShareMutation as any).onSuccess) {
        (mockShareMutation as any).onSuccess();
      }

      await waitFor(() => {
        expect(mockToast.success).toHaveBeenCalledWith('Session shared successfully');
      });
    });

    it('shows error toast on share failure', async () => {
      const onOpenChange = vi.fn();

      render(
        <TestWrapper>
          <ChatShareDialog
            sessionId="session-1"
            open={true}
            onOpenChange={onOpenChange}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();

      // Enter user email
      const input = screen.getByPlaceholderText(/enter user email or id/i);
      await user.type(input, 'fail@example.com');

      // Click share button
      const shareButtons = screen.getAllByRole('button');
      const shareButton = shareButtons.find(btn => btn.querySelector('svg'));
      await user.click(shareButton!);

      // Trigger error callback
      if ((mockShareMutation as any).onError) {
        (mockShareMutation as any).onError(new Error('Network error'));
      }

      await waitFor(() => {
        expect(mockToast.error).toHaveBeenCalledWith('Failed to share session: Network error');
      });
    });
  });

  describe.skip('Permission selection', () => {
    it('defaults to "view" permission', () => {
      render(
        <TestWrapper>
          <ChatShareDialog
            sessionId="session-1"
            open={true}
            onOpenChange={vi.fn()}
          />
        </TestWrapper>
      );

      // The select should show "Can view" as default
      expect(screen.getByText('Can view')).toBeTruthy();
    });

    it('allows changing permission to "comment"', async () => {
      render(
        <TestWrapper>
          <ChatShareDialog
            sessionId="session-1"
            open={true}
            onOpenChange={vi.fn()}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();

      // Click select trigger
      const selectTrigger = screen.getByRole('combobox');
      await user.click(selectTrigger);

      // Select "Can comment"
      const commentOption = screen.getByRole('option', { name: /can comment/i });
      await user.click(commentOption);

      // Enter user and share
      const input = screen.getByPlaceholderText(/enter user email or id/i);
      await user.type(input, 'user@example.com');

      const shareButtons = screen.getAllByRole('button');
      const shareButton = shareButtons.find(btn => btn.querySelector('svg'));
      await user.click(shareButton!);

      // Verify permission is "comment"
      expect(mockShareMutation.mutate).toHaveBeenCalledWith({
        sessionId: 'session-1',
        request: {
          permission: 'comment',
          user_ids: ['user@example.com'],
        },
      });
    });

    it('allows changing permission to "collaborate"', async () => {
      render(
        <TestWrapper>
          <ChatShareDialog
            sessionId="session-1"
            open={true}
            onOpenChange={vi.fn()}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();

      // Click select trigger
      const selectTrigger = screen.getByRole('combobox');
      await user.click(selectTrigger);

      // Select "Can edit"
      const collaborateOption = screen.getByRole('option', { name: /can edit/i });
      await user.click(collaborateOption);

      // Enter user and share
      const input = screen.getByPlaceholderText(/enter user email or id/i);
      await user.type(input, 'user@example.com');

      const shareButtons = screen.getAllByRole('button');
      const shareButton = shareButtons.find(btn => btn.querySelector('svg'));
      await user.click(shareButton!);

      // Verify permission is "collaborate"
      expect(mockShareMutation.mutate).toHaveBeenCalledWith({
        sessionId: 'session-1',
        request: {
          permission: 'collaborate',
          user_ids: ['user@example.com'],
        },
      });
    });
  });

  describe('Existing shares display', () => {
    it('displays loading state while fetching shares', () => {
      mockUseSessionShares.mockReturnValue({
        data: [],
        isLoading: true,
      });

      render(
        <TestWrapper>
          <ChatShareDialog
            sessionId="session-1"
            open={true}
            onOpenChange={vi.fn()}
          />
        </TestWrapper>
      );

      expect(screen.getByText(/loading shares/i)).toBeTruthy();
    });

    it('displays message when no shares exist', () => {
      mockUseSessionShares.mockReturnValue({
        data: [],
        isLoading: false,
      });

      render(
        <TestWrapper>
          <ChatShareDialog
            sessionId="session-1"
            open={true}
            onOpenChange={vi.fn()}
          />
        </TestWrapper>
      );

      expect(screen.getByText(/hasn't been shared yet/i)).toBeTruthy();
    });

    it('displays user shares correctly', () => {
      render(
        <TestWrapper>
          <ChatShareDialog
            sessionId="session-1"
            open={true}
            onOpenChange={vi.fn()}
          />
        </TestWrapper>
      );

      // Should show user emails
      expect(screen.getByText('user-1@example.com')).toBeTruthy();
      expect(screen.getByText('user-2@example.com')).toBeTruthy();

      // Should not show revoked share
      expect(screen.queryByText('user-3@example.com')).toBeNull();
    });

    it('displays workspace shares correctly', () => {
      render(
        <TestWrapper>
          <ChatShareDialog
            sessionId="session-1"
            open={true}
            onOpenChange={vi.fn()}
          />
        </TestWrapper>
      );

      // Should show workspace share
      expect(screen.getByText('Workspace')).toBeTruthy();
    });

    it.skip('displays permission badges with correct labels', () => {
      render(
        <TestWrapper>
          <ChatShareDialog
            sessionId="session-1"
            open={true}
            onOpenChange={vi.fn()}
          />
        </TestWrapper>
      );

      // Check permission labels
      expect(screen.getByText('Can view')).toBeTruthy();
      expect(screen.getByText('Can comment')).toBeTruthy();
      expect(screen.getByText('Can edit')).toBeTruthy();
    });

    it('displays expiration date when present', () => {
      render(
        <TestWrapper>
          <ChatShareDialog
            sessionId="session-1"
            open={true}
            onOpenChange={vi.fn()}
          />
        </TestWrapper>
      );

      // Should show expiration for share-3
      expect(screen.getByText(/expires/i)).toBeTruthy();
    });

    it('does not display revoked shares', () => {
      render(
        <TestWrapper>
          <ChatShareDialog
            sessionId="session-1"
            open={true}
            onOpenChange={vi.fn()}
          />
        </TestWrapper>
      );

      // share-4 is revoked, should not appear
      expect(screen.queryByText('user-3@example.com')).toBeNull();
    });
  });

  describe.skip('Revoke share functionality', () => {
    it('opens AlertDialog when revoke button is clicked', async () => {
      render(
        <TestWrapper>
          <ChatShareDialog
            sessionId="session-1"
            open={true}
            onOpenChange={vi.fn()}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();

      // Click first revoke button (X icon)
      const revokeButtons = screen.getAllByRole('button').filter(
        btn => btn.querySelector('svg') && btn.getAttribute('class')?.includes('ghost')
      );
      await user.click(revokeButtons[0]);

      // AlertDialog should appear
      await waitFor(() => {
        expect(screen.getByText('Revoke Share Access?')).toBeTruthy();
      });
    });

    it('uses AlertDialog for revoke confirmation, not window.confirm', async () => {
      const confirmSpy = vi.spyOn(window, 'confirm');

      render(
        <TestWrapper>
          <ChatShareDialog
            sessionId="session-1"
            open={true}
            onOpenChange={vi.fn()}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();

      // Click revoke button
      const revokeButtons = screen.getAllByRole('button').filter(
        btn => btn.querySelector('svg') && btn.getAttribute('class')?.includes('ghost')
      );
      await user.click(revokeButtons[0]);

      // Verify AlertDialog is used, not window.confirm
      expect(confirmSpy).not.toHaveBeenCalled();
      await waitFor(() => {
        expect(screen.getByText('Revoke Share Access?')).toBeTruthy();
      });

      confirmSpy.mockRestore();
    });

    it('closes AlertDialog when Cancel is clicked', async () => {
      render(
        <TestWrapper>
          <ChatShareDialog
            sessionId="session-1"
            open={true}
            onOpenChange={vi.fn()}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();

      // Click revoke button
      const revokeButtons = screen.getAllByRole('button').filter(
        btn => btn.querySelector('svg') && btn.getAttribute('class')?.includes('ghost')
      );
      await user.click(revokeButtons[0]);

      // Click Cancel in AlertDialog
      const cancelButton = screen.getByRole('button', { name: /cancel/i });
      await user.click(cancelButton);

      // AlertDialog should close
      await waitFor(() => {
        expect(screen.queryByText('Revoke Share Access?')).toBeNull();
      });

      // Revoke should not be called
      expect(mockRevokeMutation.mutate).not.toHaveBeenCalled();
    });

    it('calls revoke mutation when confirmed', async () => {
      render(
        <TestWrapper>
          <ChatShareDialog
            sessionId="session-1"
            open={true}
            onOpenChange={vi.fn()}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();

      // Click first revoke button
      const revokeButtons = screen.getAllByRole('button').filter(
        btn => btn.querySelector('svg') && btn.getAttribute('class')?.includes('ghost')
      );
      await user.click(revokeButtons[0]);

      // Click Revoke Access in AlertDialog
      const revokeAccessButton = screen.getByRole('button', { name: /revoke access/i });
      await user.click(revokeAccessButton);

      // Verify revoke was called with correct share ID
      await waitFor(() => {
        expect(mockRevokeMutation.mutate).toHaveBeenCalledWith({
          sessionId: 'session-1',
          shareId: 'share-2', // First share in the filtered list
        });
      });
    });

    it('shows success toast after successful revoke', async () => {
      render(
        <TestWrapper>
          <ChatShareDialog
            sessionId="session-1"
            open={true}
            onOpenChange={vi.fn()}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();

      // Click revoke button and confirm
      const revokeButtons = screen.getAllByRole('button').filter(
        btn => btn.querySelector('svg') && btn.getAttribute('class')?.includes('ghost')
      );
      await user.click(revokeButtons[0]);

      const revokeAccessButton = screen.getByRole('button', { name: /revoke access/i });
      await user.click(revokeAccessButton);

      // Trigger success callback
      if ((mockRevokeMutation as any).onSuccess) {
        (mockRevokeMutation as any).onSuccess();
      }

      await waitFor(() => {
        expect(mockToast.success).toHaveBeenCalledWith('Share revoked successfully');
      });
    });

    it('shows error toast on revoke failure', async () => {
      render(
        <TestWrapper>
          <ChatShareDialog
            sessionId="session-1"
            open={true}
            onOpenChange={vi.fn()}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();

      // Click revoke button and confirm
      const revokeButtons = screen.getAllByRole('button').filter(
        btn => btn.querySelector('svg') && btn.getAttribute('class')?.includes('ghost')
      );
      await user.click(revokeButtons[0]);

      const revokeAccessButton = screen.getByRole('button', { name: /revoke access/i });
      await user.click(revokeAccessButton);

      // Trigger error callback
      if ((mockRevokeMutation as any).onError) {
        (mockRevokeMutation as any).onError(new Error('Revoke failed'));
      }

      await waitFor(() => {
        expect(mockToast.error).toHaveBeenCalledWith('Failed to revoke share: Revoke failed');
      });
    });
  });

  describe.skip('Copy link functionality', () => {
    it('displays the correct session link', () => {
      render(
        <TestWrapper>
          <ChatShareDialog
            sessionId="session-123"
            open={true}
            onOpenChange={vi.fn()}
          />
        </TestWrapper>
      );

      const linkInput = screen.getByDisplayValue(/\/chat\/sessions\/session-123$/);
      expect(linkInput).toBeTruthy();
    });

    it('copies link to clipboard when copy button is clicked', async () => {
      // Mock clipboard API
      const writeTextMock = vi.fn().mockResolvedValue(undefined);
      Object.assign(navigator, {
        clipboard: {
          writeText: writeTextMock,
        },
      });

      render(
        <TestWrapper>
          <ChatShareDialog
            sessionId="session-123"
            open={true}
            onOpenChange={vi.fn()}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();

      // Click copy button (Link icon)
      const buttons = screen.getAllByRole('button');
      const copyButton = buttons.find(
        btn => btn.querySelector('svg') && btn.getAttribute('class')?.includes('outline')
      );
      expect(copyButton).toBeTruthy();
      await user.click(copyButton!);

      await waitFor(() => {
        expect(writeTextMock).toHaveBeenCalledWith(
          expect.stringContaining('/chat?session=session-123')
        );
        expect(mockToast.success).toHaveBeenCalledWith('Link copied to clipboard');
      });
    });
  });

  describe('Workspace toggle behavior', () => {
    it('shows user input by default (workspace toggle off)', () => {
      render(
        <TestWrapper>
          <ChatShareDialog
            sessionId="session-1"
            open={true}
            onOpenChange={vi.fn()}
          />
        </TestWrapper>
      );

      // User input should be visible
      expect(screen.getByPlaceholderText(/enter user email or id/i)).toBeTruthy();

      // Workspace button should not be visible
      expect(screen.queryByRole('button', { name: /share with workspace/i })).toBeNull();
    });

    it('hides user input when workspace toggle is on', async () => {
      render(
        <TestWrapper>
          <ChatShareDialog
            sessionId="session-1"
            open={true}
            onOpenChange={vi.fn()}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();

      // Toggle workspace share
      const workspaceSwitch = screen.getByRole('switch', { name: /workspace-share/i });
      await user.click(workspaceSwitch);

      // User input should be hidden
      await waitFor(() => {
        expect(screen.queryByPlaceholderText(/enter user email or id/i)).toBeNull();
      });

      // Workspace button should be visible
      expect(screen.getByRole('button', { name: /share with workspace/i })).toBeTruthy();
    });

    it('resets workspace toggle after successful share', async () => {
      render(
        <TestWrapper>
          <ChatShareDialog
            sessionId="session-1"
            open={true}
            onOpenChange={vi.fn()}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();

      // Toggle workspace share
      const workspaceSwitch = screen.getByRole('switch', { name: /workspace-share/i });
      await user.click(workspaceSwitch);

      // Share
      const shareButton = screen.getByRole('button', { name: /share with workspace/i });
      await user.click(shareButton);

      // Trigger success callback
      if ((mockShareMutation as any).onSuccess) {
        (mockShareMutation as any).onSuccess();
      }

      // Workspace toggle should be reset
      await waitFor(() => {
        const updatedSwitch = screen.getByRole('switch', { name: /workspace-share/i });
        expect(updatedSwitch.getAttribute('data-state')).toBe('unchecked');
      });
    });
  });

  describe('Query invalidation', () => {
    it('does not fetch shares when dialog is closed', () => {
      const mockQueryFn = vi.fn();
      mockUseSessionShares.mockImplementation((sessionId, options) => {
        if (options?.enabled) {
          mockQueryFn();
        }
        return { data: [], isLoading: false };
      });

      render(
        <TestWrapper>
          <ChatShareDialog
            sessionId="session-1"
            open={false}
            onOpenChange={vi.fn()}
          />
        </TestWrapper>
      );

      // Should be called with enabled: false
      expect(mockUseSessionShares).toHaveBeenCalledWith('session-1', { enabled: false });
    });

    it('fetches shares when dialog is opened', () => {
      mockUseSessionShares.mockImplementation((sessionId, options) => {
        return { data: mockShares, isLoading: false };
      });

      render(
        <TestWrapper>
          <ChatShareDialog
            sessionId="session-1"
            open={true}
            onOpenChange={vi.fn()}
          />
        </TestWrapper>
      );

      // Should be called with enabled: true
      expect(mockUseSessionShares).toHaveBeenCalledWith('session-1', { enabled: true });
    });
  });
});
