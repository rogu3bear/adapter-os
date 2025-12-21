import { describe, it, expect } from 'vitest';
import { calculateTrainingETA } from '@/utils/trainingEta';

describe('trainingEta', () => {
  describe('calculateTrainingETA', () => {
    it('should return null when progress is 0%', () => {
      const result = calculateTrainingETA(0, new Date(), new Date());
      expect(result).toBeNull();
    });

    it('should return null when progress is 100%', () => {
      const result = calculateTrainingETA(100, new Date(), new Date());
      expect(result).toBeNull();
    });

    it('should return null when status is paused', () => {
      const startedAt = new Date('2025-01-01T10:00:00Z');
      const currentTime = new Date('2025-01-01T10:30:00Z');
      const result = calculateTrainingETA(50, startedAt, currentTime, 'paused');
      expect(result).toBeNull();
    });

    it('should calculate ETA correctly for simple case', () => {
      const startedAt = new Date('2025-01-01T10:00:00Z');
      const currentTime = new Date('2025-01-01T10:30:00Z'); // 30 minutes elapsed

      // 50% progress in 30 minutes = 1% per minute
      // Remaining 50% should take 50 minutes
      const result = calculateTrainingETA(50, startedAt, currentTime);

      expect(result).toBe(1800); // 30 minutes in seconds
    });

    it('should calculate ETA with different progress rates', () => {
      const startedAt = new Date('2025-01-01T10:00:00Z');
      const currentTime = new Date('2025-01-01T11:00:00Z'); // 60 minutes elapsed

      // 75% progress in 60 minutes = 1.25% per minute
      // Remaining 25% should take 20 minutes
      const result = calculateTrainingETA(75, startedAt, currentTime);

      expect(result).toBe(1200); // 20 minutes in seconds
    });

    it('should use current time when not provided', () => {
      const startedAt = new Date(Date.now() - 30 * 60 * 1000); // 30 minutes ago
      const result = calculateTrainingETA(50, startedAt);

      // Should be approximately 30 minutes (1800 seconds)
      expect(result).toBeGreaterThan(1700);
      expect(result).toBeLessThan(1900);
    });

    it('should handle string date inputs', () => {
      const startedAt = '2025-01-01T10:00:00Z';
      const currentTime = new Date('2025-01-01T10:30:00Z');

      const result = calculateTrainingETA(50, startedAt, currentTime);
      expect(result).toBe(1800);
    });

    it('should account for paused duration', () => {
      const startedAt = new Date('2025-01-01T10:00:00Z');
      const currentTime = new Date('2025-01-01T11:00:00Z'); // 60 minutes elapsed
      const pausedDurationSeconds = 20 * 60; // 20 minutes paused

      // Actual active time: 60 - 20 = 40 minutes
      // 50% progress in 40 active minutes = 1.25% per minute
      // Remaining 50% should take 40 minutes
      const result = calculateTrainingETA(
        50,
        startedAt,
        currentTime,
        'running',
        undefined,
        pausedDurationSeconds
      );

      expect(result).toBe(2400); // 40 minutes in seconds
    });

    it('should return null when active elapsed time is 0', () => {
      const startedAt = new Date('2025-01-01T10:00:00Z');
      const currentTime = new Date('2025-01-01T10:00:00Z'); // Same time

      const result = calculateTrainingETA(50, startedAt, currentTime);
      expect(result).toBeNull();
    });

    it('should return null when progress rate is 0', () => {
      const startedAt = new Date('2025-01-01T10:00:00Z');
      const currentTime = new Date('2025-01-01T10:00:00Z');

      const result = calculateTrainingETA(0.000001, startedAt, currentTime);
      expect(result).toBeNull();
    });

    it('should handle paused duration exceeding elapsed time', () => {
      const startedAt = new Date('2025-01-01T10:00:00Z');
      const currentTime = new Date('2025-01-01T10:30:00Z'); // 30 minutes elapsed
      const pausedDurationSeconds = 60 * 60; // 60 minutes paused (more than elapsed)

      // Active time would be negative, should be clamped to 0
      const result = calculateTrainingETA(
        50,
        startedAt,
        currentTime,
        'running',
        undefined,
        pausedDurationSeconds
      );

      expect(result).toBeNull();
    });

    it('should return 0 when calculated ETA is negative', () => {
      const startedAt = new Date('2025-01-01T10:00:00Z');
      const currentTime = new Date('2025-01-01T10:00:01Z'); // 1 second elapsed

      // Very high progress (99.9%) in very short time
      // Should not return negative
      const result = calculateTrainingETA(99.9, startedAt, currentTime);

      expect(result).toBeGreaterThanOrEqual(0);
    });

    it('should handle fractional progress percentages', () => {
      const startedAt = new Date('2025-01-01T10:00:00Z');
      const currentTime = new Date('2025-01-01T10:30:00Z'); // 30 minutes elapsed

      const result = calculateTrainingETA(33.33, startedAt, currentTime);

      // 33.33% in 30 minutes, remaining 66.67% should take ~60 minutes
      expect(result).toBeGreaterThan(3500);
      expect(result).toBeLessThan(3700);
    });

    it('should handle very small progress values', () => {
      const startedAt = new Date('2025-01-01T10:00:00Z');
      const currentTime = new Date('2025-01-01T10:01:00Z'); // 1 minute elapsed

      const result = calculateTrainingETA(1, startedAt, currentTime);

      // 1% in 1 minute, remaining 99% should take 99 minutes
      expect(result).toBe(5940); // 99 minutes in seconds
    });

    it('should handle very high progress values', () => {
      const startedAt = new Date('2025-01-01T10:00:00Z');
      const currentTime = new Date('2025-01-01T10:30:00Z'); // 30 minutes elapsed

      const result = calculateTrainingETA(99, startedAt, currentTime);

      // 99% in 30 minutes, remaining 1% should take ~18.18 seconds
      expect(result).toBeGreaterThan(15);
      expect(result).toBeLessThan(20);
    });
  });
});
