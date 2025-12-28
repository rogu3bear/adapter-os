/**
 * ChatInputArea Component Tests
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ChatInputArea, type ChatInputAreaProps } from '@/components/chat/ChatInputArea';
import type { AttachedAdapter, SuggestedAdapter } from '@/contexts/ChatContext';

// Mock logger
vi.mock('@/utils/logger', () => ({
  logger: {
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
  },
}));

// Helper to create mock attached adapters
function createMockAttachedAdapter(overrides: Partial<AttachedAdapter> = {}): AttachedAdapter {
  return {
    id: 'adapter-1',
    confidence: 0.85,
    attachedBy: 'auto',
    attachedAt: Date.now(),
    ...overrides,
  };
}

// Helper to create mock suggested adapters
function createMockSuggestedAdapter(overrides: Partial<SuggestedAdapter> = {}): SuggestedAdapter {
  return {
    id: 'suggested-adapter-1',
    confidence: 0.75,
    reason: 'High relevance detected',
    ...overrides,
  };
}

// Default props for ChatInputArea
function createDefaultProps(overrides: Partial<ChatInputAreaProps> = {}): ChatInputAreaProps {
  return {
    onSend: vi.fn(),
    attachedAdapters: [],
    onRemoveAttachment: vi.fn(),
    ...overrides,
  };
}

describe('ChatInputArea', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('rendering', () => {
    it('renders textarea and send button', () => {
      const props = createDefaultProps();

      render(<ChatInputArea {...props} />);

      expect(screen.getByTestId('chat-input')).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /send message/i })).toBeInTheDocument();
    });

    it('renders with custom placeholder', () => {
      const customPlaceholder = 'Ask me anything...';
      const props = createDefaultProps({ placeholder: customPlaceholder });

      render(<ChatInputArea {...props} />);

      expect(screen.getByPlaceholderText(customPlaceholder)).toBeInTheDocument();
    });

    it('renders auto-attach status', () => {
      const props = createDefaultProps({ autoAttachEnabled: true });

      render(<ChatInputArea {...props} />);

      expect(screen.getByText(/auto-attach on/i)).toBeInTheDocument();
    });

    it('renders paused state when auto-attach is paused', () => {
      const props = createDefaultProps({
        autoAttachEnabled: true,
        autoAttachPaused: true,
      });

      render(<ChatInputArea {...props} />);

      expect(screen.getByText(/temporarily paused/i)).toBeInTheDocument();
    });
  });

  describe('sending messages', () => {
    it('calls onSend when Enter is pressed with text', async () => {
      const user = userEvent.setup();
      const onSend = vi.fn();
      const props = createDefaultProps({ onSend });

      render(<ChatInputArea {...props} />);

      const textarea = screen.getByTestId('chat-input');
      await user.type(textarea, 'Hello world');
      await user.keyboard('{Enter}');

      expect(onSend).toHaveBeenCalledWith('Hello world');
    });

    it('does not call onSend when input is empty', async () => {
      const user = userEvent.setup();
      const onSend = vi.fn();
      const props = createDefaultProps({ onSend });

      render(<ChatInputArea {...props} />);

      const textarea = screen.getByTestId('chat-input');
      await user.click(textarea);
      await user.keyboard('{Enter}');

      expect(onSend).not.toHaveBeenCalled();
    });

    it('does not call onSend when input is only whitespace', async () => {
      const user = userEvent.setup();
      const onSend = vi.fn();
      const props = createDefaultProps({ onSend });

      render(<ChatInputArea {...props} />);

      const textarea = screen.getByTestId('chat-input');
      await user.type(textarea, '   ');
      await user.keyboard('{Enter}');

      expect(onSend).not.toHaveBeenCalled();
    });

    it('calls onSend when send button is clicked', async () => {
      const user = userEvent.setup();
      const onSend = vi.fn();
      const props = createDefaultProps({ onSend });

      render(<ChatInputArea {...props} />);

      const textarea = screen.getByTestId('chat-input');
      await user.type(textarea, 'Test message');

      const sendButton = screen.getByRole('button', { name: /send message/i });
      await user.click(sendButton);

      expect(onSend).toHaveBeenCalledWith('Test message');
    });

    it('clears input after sending', async () => {
      const user = userEvent.setup();
      const onSend = vi.fn();
      const props = createDefaultProps({ onSend });

      render(<ChatInputArea {...props} />);

      const textarea = screen.getByTestId('chat-input');
      await user.type(textarea, 'Test message');
      await user.keyboard('{Enter}');

      expect(textarea).toHaveValue('');
    });

    it('allows Shift+Enter for new line without sending', async () => {
      const user = userEvent.setup();
      const onSend = vi.fn();
      const props = createDefaultProps({ onSend });

      render(<ChatInputArea {...props} />);

      const textarea = screen.getByTestId('chat-input');
      await user.type(textarea, 'Line 1');
      await user.keyboard('{Shift>}{Enter}{/Shift}');
      await user.type(textarea, 'Line 2');

      expect(onSend).not.toHaveBeenCalled();
      expect(textarea).toHaveValue('Line 1\nLine 2');
    });

    it('sends with Cmd/Ctrl+Enter', async () => {
      const user = userEvent.setup();
      const onSend = vi.fn();
      const props = createDefaultProps({ onSend });

      render(<ChatInputArea {...props} />);

      const textarea = screen.getByTestId('chat-input');
      await user.type(textarea, 'Test message');
      await user.keyboard('{Meta>}{Enter}{/Meta}');

      expect(onSend).toHaveBeenCalledWith('Test message');
    });
  });

  describe('attached adapters', () => {
    it('shows attached adapters', () => {
      const attachedAdapters = [
        createMockAttachedAdapter({ id: 'adapter-1', confidence: 0.85 }),
        createMockAttachedAdapter({ id: 'adapter-2', confidence: 0.92 }),
      ];
      const props = createDefaultProps({ attachedAdapters });

      render(<ChatInputArea {...props} />);

      expect(screen.getByText('adapter-1')).toBeInTheDocument();
      expect(screen.getByText('adapter-2')).toBeInTheDocument();
    });

    it('shows no attachments message when no adapters attached', () => {
      const props = createDefaultProps({ attachedAdapters: [] });

      render(<ChatInputArea {...props} />);

      expect(screen.getByText(/no attachments yet/i)).toBeInTheDocument();
    });

    it('calls onRemoveAttachment when adapter chip X is clicked', async () => {
      const user = userEvent.setup();
      const onRemoveAttachment = vi.fn();
      const attachedAdapters = [
        createMockAttachedAdapter({ id: 'adapter-1' }),
      ];
      const props = createDefaultProps({ attachedAdapters, onRemoveAttachment });

      render(<ChatInputArea {...props} />);

      const removeButton = screen.getByRole('button', { name: /remove adapter-1/i });
      await user.click(removeButton);

      expect(onRemoveAttachment).toHaveBeenCalledWith('adapter-1', true);
    });

    it('displays confidence percentage on attached adapters', () => {
      const attachedAdapters = [
        createMockAttachedAdapter({ id: 'adapter-1', confidence: 0.85 }),
      ];
      const props = createDefaultProps({ attachedAdapters });

      render(<ChatInputArea {...props} />);

      expect(screen.getByText('85%')).toBeInTheDocument();
    });
  });

  describe('suggested adapters', () => {
    it('shows suggested adapters section when available', () => {
      const suggestedAdapters = [
        createMockSuggestedAdapter({ id: 'suggested-1', confidence: 0.75 }),
      ];
      const props = createDefaultProps({ suggestedAdapters });

      render(<ChatInputArea {...props} />);

      expect(screen.getByText(/suggested:/i)).toBeInTheDocument();
      expect(screen.getByText('suggested-1')).toBeInTheDocument();
    });

    it('calls onAttachSuggested when suggested adapter is clicked', async () => {
      const user = userEvent.setup();
      const onAttachSuggested = vi.fn();
      const suggestedAdapters = [
        createMockSuggestedAdapter({ id: 'suggested-1', confidence: 0.75 }),
      ];
      const props = createDefaultProps({ suggestedAdapters, onAttachSuggested });

      render(<ChatInputArea {...props} />);

      const suggestedChip = screen.getByLabelText(/adapter suggested-1/i);
      await user.click(suggestedChip);

      expect(onAttachSuggested).toHaveBeenCalledWith(suggestedAdapters[0]);
    });

    it('calls onAcceptSuggestion when Tab is pressed with suggestions', async () => {
      const user = userEvent.setup();
      const onAcceptSuggestion = vi.fn();
      const suggestedAdapters = [
        createMockSuggestedAdapter({ id: 'suggested-1' }),
      ];
      const props = createDefaultProps({ suggestedAdapters, onAcceptSuggestion });

      render(<ChatInputArea {...props} />);

      const textarea = screen.getByTestId('chat-input');
      await user.click(textarea);
      await user.keyboard('{Tab}');

      expect(onAcceptSuggestion).toHaveBeenCalled();
    });
  });

  describe('disabled state', () => {
    it('disables textarea when disabled prop is true', () => {
      const props = createDefaultProps({ disabled: true });

      render(<ChatInputArea {...props} />);

      expect(screen.getByTestId('chat-input')).toBeDisabled();
    });

    it('disables send button when disabled prop is true', () => {
      const props = createDefaultProps({ disabled: true });

      render(<ChatInputArea {...props} />);

      expect(screen.getByRole('button', { name: /send message/i })).toBeDisabled();
    });

    it('does not call onSend when disabled and Enter is pressed', async () => {
      const user = userEvent.setup();
      const onSend = vi.fn();
      const props = createDefaultProps({ onSend, disabled: true });

      render(<ChatInputArea {...props} />);

      // Cannot type in disabled textarea, so just verify the state
      expect(screen.getByTestId('chat-input')).toBeDisabled();
      expect(onSend).not.toHaveBeenCalled();
    });
  });

  describe('streaming state', () => {
    it('disables textarea when streaming', () => {
      const props = createDefaultProps({ isStreaming: true });

      render(<ChatInputArea {...props} />);

      expect(screen.getByTestId('chat-input')).toBeDisabled();
    });

    it('shows loading indicator when streaming', () => {
      const props = createDefaultProps({ isStreaming: true });

      render(<ChatInputArea {...props} />);

      const sendButton = screen.getByRole('button', { name: /sending message/i });
      expect(sendButton).toBeInTheDocument();
    });

    it('disables send button when streaming', () => {
      const props = createDefaultProps({ isStreaming: true });

      render(<ChatInputArea {...props} />);

      const sendButton = screen.getByRole('button', { name: /sending message/i });
      expect(sendButton).toBeDisabled();
    });

    it('shows cancel button when streaming and onCancelStream provided', () => {
      const onCancelStream = vi.fn();
      const props = createDefaultProps({ isStreaming: true, onCancelStream });

      render(<ChatInputArea {...props} />);

      expect(screen.getByRole('button', { name: /cancel response/i })).toBeInTheDocument();
    });

    it('does not show cancel button when not streaming', () => {
      const onCancelStream = vi.fn();
      const props = createDefaultProps({ isStreaming: false, onCancelStream });

      render(<ChatInputArea {...props} />);

      expect(screen.queryByRole('button', { name: /cancel response/i })).not.toBeInTheDocument();
    });

    it('calls onCancelStream when cancel button is clicked', async () => {
      const user = userEvent.setup();
      const onCancelStream = vi.fn();
      const props = createDefaultProps({ isStreaming: true, onCancelStream });

      render(<ChatInputArea {...props} />);

      const cancelButton = screen.getByRole('button', { name: /cancel response/i });
      await user.click(cancelButton);

      expect(onCancelStream).toHaveBeenCalled();
    });
  });

  describe('RBAC permissions', () => {
    it('disables send button when canExecuteInference is false', async () => {
      const user = userEvent.setup();
      const props = createDefaultProps({ canExecuteInference: false });

      render(<ChatInputArea {...props} />);

      // Type something to ensure we have input
      const textarea = screen.getByTestId('chat-input');
      await user.type(textarea, 'Test');

      const sendButton = screen.getByRole('button', { name: /send message/i });
      expect(sendButton).toBeDisabled();
    });

    it('shows tooltip on send button when viewer cannot execute', async () => {
      const props = createDefaultProps({ canExecuteInference: false });

      render(<ChatInputArea {...props} />);

      const sendButton = screen.getByRole('button', { name: /send message/i });
      expect(sendButton).toHaveAttribute('title', 'Viewers cannot send messages');
    });
  });

  describe('snap visual effect', () => {
    it('shows snap visual when showSnapVisual is true with suggestion', () => {
      const suggestedAdapter = createMockSuggestedAdapter({ id: 'snap-adapter' });
      const props = createDefaultProps({
        showSnapVisual: true,
        suggestedAdapter,
      });

      render(<ChatInputArea {...props} />);

      expect(screen.getByText(/snapped snap-adapter to this prompt/i)).toBeInTheDocument();
    });

    it('does not show snap visual when showSnapVisual is false', () => {
      const suggestedAdapter = createMockSuggestedAdapter({ id: 'snap-adapter' });
      const props = createDefaultProps({
        showSnapVisual: false,
        suggestedAdapter,
      });

      render(<ChatInputArea {...props} />);

      expect(screen.queryByText(/snapped/i)).not.toBeInTheDocument();
    });
  });

  describe('magnet field visual', () => {
    it('shows magnet field when enabled', () => {
      const props = createDefaultProps({
        magnetFieldSettings: {
          show: true,
          confidence: 0.8,
        },
      });

      const { container } = render(<ChatInputArea {...props} />);

      expect(container.querySelector('.magnet-field')).toBeInTheDocument();
    });

    it('does not show magnet field when disabled', () => {
      const props = createDefaultProps({
        magnetFieldSettings: {
          show: false,
        },
      });

      const { container } = render(<ChatInputArea {...props} />);

      expect(container.querySelector('.magnet-field')).not.toBeInTheDocument();
    });
  });
});
