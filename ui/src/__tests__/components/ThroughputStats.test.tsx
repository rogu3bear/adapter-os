import { describe, it, expect } from 'vitest';
import type { ThroughputStats } from '@/components/chat/ChatMessage';

/**
 * Tests for throughput stats calculation logic.
 * The actual calculation happens in ChatInterface.tsx onStreamComplete callback.
 */
describe('Throughput Stats Calculation', () => {
  /**
   * Recreate the calculation logic from ChatInterface for testing
   */
  function calculateThroughputStats(
    tokensReceived: number,
    streamDuration: number | null | undefined
  ): ThroughputStats | undefined {
    return streamDuration && streamDuration > 0 && tokensReceived > 0
      ? {
          tokensGenerated: tokensReceived,
          latencyMs: streamDuration,
          tokensPerSecond: tokensReceived / (streamDuration / 1000),
        }
      : undefined;
  }

  describe('Valid inputs', () => {
    it('calculates throughput for normal values', () => {
      const result = calculateThroughputStats(100, 2000);

      expect(result).toBeDefined();
      expect(result!.tokensGenerated).toBe(100);
      expect(result!.latencyMs).toBe(2000);
      expect(result!.tokensPerSecond).toBe(50); // 100 tokens / 2 seconds
    });

    it('calculates throughput for fast generation', () => {
      const result = calculateThroughputStats(500, 1000);

      expect(result).toBeDefined();
      expect(result!.tokensPerSecond).toBe(500); // 500 tokens / 1 second
    });

    it('calculates throughput for slow generation', () => {
      const result = calculateThroughputStats(10, 10000);

      expect(result).toBeDefined();
      expect(result!.tokensPerSecond).toBe(1); // 10 tokens / 10 seconds
    });

    it('handles fractional tokens per second', () => {
      const result = calculateThroughputStats(45, 2800);

      expect(result).toBeDefined();
      expect(result!.tokensPerSecond).toBeCloseTo(16.07, 1); // 45 / 2.8
    });

    it('handles single token', () => {
      const result = calculateThroughputStats(1, 500);

      expect(result).toBeDefined();
      expect(result!.tokensGenerated).toBe(1);
      expect(result!.tokensPerSecond).toBe(2); // 1 token / 0.5 seconds
    });

    it('handles very fast generation (sub-second)', () => {
      const result = calculateThroughputStats(50, 100);

      expect(result).toBeDefined();
      expect(result!.tokensPerSecond).toBe(500); // 50 / 0.1 seconds
    });
  });

  describe('Edge cases - returns undefined', () => {
    it('returns undefined when streamDuration is 0', () => {
      const result = calculateThroughputStats(100, 0);
      expect(result).toBeUndefined();
    });

    it('returns undefined when streamDuration is null', () => {
      const result = calculateThroughputStats(100, null);
      expect(result).toBeUndefined();
    });

    it('returns undefined when streamDuration is undefined', () => {
      const result = calculateThroughputStats(100, undefined);
      expect(result).toBeUndefined();
    });

    it('returns undefined when tokensReceived is 0', () => {
      const result = calculateThroughputStats(0, 2000);
      expect(result).toBeUndefined();
    });

    it('returns undefined when both are 0', () => {
      const result = calculateThroughputStats(0, 0);
      expect(result).toBeUndefined();
    });

    it('returns undefined when tokensReceived is negative', () => {
      // This shouldn't happen, but guard against it
      const result = calculateThroughputStats(-10, 2000);
      expect(result).toBeUndefined();
    });

    it('returns undefined when streamDuration is negative', () => {
      // This shouldn't happen, but guard against it
      const result = calculateThroughputStats(100, -2000);
      expect(result).toBeUndefined();
    });
  });

  describe('Display formatting', () => {
    /**
     * Test the display format as it appears in ChatMessage
     */
    function formatThroughputDisplay(stats: ThroughputStats): string {
      return `${stats.tokensPerSecond.toFixed(1)} tok/s | ${stats.tokensGenerated} tokens | ${(stats.latencyMs / 1000).toFixed(1)}s`;
    }

    it('formats display string correctly', () => {
      const stats: ThroughputStats = {
        tokensGenerated: 128,
        latencyMs: 2800,
        tokensPerSecond: 45.71,
      };

      const display = formatThroughputDisplay(stats);
      expect(display).toBe('45.7 tok/s | 128 tokens | 2.8s');
    });

    it('formats whole numbers correctly', () => {
      const stats: ThroughputStats = {
        tokensGenerated: 100,
        latencyMs: 2000,
        tokensPerSecond: 50,
      };

      const display = formatThroughputDisplay(stats);
      expect(display).toBe('50.0 tok/s | 100 tokens | 2.0s');
    });

    it('handles sub-second latency', () => {
      const stats: ThroughputStats = {
        tokensGenerated: 50,
        latencyMs: 500,
        tokensPerSecond: 100,
      };

      const display = formatThroughputDisplay(stats);
      expect(display).toBe('100.0 tok/s | 50 tokens | 0.5s');
    });
  });
});

