import React, { useState, useRef, useCallback, useEffect } from 'react';
import { Send, Loader2, X, Link2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import { cn } from '@/lib/utils';
import { AdapterAttachmentChip } from './AdapterAttachmentChip';
import type { SuggestedAdapter, AttachedAdapter } from '@/contexts/ChatContext';

export interface ChatInputAreaProps {
  /** Callback when the user sends a message */
  onSend: (message: string) => void;
  /** Whether the input should be disabled (e.g., model not ready) */
  disabled?: boolean;
  /** Whether a message is currently streaming */
  isStreaming?: boolean;
  /** Currently attached adapters */
  attachedAdapters: AttachedAdapter[];
  /** Suggested adapter to display (optional) */
  suggestedAdapter?: SuggestedAdapter | null;
  /** List of suggested adapters to display */
  suggestedAdapters?: SuggestedAdapter[];
  /** ID of the last attached adapter (for flash animation) */
  lastAttachedAdapterId?: string | null;
  /** Whether snap visual effect should be shown */
  showSnapVisual?: boolean;
  /** Callback to remove an attached adapter */
  onRemoveAttachment: (adapterId: string, mute?: boolean) => void;
  /** Callback to accept the suggested adapter */
  onAcceptSuggestion?: () => void;
  /** Callback to dismiss/mute the suggested adapter */
  onDismissSuggestion?: (adapterId?: string) => void;
  /** Callback when a suggested adapter chip is clicked to attach */
  onAttachSuggested?: (adapter: SuggestedAdapter) => void;
  /** Callback to cancel the current stream */
  onCancelStream?: () => void;
  /** Whether auto-attach is enabled */
  autoAttachEnabled?: boolean;
  /** Whether auto-attach is paused */
  autoAttachPaused?: boolean;
  /** Placeholder text for the textarea */
  placeholder?: string;
  /** Additional className for the container */
  className?: string;
  /** Whether the user can execute inference (RBAC) */
  canExecuteInference?: boolean;
  /** Magnet field visual settings */
  magnetFieldSettings?: {
    show: boolean;
    glowStyle?: React.CSSProperties;
    confidence?: number;
  };
}

/**
 * ChatInputArea component handles the message input area of the chat interface.
 * It manages internal input state, keyboard shortcuts, and displays attached/suggested adapters.
 */
export function ChatInputArea({
  onSend,
  disabled = false,
  isStreaming = false,
  attachedAdapters,
  suggestedAdapter,
  suggestedAdapters = [],
  lastAttachedAdapterId,
  showSnapVisual = false,
  onRemoveAttachment,
  onAcceptSuggestion,
  onDismissSuggestion,
  onAttachSuggested,
  onCancelStream,
  autoAttachEnabled = false,
  autoAttachPaused = false,
  placeholder = 'Type your message... (Enter to send, Shift+Enter for new line)',
  className,
  canExecuteInference = true,
  magnetFieldSettings,
}: ChatInputAreaProps) {
  const [input, setInput] = useState('');
  const inputRef = useRef<HTMLTextAreaElement>(null);

  // Focus input on mount
  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const handleSend = useCallback(() => {
    const trimmedInput = input.trim();
    if (!trimmedInput || isStreaming || disabled) return;

    onSend(trimmedInput);
    setInput('');
  }, [input, isStreaming, disabled, onSend]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      // Tab to accept suggestion
      if (e.key === 'Tab' && (suggestedAdapters.length > 0 || suggestedAdapter)) {
        e.preventDefault();
        onAcceptSuggestion?.();
        return;
      }

      // Enter to send (Shift+Enter for new line)
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        handleSend();
      }

      // Cmd/Ctrl+Enter to send (alternative shortcut)
      if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        handleSend();
      }
    },
    [suggestedAdapters.length, suggestedAdapter, onAcceptSuggestion, handleSend]
  );

  const handleInputChange = useCallback((e: React.ChangeEvent<HTMLTextAreaElement>) => {
    setInput(e.target.value);
  }, []);

  const isInputDisabled = isStreaming || disabled;
  const isSendDisabled = isStreaming || !input.trim() || disabled || !canExecuteInference;

  const showMagnetField = magnetFieldSettings?.show ?? false;
  const magnetGlowStyle = magnetFieldSettings?.glowStyle;
  const magnetConfidence = magnetFieldSettings?.confidence ?? 0;

  const activeSuggestion = suggestedAdapter ?? suggestedAdapters[0] ?? null;

  return (
    <div className={cn('flex flex-col gap-2', className)}>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          handleSend();
        }}
        className="flex gap-2 items-start"
        aria-label="Chat message input"
      >
        <div className="flex-1 flex flex-col gap-2">
          {/* Textarea with magnet field effect */}
          <div className="relative">
            {showMagnetField && (
              <div
                className="pointer-events-none absolute inset-0 rounded-lg magnet-field"
                style={{
                  ...magnetGlowStyle,
                  transform: `scale(${1 + Math.min(0.05, magnetConfidence / 8)})`,
                }}
                aria-hidden
              />
            )}
            <Textarea
              ref={inputRef}
              value={input}
              onChange={handleInputChange}
              onKeyDown={handleKeyDown}
              placeholder={placeholder}
              className={cn(
                'min-h-[calc(var(--base-unit)*15)] resize-none flex-1 pr-28 transition-shadow relative z-[1]',
                showMagnetField ? 'magnet-textarea' : ''
              )}
              disabled={isInputDisabled}
              aria-label="Message input"
              data-testid="chat-input"
            />
          </div>

          {/* Attached adapters section */}
          <div className="flex flex-col gap-1">
            <div className="flex items-center justify-between text-xs text-muted-foreground">
              <span>Attached adapters</span>
              <div className="flex items-center gap-2">
                <span className={autoAttachEnabled ? 'text-primary font-medium' : ''}>
                  Auto-Attach {autoAttachEnabled ? 'on' : 'off'}
                </span>
                {autoAttachPaused && (
                  <span className="text-amber-600 font-medium">Temporarily paused</span>
                )}
              </div>
            </div>
            <div className="flex flex-wrap items-center gap-2">
              {attachedAdapters.length === 0 ? (
                <span className="text-xs text-muted-foreground">No attachments yet</span>
              ) : (
                attachedAdapters.map((adapter) => (
                  <AdapterAttachmentChip
                    key={adapter.id}
                    adapterId={adapter.id}
                    confidence={adapter.confidence}
                    onRemove={() => onRemoveAttachment(adapter.id, true)}
                    flash={lastAttachedAdapterId === adapter.id}
                  />
                ))
              )}
            </div>

            {/* Snap visual indicator */}
            {showSnapVisual && activeSuggestion && (
              <div className="flex items-center gap-2 text-xs text-primary pt-1">
                <Link2 className="h-3.5 w-3.5" aria-hidden />
                <span>Snapped {activeSuggestion.id} to this prompt</span>
              </div>
            )}

            {/* Suggested adapters */}
            {suggestedAdapters.length > 0 && (
              <div className="flex flex-wrap items-center gap-2 pt-1">
                <span className="text-xs text-muted-foreground">Suggested:</span>
                {suggestedAdapters.map((adapter) => (
                  <AdapterAttachmentChip
                    key={adapter.id}
                    adapterId={adapter.id}
                    confidence={adapter.confidence}
                    variant="suggested"
                    onClick={() => onAttachSuggested?.(adapter)}
                  />
                ))}
              </div>
            )}
          </div>
        </div>

        {/* Cancel button (shown during streaming) */}
        {isStreaming && onCancelStream && (
          <Button
            variant="outline"
            size="icon"
            onClick={onCancelStream}
            aria-label="Cancel response"
            className="mr-2"
            type="button"
          >
            <X className="h-4 w-4" />
          </Button>
        )}

        {/* Send button */}
        <Button
          type="submit"
          disabled={isSendDisabled}
          size="lg"
          aria-label={isStreaming ? 'Sending message...' : 'Send message'}
          title={!canExecuteInference ? 'Viewers cannot send messages' : undefined}
        >
          {isStreaming ? (
            <Loader2 className="h-4 w-4 animate-spin" />
          ) : (
            <Send className="h-4 w-4" />
          )}
        </Button>
      </form>
    </div>
  );
}

export default ChatInputArea;
