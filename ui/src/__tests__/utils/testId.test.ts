import { describe, it, expect } from 'vitest';
import { testId } from '@/utils/testId';

describe('testId utility', () => {
  describe('testId', () => {
    it('returns object with data-testid attribute', () => {
      const result = testId('my-button');
      expect(result).toEqual({ 'data-testid': 'my-button' });
    });

    it('handles empty string', () => {
      const result = testId('');
      expect(result).toEqual({ 'data-testid': '' });
    });

    it('handles kebab-case identifiers', () => {
      const result = testId('submit-form-button');
      expect(result).toEqual({ 'data-testid': 'submit-form-button' });
    });

    it('handles camelCase identifiers', () => {
      const result = testId('submitFormButton');
      expect(result).toEqual({ 'data-testid': 'submitFormButton' });
    });

    it('handles snake_case identifiers', () => {
      const result = testId('submit_form_button');
      expect(result).toEqual({ 'data-testid': 'submit_form_button' });
    });

    it('handles numeric identifiers', () => {
      const result = testId('button-123');
      expect(result).toEqual({ 'data-testid': 'button-123' });
    });

    it('handles identifiers with special characters', () => {
      const result = testId('adapter:list:item-1');
      expect(result).toEqual({ 'data-testid': 'adapter:list:item-1' });
    });

    it('handles long identifiers', () => {
      const longId = 'very-long-test-id-with-many-segments-for-testing';
      const result = testId(longId);
      expect(result).toEqual({ 'data-testid': longId });
    });

    it('handles identifiers with spaces (though not recommended)', () => {
      const result = testId('test id with spaces');
      expect(result).toEqual({ 'data-testid': 'test id with spaces' });
    });

    it('handles identifiers with underscores and hyphens', () => {
      const result = testId('test_id-mixed-123');
      expect(result).toEqual({ 'data-testid': 'test_id-mixed-123' });
    });

    it('can be spread into JSX props', () => {
      const props = { className: 'btn', ...testId('submit-btn') };
      expect(props).toEqual({
        className: 'btn',
        'data-testid': 'submit-btn',
      });
    });

    it('handles hierarchical test ids', () => {
      const result = testId('adapter-list-item-action-delete');
      expect(result).toEqual({ 'data-testid': 'adapter-list-item-action-delete' });
    });

    it('handles namespaced test ids', () => {
      const result = testId('auth:login:submit');
      expect(result).toEqual({ 'data-testid': 'auth:login:submit' });
    });

    it('returns new object each time', () => {
      const result1 = testId('button');
      const result2 = testId('button');
      expect(result1).toEqual(result2);
      expect(result1).not.toBe(result2); // Different object references
    });

    it('handles unicode characters', () => {
      const result = testId('button-🚀');
      expect(result).toEqual({ 'data-testid': 'button-🚀' });
    });

    it('handles dynamic test ids', () => {
      const dynamicId = `adapter-${123}-delete`;
      const result = testId(dynamicId);
      expect(result).toEqual({ 'data-testid': 'adapter-123-delete' });
    });
  });
});
