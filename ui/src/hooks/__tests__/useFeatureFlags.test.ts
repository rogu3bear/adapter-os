/**
 * Tests for feature flag hooks
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { useChatAutoLoadModels, useFeatureFlag } from '@/hooks/config/useFeatureFlags';

describe('useFeatureFlags', () => {
  describe('useChatAutoLoadModels', () => {
    it('should return false when VITE_CHAT_AUTO_LOAD_MODELS is not set', () => {
      // Default behavior when environment variable is not set
      const result = useChatAutoLoadModels();
      expect(result).toBe(false);
    });

    it('should return false when VITE_CHAT_AUTO_LOAD_MODELS is set to "false"', () => {
      // @ts-expect-error - mocking environment variable
      import.meta.env.VITE_CHAT_AUTO_LOAD_MODELS = 'false';
      const result = useChatAutoLoadModels();
      expect(result).toBe(false);
    });

    it('should return true when VITE_CHAT_AUTO_LOAD_MODELS is set to "true"', () => {
      // @ts-expect-error - mocking environment variable
      import.meta.env.VITE_CHAT_AUTO_LOAD_MODELS = 'true';
      const result = useChatAutoLoadModels();
      expect(result).toBe(true);
    });

    it('should return false for any other value', () => {
      // @ts-expect-error - mocking environment variable
      import.meta.env.VITE_CHAT_AUTO_LOAD_MODELS = 'yes';
      const result = useChatAutoLoadModels();
      expect(result).toBe(false);
    });
  });

  describe('useFeatureFlag', () => {
    it('should return false for undefined flags', () => {
      const result = useFeatureFlag('NONEXISTENT_FLAG');
      expect(result).toBe(false);
    });

    it('should return true when flag is set to "true"', () => {
      // @ts-expect-error - mocking environment variable
      import.meta.env.VITE_TEST_FLAG = 'true';
      const result = useFeatureFlag('TEST_FLAG');
      expect(result).toBe(true);
    });

    it('should return false when flag is set to "false"', () => {
      // @ts-expect-error - mocking environment variable
      import.meta.env.VITE_TEST_FLAG = 'false';
      const result = useFeatureFlag('TEST_FLAG');
      expect(result).toBe(false);
    });
  });
});
