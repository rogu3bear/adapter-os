import React from 'react';
import { Textarea } from '../ui/textarea';
import { Label } from '../ui/label';
import { Alert, AlertDescription } from '../ui/alert';
import { HelpTooltip } from '@/components/ui/help-tooltip';
import { AlertTriangle, HelpCircle } from 'lucide-react';

export const MAX_PROMPT_LENGTH = 50000;
export const MAX_PROMPT_BYTES = 100000;

export interface ValidationResult {
  valid: boolean;
  error?: string;
  warning?: string;
  suggestion?: string;
}

export const validatePromptLength = (prompt: string): ValidationResult => {
  if (prompt.length > MAX_PROMPT_LENGTH) {
    return {
      valid: false,
      error: `Prompt too long (${prompt.length.toLocaleString()} characters). Maximum: ${MAX_PROMPT_LENGTH.toLocaleString()}`,
      suggestion: 'Consider breaking into smaller chunks or using batch processing for large inputs'
    };
  }

  const byteLength = new Blob([prompt]).size;
  if (byteLength > MAX_PROMPT_BYTES) {
    return {
      valid: false,
      error: `Prompt size too large (${(byteLength / 1024).toFixed(1)}KB). Maximum: ${(MAX_PROMPT_BYTES / 1024).toFixed(0)}KB`,
      suggestion: 'Reduce content size or consider using file upload for large documents'
    };
  }

  if (prompt.length > MAX_PROMPT_LENGTH * 0.8) {
    return {
      valid: true,
      warning: `Approaching character limit (${prompt.length.toLocaleString()}/${MAX_PROMPT_LENGTH.toLocaleString()})`
    };
  }

  return { valid: true };
};

export const validateUnicodeContent = (text: string): ValidationResult => {
  try {
    const normalized = text.normalize('NFC');

    const hasProblematicUnicode = /[\u0000-\u0008\u000B\u000C\u000E-\u001F\u007F-\u009F\u200B\u200C\u200D]/.test(normalized);
    if (hasProblematicUnicode) {
      return {
        valid: false,
        error: 'Prompt contains unsupported control or invisible characters',
        suggestion: 'Remove or replace invisible characters, zero-width spaces, or control characters'
      };
    }

    const emojiCount = (normalized.match(/\p{Emoji}/gu) || []).length;
    const textLength = normalized.replace(/\p{Emoji}/gu, '').length;
    if (emojiCount > textLength * 0.5 && emojiCount > 20) {
      return {
        valid: false,
        error: 'Too many emojis detected',
        suggestion: 'Reduce emoji usage or use descriptive text instead'
      };
    }

    return { valid: true };
  } catch {
    return {
      valid: false,
      error: 'Unicode processing failed - text may contain invalid characters',
      suggestion: 'Try re-entering the text or copy from a different source'
    };
  }
};

export const validatePromptContent = (prompt: string): ValidationResult => {
  if (!prompt || prompt.trim().length === 0) {
    return {
      valid: false,
      error: 'Prompt cannot be empty',
      suggestion: 'Please enter a question or instruction for the AI model'
    };
  }

  const visibleChars = prompt.replace(/[\u0000-\u0008\u000B\u000C\u000E-\u001F\u007F-\u009F\u200B\u200C\u200D\s]/g, '');
  if (visibleChars.length === 0) {
    return {
      valid: false,
      error: 'Prompt contains only invisible characters or whitespace',
      suggestion: 'Please enter meaningful text content'
    };
  }

  const normalizedLength = prompt.normalize('NFC').trim().length;
  if (normalizedLength < 3) {
    return {
      valid: false,
      error: 'Prompt too short',
      suggestion: 'Please provide more context (minimum 3 characters)'
    };
  }

  return { valid: true };
};

export const validatePrompt = (prompt: string): ValidationResult => {
  const lengthValidation = validatePromptLength(prompt);
  if (!lengthValidation.valid) return lengthValidation;

  const contentValidation = validatePromptContent(prompt);
  if (!contentValidation.valid) return contentValidation;

  const unicodeValidation = validateUnicodeContent(prompt);
  if (!unicodeValidation.valid) return unicodeValidation;

  const warnings = [lengthValidation.warning, contentValidation.warning, unicodeValidation.warning]
    .filter(Boolean)
    .join('; ');

  return {
    valid: true,
    ...(warnings && { warning: warnings })
  };
};

export interface PromptInputProps {
  value: string;
  onChange: (value: string) => void;
  validation: ValidationResult | null;
  rows?: number;
  placeholder?: string;
  isMobile?: boolean;
}

export function PromptInput({
  value,
  onChange,
  validation,
  rows = 6,
  placeholder = 'Enter your prompt here...',
  isMobile = false
}: PromptInputProps) {
  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <Label htmlFor="prompt" className="flex items-center gap-1">
          Prompt
          <HelpTooltip helpId="inference-prompt">
            <span className="cursor-help text-muted-foreground hover:text-foreground">
              <HelpCircle className="h-3 w-3" />
            </span>
          </HelpTooltip>
          <span className="sr-only">
            Use Ctrl+G or Cmd+G to generate, Ctrl+S or Cmd+S to toggle streaming mode, Ctrl+B or Cmd+B to toggle batch mode, Escape to cancel
          </span>
        </Label>
      </div>

      <Textarea
        id="prompt"
        placeholder={placeholder}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        rows={rows}
        className={validation?.valid === false ? 'border-destructive' : ''}
        aria-describedby={
          validation?.error ? 'prompt-error' : validation?.warning ? 'prompt-warning' : undefined
        }
        aria-invalid={validation?.valid === false}
      />

      {validation?.error && (
        <Alert variant="destructive" className="text-sm" id="prompt-error">
          <AlertTriangle className="h-4 w-4" />
          <AlertDescription>
            <strong>Validation Error:</strong> {validation.error}
            {validation.suggestion && (
              <div className="mt-1 text-sm opacity-90">
                <strong>Suggestion:</strong> {validation.suggestion}
              </div>
            )}
          </AlertDescription>
        </Alert>
      )}

      {validation?.warning && (
        <Alert variant="default" className="text-sm border-yellow-200 bg-yellow-50" id="prompt-warning">
          <AlertTriangle className="h-4 w-4 text-yellow-600" />
          <AlertDescription className="text-yellow-800">
            <strong>Warning:</strong> {validation.warning}
          </AlertDescription>
        </Alert>
      )}

      {validation?.valid === false && !validation.error && (
        <div className="text-xs text-muted-foreground">
          Character count: {value.length.toLocaleString()} / {MAX_PROMPT_LENGTH.toLocaleString()}
        </div>
      )}

      {isMobile && (
        <div className="text-xs text-muted-foreground mt-1">
          Swipe left/right to change modes, swipe up for templates
        </div>
      )}
    </div>
  );
}
