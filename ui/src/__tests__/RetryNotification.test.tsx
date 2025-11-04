import { render, screen, act } from '@testing-library/react';
import { vi } from 'vitest';
import { RetryNotification } from '../components/ui/retry-notification';

// Mock timers
beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.clearAllTimers();
  vi.useRealTimers();
});

describe('RetryNotification Timer Accuracy', () => {
  const defaultProps = {
    operation: 'test-operation',
    attempt: 1,
    maxAttempts: 3,
    delayMs: 5000,
  };

  it('should countdown correctly from initial delayMs', () => {
    render(<RetryNotification {...defaultProps} />);

    // Initial state should show full delay
    expect(screen.getByText('Next attempt in 5 seconds')).toBeInTheDocument();

    // Advance 1 second
    act(() => {
      vi.advanceTimersByTime(1000);
    });
    expect(screen.getByText('Next attempt in 4 seconds')).toBeInTheDocument();

    // Advance another second
    act(() => {
      vi.advanceTimersByTime(1000);
    });
    expect(screen.getByText('Next attempt in 3 seconds')).toBeInTheDocument();
  });

  it('should handle rapid delayMs changes without overlapping intervals', () => {
    const { rerender } = render(<RetryNotification {...defaultProps} />);

    // Start with 5 seconds
    expect(screen.getByText('Next attempt in 5 seconds')).toBeInTheDocument();

    // Advance 2 seconds (3 seconds remaining)
    act(() => {
      vi.advanceTimersByTime(2000);
    });
    expect(screen.getByText('Next attempt in 3 seconds')).toBeInTheDocument();

    // Change delayMs to 8000 (should reset to 8 seconds)
    rerender(<RetryNotification {...defaultProps} delayMs={8000} />);
    expect(screen.getByText('Next attempt in 8 seconds')).toBeInTheDocument();

    // Advance 1 second (7 seconds remaining)
    act(() => {
      vi.advanceTimersByTime(1000);
    });
    expect(screen.getByText('Next attempt in 7 seconds')).toBeInTheDocument();

    // Change delayMs again to 2000 (should reset to 2 seconds)
    rerender(<RetryNotification {...defaultProps} delayMs={2000} />);
    expect(screen.getByText('Next attempt in 2 seconds')).toBeInTheDocument();

    // Advance 1 second (1 second remaining)
    act(() => {
      vi.advanceTimersByTime(1000);
    });
    expect(screen.getByText('Next attempt in 1 second')).toBeInTheDocument();
  });

  it('should clear previous interval when delayMs changes', () => {
    const { rerender } = render(<RetryNotification {...defaultProps} />);

    // Start countdown
    expect(screen.getByText('Next attempt in 5 seconds')).toBeInTheDocument();

    // Advance 3 seconds (2 seconds remaining)
    act(() => {
      vi.advanceTimersByTime(3000);
    });
    expect(screen.getByText('Next attempt in 2 seconds')).toBeInTheDocument();

    // Change delayMs - this should clear the old interval and start fresh
    rerender(<RetryNotification {...defaultProps} delayMs={10000} />);
    expect(screen.getByText('Next attempt in 10 seconds')).toBeInTheDocument();

    // Advance 2 seconds - should still be at 8 seconds (not affected by old interval)
    act(() => {
      vi.advanceTimersByTime(2000);
    });
    expect(screen.getByText('Next attempt in 8 seconds')).toBeInTheDocument();
  });

  it('should not create overlapping countdowns when delayMs changes rapidly', () => {
    const { rerender } = render(<RetryNotification {...defaultProps} />);

    // Start with 5s
    expect(screen.getByText('Next attempt in 5 seconds')).toBeInTheDocument();

    // Rapidly change delayMs multiple times
    rerender(<RetryNotification {...defaultProps} delayMs={3000} />);
    expect(screen.getByText('Next attempt in 3 seconds')).toBeInTheDocument();

    rerender(<RetryNotification {...defaultProps} delayMs={7000} />);
    expect(screen.getByText('Next attempt in 7 seconds')).toBeInTheDocument();

    rerender(<RetryNotification {...defaultProps} delayMs={1000} />);
    expect(screen.getByText('Next attempt in 1 second')).toBeInTheDocument();

    // Advance 1 second - should reach 0 and stop
    act(() => {
      vi.advanceTimersByTime(1000);
    });

    // Should not show negative numbers or continue counting
    expect(screen.queryByText(/Next attempt in/)).not.toBeInTheDocument();
  });
});
