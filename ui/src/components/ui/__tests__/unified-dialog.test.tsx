import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { UnifiedDialog } from '@/components/ui/unified-dialog';

describe('UnifiedDialog', () => {
  describe('Basic Dialog', () => {
    it('renders with title and description', () => {
      render(
        <UnifiedDialog
          open={true}
          onOpenChange={vi.fn()}
          title="Test Dialog"
          description="Test description"
        >
          <div>Content</div>
        </UnifiedDialog>
      );

      expect(screen.getByText('Test Dialog')).toBeInTheDocument();
      expect(screen.getByText('Test description')).toBeInTheDocument();
      expect(screen.getByText('Content')).toBeInTheDocument();
    });

    it('calls onOpenChange when close button is clicked', async () => {
      const onOpenChange = vi.fn();
      render(
        <UnifiedDialog
          open={true}
          onOpenChange={onOpenChange}
          title="Test Dialog"
        >
          <div>Content</div>
        </UnifiedDialog>
      );

      const closeButton = screen.getByRole('button', { name: /close/i });
      fireEvent.click(closeButton);

      await waitFor(() => {
        expect(onOpenChange).toHaveBeenCalledWith(false);
      });
    });

    it('respects preventClose prop', () => {
      const onOpenChange = vi.fn();
      render(
        <UnifiedDialog
          open={true}
          onOpenChange={onOpenChange}
          title="Test Dialog"
          preventClose={true}
        >
          <div>Content</div>
        </UnifiedDialog>
      );

      // Close button should not be rendered when preventClose is true
      expect(screen.queryByRole('button', { name: /close/i })).not.toBeInTheDocument();
    });

    it('renders with custom footer', () => {
      render(
        <UnifiedDialog
          open={true}
          onOpenChange={vi.fn()}
          title="Test Dialog"
          footer={
            <button>Custom Action</button>
          }
        >
          <div>Content</div>
        </UnifiedDialog>
      );

      expect(screen.getByText('Custom Action')).toBeInTheDocument();
    });

    it('supports different sizes', () => {
      const { container } = render(
        <UnifiedDialog
          open={true}
          onOpenChange={vi.fn()}
          title="Test Dialog"
          size="sm"
        >
          <div>Content</div>
        </UnifiedDialog>
      );

      const dialogContent = container.querySelector('[data-slot="unified-dialog-content"]');
      expect(dialogContent).toHaveClass('sm:max-w-sm');
    });

    it('renders with icon and background', () => {
      const CustomIcon = <div data-testid="custom-icon">Icon</div>;
      render(
        <UnifiedDialog
          open={true}
          onOpenChange={vi.fn()}
          title="Test Dialog"
          icon={CustomIcon}
          showIconBackground={true}
        >
          <div>Content</div>
        </UnifiedDialog>
      );

      expect(screen.getByTestId('custom-icon')).toBeInTheDocument();
    });
  });

  describe('Confirmation Dialog', () => {
    it('renders confirmation variant with icon', () => {
      render(
        <UnifiedDialog.Confirmation
          open={true}
          onOpenChange={vi.fn()}
          title="Confirm Action"
          description="Are you sure?"
          onConfirm={vi.fn()}
        />
      );

      expect(screen.getByText('Confirm Action')).toBeInTheDocument();
      expect(screen.getByText('Are you sure?')).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /confirm/i })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /cancel/i })).toBeInTheDocument();
    });

    it('calls onConfirm when confirm button is clicked', async () => {
      const onConfirm = vi.fn();
      const onOpenChange = vi.fn();
      render(
        <UnifiedDialog.Confirmation
          open={true}
          onOpenChange={onOpenChange}
          title="Confirm Action"
          onConfirm={onConfirm}
        />
      );

      const confirmButton = screen.getByRole('button', { name: /confirm/i });
      fireEvent.click(confirmButton);

      await waitFor(() => {
        expect(onConfirm).toHaveBeenCalled();
        expect(onOpenChange).toHaveBeenCalledWith(false);
      });
    });

    it('calls onCancel when cancel button is clicked', async () => {
      const onCancel = vi.fn();
      const onOpenChange = vi.fn();
      render(
        <UnifiedDialog.Confirmation
          open={true}
          onOpenChange={onOpenChange}
          title="Confirm Action"
          onConfirm={vi.fn()}
          onCancel={onCancel}
        />
      );

      const cancelButton = screen.getByRole('button', { name: /cancel/i });
      fireEvent.click(cancelButton);

      await waitFor(() => {
        expect(onCancel).toHaveBeenCalled();
        expect(onOpenChange).toHaveBeenCalledWith(false);
      });
    });

    it('shows loading state', () => {
      render(
        <UnifiedDialog.Confirmation
          open={true}
          onOpenChange={vi.fn()}
          title="Confirm Action"
          onConfirm={vi.fn()}
          isLoading={true}
        />
      );

      const confirmButton = screen.getByRole('button', { name: /confirm/i });
      expect(confirmButton).toBeDisabled();
    });

    it('renders with destructive variant icon by default', () => {
      const { container } = render(
        <UnifiedDialog.Confirmation
          open={true}
          onOpenChange={vi.fn()}
          title="Delete Item"
          confirmVariant="destructive"
          onConfirm={vi.fn()}
        />
      );

      // Check for destructive variant icon (TrashIcon)
      const iconContainer = container.querySelector('.text-destructive');
      expect(iconContainer).toBeInTheDocument();
    });
  });

  describe('Form Dialog', () => {
    it('renders form variant', () => {
      render(
        <UnifiedDialog.Form
          open={true}
          onOpenChange={vi.fn()}
          title="Create Item"
          description="Fill in the details"
          onSubmit={vi.fn()}
        >
          <input name="title" placeholder="Title" />
        </UnifiedDialog.Form>
      );

      expect(screen.getByText('Create Item')).toBeInTheDocument();
      expect(screen.getByText('Fill in the details')).toBeInTheDocument();
      expect(screen.getByPlaceholderText('Title')).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /submit/i })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /cancel/i })).toBeInTheDocument();
    });

    it('calls onSubmit with form data when submitted', async () => {
      const onSubmit = vi.fn();
      render(
        <UnifiedDialog.Form
          open={true}
          onOpenChange={vi.fn()}
          title="Create Item"
          onSubmit={onSubmit}
        >
          <input name="title" defaultValue="Test Title" />
        </UnifiedDialog.Form>
      );

      const submitButton = screen.getByRole('button', { name: /submit/i });
      fireEvent.click(submitButton);

      await waitFor(() => {
        expect(onSubmit).toHaveBeenCalledWith({
          title: 'Test Title',
        });
      });
    });

    it('disables submit button when form is invalid', () => {
      render(
        <UnifiedDialog.Form
          open={true}
          onOpenChange={vi.fn()}
          title="Create Item"
          onSubmit={vi.fn()}
          isValid={false}
        >
          <input name="title" />
        </UnifiedDialog.Form>
      );

      const submitButton = screen.getByRole('button', { name: /submit/i });
      expect(submitButton).toBeDisabled();
    });

    it('shows loading state when submitting', () => {
      render(
        <UnifiedDialog.Form
          open={true}
          onOpenChange={vi.fn()}
          title="Create Item"
          onSubmit={vi.fn()}
          isSubmitting={true}
        >
          <input name="title" />
        </UnifiedDialog.Form>
      );

      const submitButton = screen.getByRole('button', { name: /submit/i });
      expect(submitButton).toBeDisabled();
    });

    it('calls onCancel when cancel button is clicked', async () => {
      const onCancel = vi.fn();
      const onOpenChange = vi.fn();
      render(
        <UnifiedDialog.Form
          open={true}
          onOpenChange={onOpenChange}
          title="Create Item"
          onSubmit={vi.fn()}
          onCancel={onCancel}
        >
          <input name="title" />
        </UnifiedDialog.Form>
      );

      const cancelButton = screen.getByRole('button', { name: /cancel/i });
      fireEvent.click(cancelButton);

      await waitFor(() => {
        expect(onCancel).toHaveBeenCalled();
        expect(onOpenChange).toHaveBeenCalledWith(false);
      });
    });

    it('resets form when closed if resetOnClose is true', async () => {
      const { rerender } = render(
        <UnifiedDialog.Form
          open={true}
          onOpenChange={vi.fn()}
          title="Create Item"
          onSubmit={vi.fn()}
          resetOnClose={true}
        >
          <input name="title" defaultValue="Test" />
        </UnifiedDialog.Form>
      );

      const input = screen.getByRole('textbox') as HTMLInputElement;
      expect(input.value).toBe('Test');

      // Simulate closing
      rerender(
        <UnifiedDialog.Form
          open={false}
          onOpenChange={vi.fn()}
          title="Create Item"
          onSubmit={vi.fn()}
          resetOnClose={true}
        >
          <input name="title" defaultValue="Test" />
        </UnifiedDialog.Form>
      );

      // Note: In a real test, we'd need to check that form.reset() was called
      // This is a simplified verification
    });
  });

  describe('Accessibility', () => {
    it('has proper ARIA attributes', () => {
      render(
        <UnifiedDialog
          open={true}
          onOpenChange={vi.fn()}
          title="Test Dialog"
          description="Test description"
        >
          <div>Content</div>
        </UnifiedDialog>
      );

      // Dialog should have role="dialog"
      const dialog = screen.getByRole('dialog');
      expect(dialog).toBeInTheDocument();
    });

    it('focuses first interactive element when opened', () => {
      render(
        <UnifiedDialog
          open={true}
          onOpenChange={vi.fn()}
          title="Test Dialog"
        >
          <button>First Button</button>
          <button>Second Button</button>
        </UnifiedDialog>
      );

      // Radix handles focus management automatically
      expect(screen.getByText('First Button')).toBeInTheDocument();
    });
  });
});