describe('Verification Labels', () => {
  /**
   * Map of engineering terms to human-facing labels
   */
  const VERIFICATION_LABELS = {
    exact: { human: 'Verified', engineering: 'Exact' },
    semantic: { human: 'Balanced', engineering: 'Semantic' },
    divergent: { human: 'Unverified', engineering: 'Divergent' },
    error: { human: 'Error', engineering: 'Error' },
  } as const;

  type MatchStatus = keyof typeof VERIFICATION_LABELS;

  it('maps exact to Verified', () => {
    expect(VERIFICATION_LABELS.exact.human).toBe('Verified');
    expect(VERIFICATION_LABELS.exact.engineering).toBe('Exact');
  });

  it('maps semantic to Balanced', () => {
    expect(VERIFICATION_LABELS.semantic.human).toBe('Balanced');
    expect(VERIFICATION_LABELS.semantic.engineering).toBe('Semantic');
  });

  it('maps divergent to Unverified', () => {
    expect(VERIFICATION_LABELS.divergent.human).toBe('Unverified');
    expect(VERIFICATION_LABELS.divergent.engineering).toBe('Divergent');
  });

  it('maps error to Error', () => {
    expect(VERIFICATION_LABELS.error.human).toBe('Error');
    expect(VERIFICATION_LABELS.error.engineering).toBe('Error');
  });

  it('all statuses have both labels defined', () => {
    const statuses: MatchStatus[] = ['exact', 'semantic', 'divergent', 'error'];

    statuses.forEach((status) => {
      expect(VERIFICATION_LABELS[status].human).toBeDefined();
      expect(VERIFICATION_LABELS[status].human.length).toBeGreaterThan(0);
      expect(VERIFICATION_LABELS[status].engineering).toBeDefined();
      expect(VERIFICATION_LABELS[status].engineering.length).toBeGreaterThan(0);
    });
  });
});

describe('Citation Stability', () => {
  interface RagReproducibility {
    score: number;
    matching_docs: number;
    total_original_docs: number;
    missing_doc_ids: string[];
  }

  function getCitationStabilityBadgeVariant(
    score: number
  ): 'success' | 'warning' | 'error' {
    if (score === 1) return 'success';
    if (score >= 0.8) return 'warning';
    return 'error';
  }

  describe('Badge variant selection', () => {
    it('returns success for 100% stability', () => {
      expect(getCitationStabilityBadgeVariant(1)).toBe('success');
    });

    it('returns warning for 80-99% stability', () => {
      expect(getCitationStabilityBadgeVariant(0.99)).toBe('warning');
      expect(getCitationStabilityBadgeVariant(0.9)).toBe('warning');
      expect(getCitationStabilityBadgeVariant(0.8)).toBe('warning');
    });

    it('returns error for below 80% stability', () => {
      expect(getCitationStabilityBadgeVariant(0.79)).toBe('error');
      expect(getCitationStabilityBadgeVariant(0.5)).toBe('error');
      expect(getCitationStabilityBadgeVariant(0)).toBe('error');
    });
  });

  describe('Citation summary text', () => {
    function getCitationSummary(rag: RagReproducibility): string {
      return `${rag.matching_docs} of ${rag.total_original_docs} citations unchanged`;
    }

    it('formats summary correctly for full match', () => {
      const rag: RagReproducibility = {
        score: 1,
        matching_docs: 5,
        total_original_docs: 5,
        missing_doc_ids: [],
      };
      expect(getCitationSummary(rag)).toBe('5 of 5 citations unchanged');
    });

    it('formats summary correctly for partial match', () => {
      const rag: RagReproducibility = {
        score: 0.6,
        matching_docs: 3,
        total_original_docs: 5,
        missing_doc_ids: ['doc-1', 'doc-2'],
      };
      expect(getCitationSummary(rag)).toBe('3 of 5 citations unchanged');
    });

    it('formats summary correctly for no match', () => {
      const rag: RagReproducibility = {
        score: 0,
        matching_docs: 0,
        total_original_docs: 3,
        missing_doc_ids: ['doc-1', 'doc-2', 'doc-3'],
      };
      expect(getCitationSummary(rag)).toBe('0 of 3 citations unchanged');
    });
  });
});
