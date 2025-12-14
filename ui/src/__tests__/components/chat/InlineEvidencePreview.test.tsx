import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { InlineEvidencePreview } from '@/components/chat/InlineEvidencePreview';
import type { EvidenceItem } from '@/components/chat/ChatMessage';

const mockEvidence: EvidenceItem[] = [
  {
    document_id: 'doc-1',
    document_name: 'Financial Report 2024.pdf',
    chunk_id: 'chunk-1',
    page_number: 15,
    text_preview: 'High relevance text',
    relevance_score: 0.95,
    rank: 1,
  },
  {
    document_id: 'doc-2',
    document_name: 'Policy Manual.pdf',
    chunk_id: 'chunk-2',
    page_number: 8,
    text_preview: 'Medium relevance text',
    relevance_score: 0.72,
    rank: 2,
  },
  {
    document_id: 'doc-3',
    document_name: 'Guidelines.pdf',
    chunk_id: 'chunk-3',
    page_number: 42,
    text_preview: 'Lower relevance text',
    relevance_score: 0.55,
    rank: 3,
  },
  {
    document_id: 'doc-4',
    document_name: 'Extra Doc.pdf',
    chunk_id: 'chunk-4',
    page_number: 5,
    text_preview: 'Additional text',
    relevance_score: 0.65,
    rank: 4,
  },
  {
    document_id: 'doc-5',
    document_name: 'Another Doc.pdf',
    chunk_id: 'chunk-5',
    page_number: null,
    text_preview: 'Fifth item',
    relevance_score: 0.50,
    rank: 5,
  },
];

