import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ExportDialog } from '@/components/export/ExportDialog';
import type { ExportFormat } from '@/utils/export/types';

describe('ExportDialog', () => {
  const mockOnOpenChange = vi.fn();
  const mockOnExport = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    mockOnExport.mockResolvedValue(undefined);
  });

  it('renders dialog when open', () => {
    render(
      <ExportDialog
        open={true}
        onOpenChange={mockOnOpenChange}
        onExport={mockOnExport}
      />
    );

    expect(screen.getByTestId('export-dialog')).toBeInTheDocument();
    expect(screen.getByText('Export Chat Session')).toBeInTheDocument();
  });

  it('does not render when closed', () => {
    render(
      <ExportDialog
        open={false}
        onOpenChange={mockOnOpenChange}
        onExport={mockOnExport}
      />
    );

    expect(screen.queryByTestId('export-dialog')).not.toBeInTheDocument();
  });

  it('displays custom title', () => {
    render(
      <ExportDialog
        open={true}
        onOpenChange={mockOnOpenChange}
        onExport={mockOnExport}
        title="Export Dataset"
      />
    );

    expect(screen.getByText('Export Dataset')).toBeInTheDocument();
  });

  it('shows all available format options', () => {
    render(
      <ExportDialog
        open={true}
        onOpenChange={mockOnOpenChange}
        onExport={mockOnExport}
        availableFormats={['markdown', 'json', 'pdf']}
      />
    );

    expect(screen.getByText('Markdown')).toBeInTheDocument();
    expect(screen.getByText('JSON')).toBeInTheDocument();
    expect(screen.getByText('PDF')).toBeInTheDocument();
  });

  it('includes evidence bundle option when enabled', () => {
    render(
      <ExportDialog
        open={true}
        onOpenChange={mockOnOpenChange}
        onExport={mockOnExport}
        includeEvidenceBundle={true}
      />
    );

    expect(screen.getByText('Evidence Bundle')).toBeInTheDocument();
  });

  it('does not show evidence bundle by default', () => {
    render(
      <ExportDialog
        open={true}
        onOpenChange={mockOnOpenChange}
        onExport={mockOnExport}
      />
    );

    expect(screen.queryByText('Evidence Bundle')).not.toBeInTheDocument();
  });

  it('allows selecting different export formats', async () => {
    const user = userEvent.setup();
    render(
      <ExportDialog
        open={true}
        onOpenChange={mockOnOpenChange}
        onExport={mockOnExport}
      />
    );

    const jsonOption = screen.getByLabelText(/JSON/);
    await user.click(jsonOption);

    expect(jsonOption).toBeChecked();
  });

  it('displays message count', () => {
    render(
      <ExportDialog
        open={true}
        onOpenChange={mockOnOpenChange}
        onExport={mockOnExport}
        messageCount={42}
      />
    );

    expect(screen.getByText('Messages')).toBeInTheDocument();
    expect(screen.getByText('42')).toBeInTheDocument();
  });

  it('displays evidence count when provided', () => {
    render(
      <ExportDialog
        open={true}
        onOpenChange={mockOnOpenChange}
        onExport={mockOnExport}
        evidenceCount={15}
      />
    );

    expect(screen.getByText(/Evidence citations/)).toBeInTheDocument();
    expect(screen.getByText('15')).toBeInTheDocument();
  });

  it('displays trace count when provided', () => {
    render(
      <ExportDialog
        open={true}
        onOpenChange={mockOnOpenChange}
        onExport={mockOnExport}
        traceCount={10}
      />
    );

    expect(screen.getByText('Trace records')).toBeInTheDocument();
    expect(screen.getByText('10')).toBeInTheDocument();
  });

  it('shows verified determinism badge', () => {
    render(
      <ExportDialog
        open={true}
        onOpenChange={mockOnOpenChange}
        onExport={mockOnExport}
        determinismState="verified"
      />
    );

    expect(screen.getByText('VERIFIED')).toBeInTheDocument();
  });

  it('shows unverified determinism badge', () => {
    render(
      <ExportDialog
        open={true}
        onOpenChange={mockOnOpenChange}
        onExport={mockOnExport}
        determinismState="unverified"
      />
    );

    expect(screen.getByText('UNVERIFIED')).toBeInTheDocument();
  });

  it('shows approximate determinism badge', () => {
    render(
      <ExportDialog
        open={true}
        onOpenChange={mockOnOpenChange}
        onExport={mockOnExport}
        determinismState="approximate"
      />
    );

    expect(screen.getByText('APPROXIMATE')).toBeInTheDocument();
  });

  it('calls onExport with selected format when export button clicked', async () => {
    const user = userEvent.setup();
    render(
      <ExportDialog
        open={true}
        onOpenChange={mockOnOpenChange}
        onExport={mockOnExport}
      />
    );

    const exportButton = screen.getByTestId('export-confirm');
    await user.click(exportButton);

    expect(mockOnExport).toHaveBeenCalledWith('markdown');
  });

  it('calls onExport with correct format after changing selection', async () => {
    const user = userEvent.setup();
    render(
      <ExportDialog
        open={true}
        onOpenChange={mockOnOpenChange}
        onExport={mockOnExport}
      />
    );

    const jsonOption = screen.getByLabelText(/JSON/);
    await user.click(jsonOption);

    const exportButton = screen.getByTestId('export-confirm');
    await user.click(exportButton);

    expect(mockOnExport).toHaveBeenCalledWith('json');
  });

  it('closes dialog after successful export', async () => {
    const user = userEvent.setup();
    render(
      <ExportDialog
        open={true}
        onOpenChange={mockOnOpenChange}
        onExport={mockOnExport}
      />
    );

    const exportButton = screen.getByTestId('export-confirm');
    await user.click(exportButton);

    await waitFor(() => {
      expect(mockOnOpenChange).toHaveBeenCalledWith(false);
    });
  });

  it('shows loading state during export', async () => {
    const user = userEvent.setup();
    mockOnExport.mockImplementation(
      () => new Promise((resolve) => setTimeout(resolve, 100))
    );

    render(
      <ExportDialog
        open={true}
        onOpenChange={mockOnOpenChange}
        onExport={mockOnExport}
      />
    );

    const exportButton = screen.getByTestId('export-confirm');
    await user.click(exportButton);

    expect(screen.getByText('Exporting...')).toBeInTheDocument();
    expect(exportButton).toBeDisabled();
  });

  it('disables buttons during export', async () => {
    const user = userEvent.setup();
    mockOnExport.mockImplementation(
      () => new Promise((resolve) => setTimeout(resolve, 100))
    );

    render(
      <ExportDialog
        open={true}
        onOpenChange={mockOnOpenChange}
        onExport={mockOnExport}
      />
    );

    const exportButton = screen.getByTestId('export-confirm');
    const cancelButton = screen.getByText('Cancel');

    await user.click(exportButton);

    expect(exportButton).toBeDisabled();
    expect(cancelButton).toBeDisabled();
  });

  it('re-enables button after error (component has try-finally)', async () => {
    const user = userEvent.setup();

    // Note: The component uses try-finally (not try-catch-finally)
    // So errors will propagate but button will still be re-enabled
    mockOnExport.mockRejectedValue(new Error('Export failed'));

    render(
      <ExportDialog
        open={true}
        onOpenChange={mockOnOpenChange}
        onExport={mockOnExport}
      />
    );

    const exportButton = screen.getByTestId('export-confirm');

    // Start the click - it will trigger export
    const clickPromise = user.click(exportButton);

    // Wait for the button to be re-enabled (finally block)
    await waitFor(
      () => {
        expect(exportButton).not.toBeDisabled();
      },
      { timeout: 2000 }
    );

    // Note: Dialog doesn't close on error because onOpenChange(false)
    // is only called in try block, not in finally
    expect(mockOnOpenChange).not.toHaveBeenCalledWith(false);
  });

  it('calls onOpenChange when cancel button clicked', async () => {
    const user = userEvent.setup();
    render(
      <ExportDialog
        open={true}
        onOpenChange={mockOnOpenChange}
        onExport={mockOnExport}
      />
    );

    const cancelButton = screen.getByText('Cancel');
    await user.click(cancelButton);

    expect(mockOnOpenChange).toHaveBeenCalledWith(false);
  });

  it('displays file extension for each format', () => {
    render(
      <ExportDialog
        open={true}
        onOpenChange={mockOnOpenChange}
        onExport={mockOnExport}
      />
    );

    expect(screen.getByText('(.md)')).toBeInTheDocument();
    expect(screen.getByText('(.json)')).toBeInTheDocument();
    expect(screen.getByText('(.pdf)')).toBeInTheDocument();
  });

  it('highlights selected format with visual styling', async () => {
    const user = userEvent.setup();
    render(
      <ExportDialog
        open={true}
        onOpenChange={mockOnOpenChange}
        onExport={mockOnExport}
      />
    );

    const jsonOption = screen.getByLabelText(/JSON/);
    await user.click(jsonOption);

    const jsonContainer = jsonOption.closest('div');
    expect(jsonContainer).toHaveClass('border-primary');
  });

  it('allows clicking on format container to select', async () => {
    const user = userEvent.setup();
    render(
      <ExportDialog
        open={true}
        onOpenChange={mockOnOpenChange}
        onExport={mockOnExport}
      />
    );

    const jsonLabel = screen.getByText('JSON');
    const jsonContainer = jsonLabel.closest('[class*="rounded-md"]') as HTMLElement;

    await user.click(jsonContainer);

    const jsonRadio = screen.getByLabelText(/JSON/) as HTMLInputElement;
    expect(jsonRadio).toBeChecked();
  });

  it('shows bbox info for markdown format in evidence count', () => {
    render(
      <ExportDialog
        open={true}
        onOpenChange={mockOnOpenChange}
        onExport={mockOnExport}
        evidenceCount={5}
      />
    );

    // Default is markdown, should show bbox note
    expect(screen.getByText(/Evidence citations.*with bbox/)).toBeInTheDocument();
  });

  it('hides bbox info for non-markdown formats', async () => {
    const user = userEvent.setup();
    render(
      <ExportDialog
        open={true}
        onOpenChange={mockOnOpenChange}
        onExport={mockOnExport}
        evidenceCount={5}
      />
    );

    const jsonOption = screen.getByLabelText(/JSON/);
    await user.click(jsonOption);

    expect(screen.queryByText(/with bbox/)).not.toBeInTheDocument();
  });
});
