import { describe, it, expect } from 'vitest';
import { getUserFriendlyError } from '@/utils/errorMessages';

describe('errorMessages', () => {
  it('maps OUT_OF_MEMORY to memory guidance', () => {
    const error = getUserFriendlyError('OUT_OF_MEMORY', undefined, {
      memoryRequired: 5120,
      memoryAvailable: 2048,
    });

    expect(error.title).toBe('Not Enough Memory');
    expect(error.variant).toBe('error');
    expect(error.message).toContain('Could not load the model');
    expect(error.actionText).toBe('Free Memory');
  });

  it('maps LOAD_FAILED to retryable guidance', () => {
    const error = getUserFriendlyError('LOAD_FAILED', undefined, {
      modelId: 'qwen7b',
    });

    expect(error.title).toBe('Model Loading Failed');
    expect(error.variant).toBe('error');
    expect(error.message).toContain('qwen7b');
    expect(error.actionText).toBe('Try Again');
  });
});