describe('InlineEvidencePreview', () => {
  let onViewAllMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    onViewAllMock = vi.fn();
    vi.clearAllMocks();
  });

  describe('Rendering logic', () => {
    it('does not render when evidence is empty', () => {
      const { container } = render(
        <InlineEvidencePreview
          messageId="msg-1"
          evidence={[]}
          onViewAll={onViewAllMock}
        />
      );

      expect(container.firstChild).toBeNull();
    });

    it('does not render when evidence is null', () => {
      const { container } = render(
        <InlineEvidencePreview
          messageId="msg-1"
          evidence={null as any}
          onViewAll={onViewAllMock}
        />
      );

      expect(container.firstChild).toBeNull();
    });

    it('renders with evidence items', () => {
      render(
        <InlineEvidencePreview
          messageId="msg-1"
          evidence={mockEvidence}
          onViewAll={onViewAllMock}
        />
      );

      expect(screen.getByTestId('inline-evidence-preview-msg-1')).toBeInTheDocument();
    });
  });

  describe('Item display', () => {
    it('shows top 3 items by default', () => {
      render(
        <InlineEvidencePreview
          messageId="msg-1"
          evidence={mockEvidence}
          onViewAll={onViewAllMock}
        />
      );

      // Should show top 3 by relevance score (sorted descending)
      expect(screen.getByText('Financial Report 2024.pdf')).toBeInTheDocument();
      expect(screen.getByText('Policy Manual.pdf')).toBeInTheDocument();
      expect(screen.getByText('Guidelines.pdf')).toBeInTheDocument();

      // Fourth and fifth should not be visible inline
      expect(screen.queryByText('Extra Doc.pdf')).not.toBeInTheDocument();
      expect(screen.queryByText('Another Doc.pdf')).not.toBeInTheDocument();
    });

    it('respects custom maxItems prop', () => {
      render(
        <InlineEvidencePreview
          messageId="msg-1"
          evidence={mockEvidence}
          maxItems={2}
          onViewAll={onViewAllMock}
        />
      );

      // Should only show top 2
      expect(screen.getByText('Financial Report 2024.pdf')).toBeInTheDocument();
      expect(screen.getByText('Policy Manual.pdf')).toBeInTheDocument();
      expect(screen.queryByText('Guidelines.pdf')).not.toBeInTheDocument();
    });

    it('sorts evidence by relevance score descending', () => {
      // Mix up the order
      const unsortedEvidence = [mockEvidence[2], mockEvidence[0], mockEvidence[1]];

      render(
        <InlineEvidencePreview
          messageId="msg-1"
          evidence={unsortedEvidence}
          maxItems={3}
          onViewAll={onViewAllMock}
        />
      );

      // Should still display in correct order (highest relevance first)
      const items = screen.getAllByRole('button');
      expect(items[0]).toHaveTextContent('Financial Report 2024.pdf');
      expect(items[1]).toHaveTextContent('Policy Manual.pdf');
      expect(items[2]).toHaveTextContent('Guidelines.pdf');
    });

    it('displays page numbers when available', () => {
      render(
        <InlineEvidencePreview
          messageId="msg-1"
          evidence={mockEvidence.slice(0, 1)}
          onViewAll={onViewAllMock}
        />
      );

      expect(screen.getByText('p.15')).toBeInTheDocument();
    });

    it('does not display page number when null', () => {
      const evidenceWithoutPage = [mockEvidence[4]]; // Last item has null page_number

      render(
        <InlineEvidencePreview
          messageId="msg-1"
          evidence={evidenceWithoutPage}
          onViewAll={onViewAllMock}
        />
      );

      expect(screen.queryByText(/p\./)).not.toBeInTheDocument();
    });
  });

  describe('Relevance score badges', () => {
    it('shows green badge for high relevance (>= 0.8)', () => {
      render(
        <InlineEvidencePreview
          messageId="msg-1"
          evidence={[mockEvidence[0]]} // 0.95 score
          onViewAll={onViewAllMock}
        />
      );

      const badge = screen.getByText('95%');
      expect(badge).toBeInTheDocument();
      expect(badge.classList.contains('bg-green-100')).toBe(true);
      expect(badge.classList.contains('text-green-700')).toBe(true);
    });

    it('shows yellow badge for medium relevance (>= 0.6, < 0.8)', () => {
      render(
        <InlineEvidencePreview
          messageId="msg-1"
          evidence={[mockEvidence[1]]} // 0.72 score
          onViewAll={onViewAllMock}
        />
      );

      const badge = screen.getByText('72%');
      expect(badge).toBeInTheDocument();
      expect(badge.classList.contains('bg-yellow-100')).toBe(true);
      expect(badge.classList.contains('text-yellow-700')).toBe(true);
    });

    it('shows red badge for low relevance (< 0.6)', () => {
      render(
        <InlineEvidencePreview
          messageId="msg-1"
          evidence={[mockEvidence[2]]} // 0.55 score
          onViewAll={onViewAllMock}
        />
      );

      const badge = screen.getByText('55%');
      expect(badge).toBeInTheDocument();
      expect(badge.classList.contains('bg-red-100')).toBe(true);
      expect(badge.classList.contains('text-red-700')).toBe(true);
    });

    it('rounds relevance scores correctly', () => {
      render(
        <InlineEvidencePreview
          messageId="msg-1"
          evidence={mockEvidence.slice(0, 3)}
          onViewAll={onViewAllMock}
        />
      );

      expect(screen.getByText('95%')).toBeInTheDocument();
      expect(screen.getByText('72%')).toBeInTheDocument();
      expect(screen.getByText('55%')).toBeInTheDocument();
    });
  });

  describe('View All button', () => {
    it('shows "View all" button when more items exist', () => {
      render(
        <InlineEvidencePreview
          messageId="msg-1"
          evidence={mockEvidence} // 5 items, default maxItems is 3
          onViewAll={onViewAllMock}
        />
      );

      const viewAllButton = screen.getByTestId('view-all-evidence');
      expect(viewAllButton).toBeInTheDocument();
      expect(viewAllButton).toHaveTextContent('View all 5 sources');
    });

    it('does not show "View all" button when items fit', () => {
      render(
        <InlineEvidencePreview
          messageId="msg-1"
          evidence={mockEvidence.slice(0, 2)} // Only 2 items
          maxItems={3}
          onViewAll={onViewAllMock}
        />
      );

      expect(screen.queryByTestId('view-all-evidence')).not.toBeInTheDocument();
    });

    it('does not show "View all" button when exactly at limit', () => {
      render(
        <InlineEvidencePreview
          messageId="msg-1"
          evidence={mockEvidence.slice(0, 3)} // Exactly 3 items
          maxItems={3}
          onViewAll={onViewAllMock}
        />
      );

      expect(screen.queryByTestId('view-all-evidence')).not.toBeInTheDocument();
    });

    it('calls onViewAll when "View all" button clicked', async () => {
      const user = userEvent.setup();

      render(
        <InlineEvidencePreview
          messageId="msg-1"
          evidence={mockEvidence}
          onViewAll={onViewAllMock}
        />
      );

      const viewAllButton = screen.getByTestId('view-all-evidence');
      await user.click(viewAllButton);

      expect(onViewAllMock).toHaveBeenCalledTimes(1);
    });
  });

  describe('Item interaction', () => {
    it('calls onViewAll when evidence item clicked', async () => {
      const user = userEvent.setup();

      render(
        <InlineEvidencePreview
          messageId="msg-1"
          evidence={mockEvidence.slice(0, 1)}
          onViewAll={onViewAllMock}
        />
      );

      const item = screen.getByRole('button');
      await user.click(item);

      expect(onViewAllMock).toHaveBeenCalledTimes(1);
    });

    it('calls onViewAll when Enter key pressed on evidence item', async () => {
      const user = userEvent.setup();

      render(
        <InlineEvidencePreview
          messageId="msg-1"
          evidence={mockEvidence.slice(0, 1)}
          onViewAll={onViewAllMock}
        />
      );

      const item = screen.getByRole('button');
      item.focus();
      await user.keyboard('{Enter}');

      expect(onViewAllMock).toHaveBeenCalledTimes(1);
    });

    it('calls onViewAll when Space key pressed on evidence item', async () => {
      const user = userEvent.setup();

      render(
        <InlineEvidencePreview
          messageId="msg-1"
          evidence={mockEvidence.slice(0, 1)}
          onViewAll={onViewAllMock}
        />
      );

      const item = screen.getByRole('button');
      item.focus();
      await user.keyboard(' ');

      expect(onViewAllMock).toHaveBeenCalledTimes(1);
    });

    it('is keyboard accessible with tabIndex', () => {
      render(
        <InlineEvidencePreview
          messageId="msg-1"
          evidence={mockEvidence.slice(0, 1)}
          onViewAll={onViewAllMock}
        />
      );

      const item = screen.getByRole('button');
      expect(item).toHaveAttribute('tabIndex', '0');
    });
  });

  describe('Custom className', () => {
    it('applies custom className', () => {
      render(
        <InlineEvidencePreview
          messageId="msg-1"
          evidence={mockEvidence.slice(0, 1)}
          onViewAll={onViewAllMock}
          className="custom-class"
        />
      );

      const container = screen.getByTestId('inline-evidence-preview-msg-1');
      expect(container.classList.contains('custom-class')).toBe(true);
    });
  });
});
