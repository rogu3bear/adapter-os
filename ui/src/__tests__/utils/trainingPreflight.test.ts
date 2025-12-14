import { describe, it, expect } from 'vitest';
import {
  runClientPreflight,
  canUseQuickTrain,
  getPreflightSummary,
  type ClientPreflightResult,
} from '@/utils/trainingPreflight';
import type { Dataset } from '@/api/training-types';

/**
 * Create a mock dataset with sensible defaults
 */
function createMockDataset(overrides: Partial<Dataset> = {}): Dataset {
  return {
    id: 'test-dataset-id',
    name: 'Test Dataset',
    description: 'A test dataset',
    file_count: 10,
    total_size_bytes: 1024 * 1024, // 1 MB
    format: 'jsonl',
    hash_b3: 'abc123',
    storage_path: '/test/path',
    validation_status: 'valid',
    created_at: '2024-01-01T00:00:00Z',
    updated_at: '2024-01-01T00:00:00Z',
    trust_state: 'allowed',
    total_tokens: 10000,
    ...overrides,
  } as Dataset;
}

describe('trainingPreflight', () => {
  describe('runClientPreflight', () => {
    it('returns passed=true for a valid dataset', () => {
      const dataset = createMockDataset();
      const result = runClientPreflight(dataset);

      expect(result.passed).toBe(true);
      expect(result.clean).toBe(true);
      expect(result.checks.length).toBeGreaterThanOrEqual(4);
    });

    it('fails validation_status check for invalid datasets', () => {
      const dataset = createMockDataset({ validation_status: 'invalid' });
      const result = runClientPreflight(dataset);

      expect(result.passed).toBe(false);
      const validationCheck = result.checks.find((c) => c.policy_id === 'validation_status');
      expect(validationCheck?.passed).toBe(false);
      expect(validationCheck?.severity).toBe('error');
    });

    it('fails trust_state check for blocked datasets', () => {
      const dataset = createMockDataset({ trust_state: 'blocked' });
      const result = runClientPreflight(dataset);

      expect(result.passed).toBe(false);
      const trustCheck = result.checks.find((c) => c.policy_id === 'trust_state');
      expect(trustCheck?.passed).toBe(false);
      expect(trustCheck?.severity).toBe('error');
    });

    it('passes with warning for allowed_with_warning trust state', () => {
      const dataset = createMockDataset({ trust_state: 'allowed_with_warning' });
      const result = runClientPreflight(dataset);

      expect(result.passed).toBe(true);
      expect(result.clean).toBe(false); // Has warning
      const trustCheck = result.checks.find((c) => c.policy_id === 'trust_state');
      expect(trustCheck?.passed).toBe(true);
      expect(trustCheck?.severity).toBe('warning');
    });

    it('fails file_count check for empty datasets', () => {
      const dataset = createMockDataset({ file_count: 0 });
      const result = runClientPreflight(dataset);

      expect(result.passed).toBe(false);
      const fileCheck = result.checks.find((c) => c.policy_id === 'file_count');
      expect(fileCheck?.passed).toBe(false);
      expect(fileCheck?.severity).toBe('error');
    });

    it('warns for large datasets (>1GB)', () => {
      const dataset = createMockDataset({
        total_size_bytes: 2 * 1024 * 1024 * 1024, // 2 GB
      });
      const result = runClientPreflight(dataset);

      expect(result.passed).toBe(true);
      expect(result.clean).toBe(false); // Has warning
      const sizeCheck = result.checks.find((c) => c.policy_id === 'size_limit');
      expect(sizeCheck?.passed).toBe(true);
      expect(sizeCheck?.severity).toBe('warning');
    });

    it('fails for huge datasets (>10GB)', () => {
      const dataset = createMockDataset({
        total_size_bytes: 15 * 1024 * 1024 * 1024, // 15 GB
      });
      const result = runClientPreflight(dataset);

      expect(result.passed).toBe(false);
      const sizeCheck = result.checks.find((c) => c.policy_id === 'size_limit');
      expect(sizeCheck?.passed).toBe(false);
      expect(sizeCheck?.severity).toBe('error');
    });

    it('warns for datasets with no tokens', () => {
      const dataset = createMockDataset({ total_tokens: 0 });
      const result = runClientPreflight(dataset);

      expect(result.passed).toBe(true);
      expect(result.clean).toBe(false);
      const tokenCheck = result.checks.find((c) => c.policy_id === 'token_count');
      expect(tokenCheck?.passed).toBe(false);
      expect(tokenCheck?.severity).toBe('warning');
    });

    it('does not add token warning if no files present', () => {
      const dataset = createMockDataset({ file_count: 0, total_tokens: 0 });
      const result = runClientPreflight(dataset);

      // Token check should not be added when there are no files
      const tokenCheck = result.checks.find((c) => c.policy_id === 'token_count');
      expect(tokenCheck).toBeUndefined();
    });
  });

  describe('canUseQuickTrain', () => {
    it('returns true for valid datasets', () => {
      const dataset = createMockDataset();
      expect(canUseQuickTrain(dataset)).toBe(true);
    });

    it('returns false for invalid datasets', () => {
      const dataset = createMockDataset({ validation_status: 'pending' });
      expect(canUseQuickTrain(dataset)).toBe(false);
    });

    it('returns false for blocked trust state', () => {
      const dataset = createMockDataset({ trust_state: 'blocked' });
      expect(canUseQuickTrain(dataset)).toBe(false);
    });

    it('returns true for datasets with warnings (warnings are allowed)', () => {
      const dataset = createMockDataset({ trust_state: 'allowed_with_warning' });
      expect(canUseQuickTrain(dataset)).toBe(true);
    });

    it('returns false for empty datasets', () => {
      const dataset = createMockDataset({ file_count: 0 });
      expect(canUseQuickTrain(dataset)).toBe(false);
    });
  });

  describe('getPreflightSummary', () => {
    it('returns clean message when all checks pass', () => {
      const result: ClientPreflightResult = {
        passed: true,
        clean: true,
        checks: [
          { policy_id: 'test', policy_name: 'Test', passed: true, severity: 'info', message: 'OK' },
        ],
      };

      expect(getPreflightSummary(result)).toBe('All checks passed. Ready to start training.');
    });

    it('returns warning message when passed with warnings', () => {
      const result: ClientPreflightResult = {
        passed: true,
        clean: false,
        checks: [
          {
            policy_id: 'test',
            policy_name: 'Test',
            passed: true,
            severity: 'warning',
            message: 'Warning',
          },
        ],
      };

      expect(getPreflightSummary(result)).toContain('warning');
      expect(getPreflightSummary(result)).toContain('Review before proceeding');
    });

    it('returns error message when checks fail', () => {
      const result: ClientPreflightResult = {
        passed: false,
        clean: false,
        checks: [
          {
            policy_id: 'test',
            policy_name: 'Test',
            passed: false,
            severity: 'error',
            message: 'Error',
          },
          {
            policy_id: 'test2',
            policy_name: 'Test 2',
            passed: false,
            severity: 'error',
            message: 'Error 2',
          },
        ],
      };

      expect(getPreflightSummary(result)).toContain('2 issues');
      expect(getPreflightSummary(result)).toContain('must be resolved');
    });

    it('handles singular issue in error message', () => {
      const result: ClientPreflightResult = {
        passed: false,
        clean: false,
        checks: [
          {
            policy_id: 'test',
            policy_name: 'Test',
            passed: false,
            severity: 'error',
            message: 'Error',
          },
        ],
      };

      expect(getPreflightSummary(result)).toContain('1 issue');
      expect(getPreflightSummary(result)).not.toContain('issues');
    });
  });
});
