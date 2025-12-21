import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MemoryRouter } from 'react-router-dom';
import { TrainingProgress } from '@/pages/Training/TrainingProgress';
import type { TrainingJob } from '@/api/training-types';
import type { ReactElement } from 'react';
import { Dialog, DialogContent } from '@/components/ui/dialog';
import * as React from 'react';

const jobStore = vi.hoisted(() => {
  let state: TrainingJob | null = null;
  const listeners = new Set<(job: TrainingJob | null) => void>();
  return {
    set(job: TrainingJob | null) {
      state = job;
      listeners.forEach(listener => listener(job));
    },
    get() {
      return state;
    },
    subscribe(listener: (job: TrainingJob | null) => void) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    reset() {
      state = null;
      listeners.clear();
    },
  };
});

const mockApiClient = vi.hoisted(() => ({
  downloadArtifact: vi.fn(),
}));

vi.mock('@/api/client', () => ({
  __esModule: true,
  default: mockApiClient,
}));

vi.mock('@/hooks/streaming/useStreamingEndpoints', () => {
  const mockUseTrainingStream = vi.fn().mockReturnValue({
    connected: false,
    error: null,
    lastUpdated: null,
  });
  return { useTrainingStream: mockUseTrainingStream };
});

vi.mock('@/hooks/training', () => {
  const useTrainingJob = () => {
    const [job, setJob] = React.useState<TrainingJob | null>(jobStore.get());
    React.useEffect(() => jobStore.subscribe(setJob), []);
    return { data: job, isLoading: false, error: null, refetch: vi.fn() };
  };

  const useJobLogs = () => ({ data: [], isLoading: false, refetch: vi.fn() });
  const useJobMetrics = () => ({ data: null, isLoading: false, refetch: vi.fn() });
  const useJobArtifacts = () => ({
    data: {
      artifacts: [
        {
          id: 'artifact-1',
          type: 'final' as const,
          path: '/artifacts/test-stub.aos',
          size_bytes: 1024,
          created_at: '2025-01-01T00:00:05.000Z',
        },
      ],
    },
    isLoading: false,
    refetch: vi.fn(),
  });

  const useCreateChatFromJob = () => ({
    mutate: vi.fn(),
    isPending: false,
  });

  return {
    useTraining: {
      useTrainingJob,
      useJobLogs,
      useJobMetrics,
      useJobArtifacts,
    },
    useCreateChatFromJob,
  };
});

vi.mock('sonner', () => {
  const toast = {
    success: vi.fn(),
    error: vi.fn(),
  };
  return { toast };
});

function renderWithQuery(ui: ReactElement) {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false, cacheTime: 0 },
      mutations: { retry: false },
    },
  });

  return render(
    <MemoryRouter>
      <QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>
    </MemoryRouter>
  );
}

function renderProgress(jobId: string) {
  return renderWithQuery(
    <Dialog open>
      <DialogContent>
        <TrainingProgress jobId={jobId} onClose={() => {}} />
      </DialogContent>
    </Dialog>,
  );
}

describe('TrainingProgress create and monitor (stub backend)', () => {
  beforeEach(() => {
    jobStore.reset();
    mockApiClient.downloadArtifact.mockResolvedValue(undefined);
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  it('polls stub backend and stops once the job completes', async () => {
    const deterministicConfig = {
      rank: 8,
      alpha: 16,
      epochs: 1,
      learning_rate: 0.0005,
      batch_size: 4,
      preferred_backend: 'test-stub',
    };

    const jobTimeline: TrainingJob[] = [
      {
        id: 'job-stub',
        adapter_name: 'tenant/test-stub/r001',
        status: 'pending',
        progress_pct: 0,
        created_at: '2025-01-01T00:00:00.000Z',
        config: deterministicConfig,
      },
      {
        id: 'job-stub',
        adapter_name: 'tenant/test-stub/r001',
        status: 'running',
        progress_pct: 35,
        started_at: '2025-01-01T00:00:02.000Z',
        config: deterministicConfig,
      },
      {
        id: 'job-stub',
        adapter_name: 'tenant/test-stub/r001',
        status: 'completed',
        progress_pct: 100,
        completed_at: '2025-01-01T00:00:04.000Z',
        adapter_id: 'adapter-stub',
        config: deterministicConfig,
      },
    ];

    jobStore.set(jobTimeline[0]);
    renderProgress('job-stub');
    expect((await screen.findAllByText(/pending/i)).length).toBeGreaterThan(0);

    act(() => jobStore.set(jobTimeline[1]));
    expect((await screen.findAllByText(/running/i)).length).toBeGreaterThan(0);

    act(() => jobStore.set(jobTimeline[2]));
    expect((await screen.findAllByText(/completed/i)).length).toBeGreaterThan(0);

    await userEvent.click(screen.getByRole('tab', { name: /artifacts/i }));
    expect(await screen.findByText('/artifacts/test-stub.aos')).toBeInTheDocument();
  }, 15000);

  it('shows failure error code and message', async () => {
    const failedJob: TrainingJob = {
      id: 'job-failed',
      adapter_name: 'tenant/failure/r001',
      status: 'failed',
      progress_pct: 0,
      created_at: '2025-01-01T00:00:00.000Z',
      error_category: 'policy',
      error_detail: 'POLICY::EgressDenied',
      error_message: 'Policy blocked training output',
    };

    jobStore.set(failedJob);
    renderProgress('job-failed');

    expect((await screen.findAllByText(/failed/i)).length).toBeGreaterThan(0);
    expect(screen.getByText(/failure \[policy\]/i)).toBeInTheDocument();
    expect(screen.getByText(/policy blocked training output/i)).toBeInTheDocument();
  }, 15000);
});
