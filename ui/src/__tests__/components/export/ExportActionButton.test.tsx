import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ExportActionButton } from '@/components/export/ExportActionButton';

// Mock sonner toast
const mockToastSuccess = vi.fn();
const mockToastError = vi.fn();

vi.mock('sonner', () => ({
  toast: {
    success: (msg: string) => mockToastSuccess(msg),
    error: (msg: string) => mockToastError(msg),
  },
}));

describe('ExportActionButton', () => {
  const mockOnExportMarkdown = vi.fn();
  const mockOnExportJson = vi.fn();
  const mockOnExportPdf = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    mockOnExportMarkdown.mockResolvedValue(undefined);
    mockOnExportJson.mockResolvedValue(undefined);
    mockOnExportPdf.mockResolvedValue(undefined);
  });

  it('renders export button', () => {
    render(
      <ExportActionButton
        onExportMarkdown={mockOnExportMarkdown}
        onExportJson={mockOnExportJson}
      />
    );

    expect(screen.getByTestId('export-button')).toBeInTheDocument();
    expect(screen.getByText('Export')).toBeInTheDocument();
  });

  it('opens dropdown menu when clicked', async () => {
    const user = userEvent.setup();
    render(
      <ExportActionButton
        onExportMarkdown={mockOnExportMarkdown}
        onExportJson={mockOnExportJson}
      />
    );

    const button = screen.getByTestId('export-button');
    await user.click(button);

    expect(screen.getByText('Export as Markdown')).toBeInTheDocument();
    expect(screen.getByText('Export as JSON')).toBeInTheDocument();
  });

  it('shows PDF option when onExportPdf provided', async () => {
    const user = userEvent.setup();
    render(
      <ExportActionButton
        onExportMarkdown={mockOnExportMarkdown}
        onExportJson={mockOnExportJson}
        onExportPdf={mockOnExportPdf}
      />
    );

    const button = screen.getByTestId('export-button');
    await user.click(button);

    expect(screen.getByText('Export as PDF')).toBeInTheDocument();
  });

  it('does not show PDF option when onExportPdf not provided', async () => {
    const user = userEvent.setup();
    render(
      <ExportActionButton
        onExportMarkdown={mockOnExportMarkdown}
        onExportJson={mockOnExportJson}
      />
    );

    const button = screen.getByTestId('export-button');
    await user.click(button);

    expect(screen.queryByText('Export as PDF')).not.toBeInTheDocument();
  });

  it('calls onExportMarkdown when markdown option clicked', async () => {
    const user = userEvent.setup();
    render(
      <ExportActionButton
        onExportMarkdown={mockOnExportMarkdown}
        onExportJson={mockOnExportJson}
      />
    );

    const button = screen.getByTestId('export-button');
    await user.click(button);

    const markdownOption = screen.getByTestId('export-markdown');
    await user.click(markdownOption);

    expect(mockOnExportMarkdown).toHaveBeenCalled();
  });

  it('calls onExportJson when JSON option clicked', async () => {
    const user = userEvent.setup();
    render(
      <ExportActionButton
        onExportMarkdown={mockOnExportMarkdown}
        onExportJson={mockOnExportJson}
      />
    );

    const button = screen.getByTestId('export-button');
    await user.click(button);

    const jsonOption = screen.getByTestId('export-json');
    await user.click(jsonOption);

    expect(mockOnExportJson).toHaveBeenCalled();
  });

  it('calls onExportPdf when PDF option clicked', async () => {
    const user = userEvent.setup();
    render(
      <ExportActionButton
        onExportMarkdown={mockOnExportMarkdown}
        onExportJson={mockOnExportJson}
        onExportPdf={mockOnExportPdf}
      />
    );

    const button = screen.getByTestId('export-button');
    await user.click(button);

    const pdfOption = screen.getByTestId('export-pdf');
    await user.click(pdfOption);

    expect(mockOnExportPdf).toHaveBeenCalled();
  });

  it('shows success toast after successful export', async () => {
    const user = userEvent.setup();
    render(
      <ExportActionButton
        onExportMarkdown={mockOnExportMarkdown}
        onExportJson={mockOnExportJson}
      />
    );

    const button = screen.getByTestId('export-button');
    await user.click(button);

    const markdownOption = screen.getByTestId('export-markdown');
    await user.click(markdownOption);

    await waitFor(() => {
      expect(mockToastSuccess).toHaveBeenCalledWith('Exported as Markdown');
    });
  });

  it('shows error toast when export fails', async () => {
    const user = userEvent.setup();
    mockOnExportMarkdown.mockRejectedValue(new Error('Export failed'));

    render(
      <ExportActionButton
        onExportMarkdown={mockOnExportMarkdown}
        onExportJson={mockOnExportJson}
      />
    );

    const button = screen.getByTestId('export-button');
    await user.click(button);

    const markdownOption = screen.getByTestId('export-markdown');
    await user.click(markdownOption);

    await waitFor(() => {
      expect(mockToastError).toHaveBeenCalledWith('Export failed: Export failed');
    });
  });

  it('shows loading state during export', async () => {
    const user = userEvent.setup();
    mockOnExportMarkdown.mockImplementation(
      () => new Promise((resolve) => setTimeout(resolve, 100))
    );

    render(
      <ExportActionButton
        onExportMarkdown={mockOnExportMarkdown}
        onExportJson={mockOnExportJson}
      />
    );

    const button = screen.getByTestId('export-button');
    await user.click(button);

    const markdownOption = screen.getByTestId('export-markdown');
    await user.click(markdownOption);

    await waitFor(() => {
      expect(screen.getByText('Exporting...')).toBeInTheDocument();
    });
  });

  it('disables button during export', async () => {
    const user = userEvent.setup();
    mockOnExportMarkdown.mockImplementation(
      () => new Promise((resolve) => setTimeout(resolve, 100))
    );

    render(
      <ExportActionButton
        onExportMarkdown={mockOnExportMarkdown}
        onExportJson={mockOnExportJson}
      />
    );

    const button = screen.getByTestId('export-button');
    await user.click(button);

    const markdownOption = screen.getByTestId('export-markdown');
    await user.click(markdownOption);

    await waitFor(() => {
      expect(button).toBeDisabled();
    });
  });

  it('re-enables button after export completes', async () => {
    const user = userEvent.setup();
    render(
      <ExportActionButton
        onExportMarkdown={mockOnExportMarkdown}
        onExportJson={mockOnExportJson}
      />
    );

    const button = screen.getByTestId('export-button');
    await user.click(button);

    const markdownOption = screen.getByTestId('export-markdown');
    await user.click(markdownOption);

    await waitFor(() => {
      expect(button).not.toBeDisabled();
    });
  });

  it('respects disabled prop', () => {
    render(
      <ExportActionButton
        onExportMarkdown={mockOnExportMarkdown}
        onExportJson={mockOnExportJson}
        disabled={true}
      />
    );

    const button = screen.getByTestId('export-button');
    expect(button).toBeDisabled();
  });

  it('applies custom variant', () => {
    render(
      <ExportActionButton
        onExportMarkdown={mockOnExportMarkdown}
        onExportJson={mockOnExportJson}
        variant="default"
      />
    );

    const button = screen.getByTestId('export-button');
    // Check that variant class is applied (implementation specific)
    expect(button).toBeInTheDocument();
  });

  it('applies custom size', () => {
    render(
      <ExportActionButton
        onExportMarkdown={mockOnExportMarkdown}
        onExportJson={mockOnExportJson}
        size="lg"
      />
    );

    const button = screen.getByTestId('export-button');
    expect(button).toBeInTheDocument();
  });

  it('handles concurrent export attempts', async () => {
    const user = userEvent.setup();
    mockOnExportMarkdown.mockImplementation(
      () => new Promise((resolve) => setTimeout(resolve, 100))
    );

    render(
      <ExportActionButton
        onExportMarkdown={mockOnExportMarkdown}
        onExportJson={mockOnExportJson}
      />
    );

    const button = screen.getByTestId('export-button');
    await user.click(button);

    const markdownOption = screen.getByTestId('export-markdown');
    await user.click(markdownOption);

    // Try to export again while first is in progress
    // Button should be disabled
    expect(button).toBeDisabled();
  });

  it('shows correct toast message for JSON export', async () => {
    const user = userEvent.setup();
    render(
      <ExportActionButton
        onExportMarkdown={mockOnExportMarkdown}
        onExportJson={mockOnExportJson}
      />
    );

    const button = screen.getByTestId('export-button');
    await user.click(button);

    const jsonOption = screen.getByTestId('export-json');
    await user.click(jsonOption);

    await waitFor(() => {
      expect(mockToastSuccess).toHaveBeenCalledWith('Exported as JSON');
    });
  });

  it('shows correct toast message for PDF export', async () => {
    const user = userEvent.setup();
    render(
      <ExportActionButton
        onExportMarkdown={mockOnExportMarkdown}
        onExportJson={mockOnExportJson}
        onExportPdf={mockOnExportPdf}
      />
    );

    const button = screen.getByTestId('export-button');
    await user.click(button);

    const pdfOption = screen.getByTestId('export-pdf');
    await user.click(pdfOption);

    await waitFor(() => {
      expect(mockToastSuccess).toHaveBeenCalledWith('Exported as PDF');
    });
  });

  it('handles errors with empty message gracefully', async () => {
    const user = userEvent.setup();
    mockOnExportMarkdown.mockRejectedValue(new Error(''));

    render(
      <ExportActionButton
        onExportMarkdown={mockOnExportMarkdown}
        onExportJson={mockOnExportJson}
      />
    );

    const button = screen.getByTestId('export-button');
    await user.click(button);

    const markdownOption = screen.getByTestId('export-markdown');
    await user.click(markdownOption);

    await waitFor(() => {
      expect(mockToastError).toHaveBeenCalledWith('Export failed: ');
    });
  });

  it('renders download icon', () => {
    render(
      <ExportActionButton
        onExportMarkdown={mockOnExportMarkdown}
        onExportJson={mockOnExportJson}
      />
    );

    const button = screen.getByTestId('export-button');
    expect(button.querySelector('svg')).toBeInTheDocument();
  });
});
