/**
 * Tests for WorkbenchTopBar component
 *
 * Tests rendering of status chips, export button, and user interactions.
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { WorkbenchTopBar } from '@/components/workbench/WorkbenchTopBar';
import { WorkbenchProvider } from '@/contexts/WorkbenchContext';
import { DatasetChatProvider } from '@/contexts/DatasetChatContext';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

// Mock the hooks that child components use
vi.mock('@/hooks/useTraining', () => ({
  useTraining: {
    useDataset: vi.fn(() => ({
      data: null,
      isLoading: false,
    })),
  },
}));

function createQueryClient() {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });
}

function renderWithProviders(ui: React.ReactElement) {
  const queryClient = createQueryClient();
  return render(
    <QueryClientProvider client={queryClient}>
      <WorkbenchProvider>
        <DatasetChatProvider>{ui}</DatasetChatProvider>
      </WorkbenchProvider>
    </QueryClientProvider>
  );
}

describe('WorkbenchTopBar', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
  });

  describe('Rendering', () => {
    it('renders the top bar container', () => {
      renderWithProviders(<WorkbenchTopBar />);

      expect(screen.getByTestId('workbench-top-bar')).toBeInTheDocument();
    });

    it('renders without crashing when no props provided', () => {
      renderWithProviders(<WorkbenchTopBar />);

      const topBar = screen.getByTestId('workbench-top-bar');
      expect(topBar).toBeInTheDocument();
    });

    it('applies custom className when provided', () => {
      renderWithProviders(<WorkbenchTopBar className="custom-class" />);

      const topBar = screen.getByTestId('workbench-top-bar');
      expect(topBar).toHaveClass('custom-class');
    });
  });

  describe('Active Stack Chip', () => {
    it('renders stack chip when stack is active', () => {
      renderWithProviders(
        <WorkbenchTopBar stackName="Test Stack" stackId="stack-123" />
      );

      expect(screen.getByTestId('active-stack-chip')).toBeInTheDocument();
      expect(screen.getByText('Test Stack')).toBeInTheDocument();
    });

    it('does not render stack chip when no stack provided', () => {
      renderWithProviders(<WorkbenchTopBar />);

      expect(screen.queryByTestId('active-stack-chip')).not.toBeInTheDocument();
    });

    it('does not render stack chip when only stackName provided', () => {
      renderWithProviders(<WorkbenchTopBar stackName="Test Stack" />);

      expect(screen.queryByTestId('active-stack-chip')).not.toBeInTheDocument();
    });

    it('does not render stack chip when only stackId provided', () => {
      renderWithProviders(<WorkbenchTopBar stackId="stack-123" />);

      expect(screen.queryByTestId('active-stack-chip')).not.toBeInTheDocument();
    });
  });

  describe('Export Button', () => {
    it('renders export button when onExport provided', () => {
      const onExport = vi.fn();

      renderWithProviders(<WorkbenchTopBar onExport={onExport} />);

      expect(screen.getByTestId('export-button')).toBeInTheDocument();
      expect(screen.getByText('Export')).toBeInTheDocument();
    });

    it('does not render export button when onExport not provided', () => {
      renderWithProviders(<WorkbenchTopBar />);

      expect(screen.queryByTestId('export-button')).not.toBeInTheDocument();
    });

    it('calls onExport when button is clicked', async () => {
      const user = userEvent.setup();
      const onExport = vi.fn();

      renderWithProviders(
        <WorkbenchTopBar onExport={onExport} canExport={true} />
      );

      const exportButton = screen.getByTestId('export-button');
      await user.click(exportButton);

      expect(onExport).toHaveBeenCalledTimes(1);
    });

    it('disables export button when canExport is false', () => {
      const onExport = vi.fn();

      renderWithProviders(
        <WorkbenchTopBar onExport={onExport} canExport={false} />
      );

      const exportButton = screen.getByTestId('export-button');
      expect(exportButton).toBeDisabled();
    });

    it('enables export button when canExport is true', () => {
      const onExport = vi.fn();

      renderWithProviders(
        <WorkbenchTopBar onExport={onExport} canExport={true} />
      );

      const exportButton = screen.getByTestId('export-button');
      expect(exportButton).not.toBeDisabled();
    });

    it('defaults canExport to false', () => {
      const onExport = vi.fn();

      renderWithProviders(<WorkbenchTopBar onExport={onExport} />);

      const exportButton = screen.getByTestId('export-button');
      expect(exportButton).toBeDisabled();
    });

    it('does not call onExport when button is disabled', async () => {
      const user = userEvent.setup();
      const onExport = vi.fn();

      renderWithProviders(
        <WorkbenchTopBar onExport={onExport} canExport={false} />
      );

      const exportButton = screen.getByTestId('export-button');
      await user.click(exportButton);

      expect(onExport).not.toHaveBeenCalled();
    });

    it('renders Download icon in export button', () => {
      const onExport = vi.fn();

      renderWithProviders(<WorkbenchTopBar onExport={onExport} />);

      const exportButton = screen.getByTestId('export-button');
      const icon = exportButton.querySelector('svg');
      expect(icon).toBeInTheDocument();
    });
  });

  describe('Layout', () => {
    it('arranges chips on left and export button on right', () => {
      const onExport = vi.fn();

      renderWithProviders(
        <WorkbenchTopBar
          stackName="My Stack"
          stackId="stack-1"
          onExport={onExport}
          canExport={true}
        />
      );

      const topBar = screen.getByTestId('workbench-top-bar');
      expect(topBar).toHaveClass('justify-between');
    });

    it('maintains layout with only stack chip', () => {
      renderWithProviders(
        <WorkbenchTopBar stackName="My Stack" stackId="stack-1" />
      );

      const topBar = screen.getByTestId('workbench-top-bar');
      expect(topBar).toBeInTheDocument();
    });

    it('maintains layout with only export button', () => {
      const onExport = vi.fn();

      renderWithProviders(<WorkbenchTopBar onExport={onExport} />);

      const topBar = screen.getByTestId('workbench-top-bar');
      expect(topBar).toBeInTheDocument();
    });
  });

  describe('Integration', () => {
    it('renders all elements together', () => {
      const onExport = vi.fn();

      renderWithProviders(
        <WorkbenchTopBar
          stackName="Production Stack"
          stackId="stack-prod"
          onExport={onExport}
          canExport={true}
        />
      );

      expect(screen.getByTestId('workbench-top-bar')).toBeInTheDocument();
      expect(screen.getByTestId('active-stack-chip')).toBeInTheDocument();
      expect(screen.getByTestId('export-button')).toBeInTheDocument();
      expect(screen.getByText('Production Stack')).toBeInTheDocument();
    });

    it('handles stack name changes', () => {
      const onExport = vi.fn();

      const { rerender } = renderWithProviders(
        <WorkbenchTopBar
          stackName="Stack V1"
          stackId="stack-1"
          onExport={onExport}
        />
      );

      expect(screen.getByText('Stack V1')).toBeInTheDocument();

      const queryClient = createQueryClient();
      rerender(
        <QueryClientProvider client={queryClient}>
          <WorkbenchProvider>
            <DatasetChatProvider>
              <WorkbenchTopBar
                stackName="Stack V2"
                stackId="stack-1"
                onExport={onExport}
              />
            </DatasetChatProvider>
          </WorkbenchProvider>
        </QueryClientProvider>
      );

      expect(screen.getByText('Stack V2')).toBeInTheDocument();
      expect(screen.queryByText('Stack V1')).not.toBeInTheDocument();
    });

    it('handles canExport toggle', () => {
      const onExport = vi.fn();

      const { rerender } = renderWithProviders(
        <WorkbenchTopBar onExport={onExport} canExport={false} />
      );

      let exportButton = screen.getByTestId('export-button');
      expect(exportButton).toBeDisabled();

      const queryClient = createQueryClient();
      rerender(
        <QueryClientProvider client={queryClient}>
          <WorkbenchProvider>
            <DatasetChatProvider>
              <WorkbenchTopBar onExport={onExport} canExport={true} />
            </DatasetChatProvider>
          </WorkbenchProvider>
        </QueryClientProvider>
      );

      exportButton = screen.getByTestId('export-button');
      expect(exportButton).not.toBeDisabled();
    });
  });
});
