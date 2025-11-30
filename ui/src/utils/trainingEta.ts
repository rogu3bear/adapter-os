/**
 * Calculate estimated time remaining for training job
 * Based on progress percentage and elapsed time
 * Handles paused jobs by not counting paused time in rate calculation
 */
export function calculateTrainingETA(
  progressPct: number,
  startedAt: string | Date,
  currentTime?: Date,
  status?: string,
  pausedAt?: string | Date,
  pausedDurationSeconds?: number
): number | null {
  if (progressPct <= 0 || progressPct >= 100) {
    return null;
  }

  // If job is paused, return null (can't estimate)
  if (status === 'paused') {
    return null;
  }

  const startTime = typeof startedAt === 'string' ? new Date(startedAt) : startedAt;
  const now = currentTime || new Date();
  const elapsedMs = now.getTime() - startTime.getTime();
  
  // Subtract paused duration if provided
  const pausedMs = pausedDurationSeconds ? pausedDurationSeconds * 1000 : 0;
  const activeElapsedMs = Math.max(0, elapsedMs - pausedMs);
  const activeElapsedSeconds = activeElapsedMs / 1000;

  if (activeElapsedSeconds <= 0) {
    return null;
  }

  // Calculate rate: progress per second (only counting active time)
  const progressRate = progressPct / activeElapsedSeconds;
  
  if (progressRate <= 0) {
    return null;
  }

  // Calculate remaining progress
  const remainingProgress = 100 - progressPct;
  
  // Estimate time remaining based on active rate
  const estimatedSeconds = remainingProgress / progressRate;

  return Math.max(0, estimatedSeconds);
}

// Re-export formatDurationSeconds as formatDuration for backward compatibility
// Use formatDurationSeconds directly from @/utils/format for new code
export { formatDurationSeconds as formatDuration } from './format';

