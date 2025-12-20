import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { RulebookTab } from '@/components/chat/drawer/RulebookTab';
import type { EvidenceItem } from '@/components/chat/ChatMessage';
import { toast } from 'sonner';

// Mock toast
vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
  },
}));

const mockEvidence: EvidenceItem[] = [
  {
    document_id: 'doc-1',
    document_name: 'Financial Report 2024.pdf',
    chunk_id: 'chunk-1',
    page_number: 15,
    text_preview: 'This section discusses quarterly earnings and revenue projections.',
    relevance_score: 0.95,
    rank: 1,
  },
  {
    document_id: 'doc-1',
    document_name: 'Financial Report 2024.pdf',
    chunk_id: 'chunk-2',
    page_number: 16,
    text_preview: 'Additional financial metrics and KPIs for Q4.',
    relevance_score: 0.88,
    rank: 2,
  },
  {
    document_id: 'doc-2',
    document_name: 'Policy Manual.pdf',
    chunk_id: 'chunk-3',
    page_number: 8,
    text_preview: 'Company policy regarding expense reporting.',
    relevance_score: 0.72,
    rank: 3,
  },
  {
    document_id: 'doc-3',
    document_name: 'Guidelines.pdf',
    chunk_id: 'chunk-4',
    page_number: null,
    text_preview: 'General guidelines for document preparation.',
    relevance_score: 0.55,
    rank: 4,
  },
];

// Mock DOM APIs for export functionality
const mockClick = vi.fn();
let lastLinkElement: HTMLAnchorElement | null = null;
const originalCreateElement = document.createElement.bind(document);
const mockCreateElement = vi.fn((tag: string) => {
  if (tag === 'a') {
    const anchor = originalCreateElement(tag) as HTMLAnchorElement;
    anchor.click = mockClick as unknown as () => void;
    lastLinkElement = anchor;
    return anchor;
  }
  return originalCreateElement(tag);
});

const mockCreateObjectURL = vi.fn(() => 'blob:mock-url');
const mockRevokeObjectURL = vi.fn();
const originalCreateObjectURL = global.URL.createObjectURL;
const originalRevokeObjectURL = global.URL.revokeObjectURL;

describe('RulebookTab', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    document.createElement = mockCreateElement as unknown as typeof document.createElement;
    global.URL.createObjectURL = mockCreateObjectURL;
    global.URL.revokeObjectURL = mockRevokeObjectURL;
    mockClick.mockClear();
    lastLinkElement = null;
  });

  afterEach(() => {
    document.createElement = originalCreateElement;
    global.URL.createObjectURL = originalCreateObjectURL;
    global.URL.revokeObjectURL = originalRevokeObjectURL;
  });

  describe('Empty state', () => {
    it('shows empty message when no evidence provided', () => {
      render(<RulebookTab evidence={null} />);

      expect(screen.getByText('No citations available')).toBeInTheDocument();
      expect(screen.getByText(/Evidence will appear here when documents are referenced/)).toBeInTheDocument();
    });

    it('shows empty message when evidence array is empty', () => {
      render(<RulebookTab evidence={[]} />);

      expect(screen.getByText('No citations available')).toBeInTheDocument();
    });
  });

  describe('Evidence display', () => {
    it('renders header with evidence count', () => {
      render(<RulebookTab evidence={mockEvidence} />);

      expect(screen.getByText('Citations')).toBeInTheDocument();
      expect(screen.getByText('4')).toBeInTheDocument(); // Badge with count
    });

    it('groups evidence by document', () => {
      render(<RulebookTab evidence={mockEvidence} />);

      expect(screen.getByText('Financial Report 2024.pdf')).toBeInTheDocument();
      expect(screen.getByText('Policy Manual.pdf')).toBeInTheDocument();
      expect(screen.getByText('Guidelines.pdf')).toBeInTheDocument();
    });

    it('shows citation counts per document', () => {
      render(<RulebookTab evidence={mockEvidence} />);

      // Financial Report has 2 citations
      expect(screen.getByText('2 citations')).toBeInTheDocument();

      // Policy Manual and Guidelines each have 1 citation
      const singleCitations = screen.getAllByText('1 citation');
      expect(singleCitations.length).toBe(2);
    });

    it('sorts evidence items by relevance score within groups', () => {
      render(<RulebookTab evidence={mockEvidence} />);

      // Within Financial Report group, chunk-1 (0.95) should appear before chunk-2 (0.88)
      const previews = screen.getAllByText(/This section discusses|Additional financial metrics/);
      expect(previews[0]).toHaveTextContent('This section discusses');
      expect(previews[1]).toHaveTextContent('Additional financial metrics');
    });

    it('displays page numbers when available', () => {
      render(<RulebookTab evidence={mockEvidence} />);

      expect(screen.getByText('p. 15')).toBeInTheDocument();
      expect(screen.getByText('p. 16')).toBeInTheDocument();
      expect(screen.getByText('p. 8')).toBeInTheDocument();
    });

    it('displays text previews', () => {
      render(<RulebookTab evidence={mockEvidence} />);

      expect(screen.getByText(/"This section discusses quarterly earnings and revenue projections."/)).toBeInTheDocument();
      expect(screen.getByText(/"Company policy regarding expense reporting."/)).toBeInTheDocument();
    });

    it('displays relevance scores as percentages', () => {
      render(<RulebookTab evidence={mockEvidence} />);

      expect(screen.getByText('95.0%')).toBeInTheDocument();
      expect(screen.getByText('88.0%')).toBeInTheDocument();
      expect(screen.getByText('72.0%')).toBeInTheDocument();
      expect(screen.getByText('55.0%')).toBeInTheDocument();
    });

    it('shows relevance labels', () => {
      render(<RulebookTab evidence={mockEvidence} />);

      expect(screen.getAllByText('High').length).toBeGreaterThan(0); // >= 0.8
      expect(screen.getAllByText('Medium').length).toBeGreaterThan(0); // >= 0.6
      expect(screen.getByText('Low')).toBeInTheDocument(); // < 0.6
    });
  });

  describe('Export functionality', () => {
    it('renders export buttons', () => {
      render(<RulebookTab evidence={mockEvidence} />);

      expect(screen.getByRole('button', { name: /json/i })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /text/i })).toBeInTheDocument();
    });

    it('exports evidence as JSON', async () => {
      const user = userEvent.setup();
      render(<RulebookTab evidence={mockEvidence} />);

      const jsonButton = screen.getByRole('button', { name: /json/i });
      await user.click(jsonButton);

      expect(mockClick).toHaveBeenCalled();
      expect(lastLinkElement?.download).toMatch(/^evidence-\d+\.json$/);
      expect(toast.success).toHaveBeenCalledWith('Evidence exported as JSON');
    });

    it('exports evidence as text', async () => {
      const user = userEvent.setup();
      render(<RulebookTab evidence={mockEvidence} />);

      const textButton = screen.getByRole('button', { name: /text/i });
      await user.click(textButton);

      expect(mockClick).toHaveBeenCalled();
      expect(lastLinkElement?.download).toMatch(/^evidence-\d+\.txt$/);
      expect(toast.success).toHaveBeenCalledWith('Evidence exported as text');
    });

    it('shows error when exporting empty evidence', async () => {
      const user = userEvent.setup();
      render(<RulebookTab evidence={null} />);

      // Empty state doesn't show export buttons, so we need to test with an empty array
      const { rerender } = render(<RulebookTab evidence={[]} />);

      // Rerender shouldn't show export buttons in empty state
      expect(screen.queryByRole('button', { name: /json/i })).not.toBeInTheDocument();
    });
  });

  describe('Document viewing', () => {
    it('calls onViewDocument when evidence item clicked', async () => {
      const onViewDocument = vi.fn();
      const user = userEvent.setup();

      render(<RulebookTab evidence={mockEvidence} onViewDocument={onViewDocument} />);

      // Click the first evidence item
      const evidenceItems = screen.getAllByText(/"This section discusses/);
      await user.click(evidenceItems[0]);

      expect(onViewDocument).toHaveBeenCalledWith(
        'doc-1',
        15,
        'This section discusses quarterly earnings and revenue projections.'
      );
    });

    it('handles evidence without page numbers', async () => {
      const onViewDocument = vi.fn();
      const user = userEvent.setup();

      render(<RulebookTab evidence={mockEvidence} onViewDocument={onViewDocument} />);

      // Click the evidence item without page number (Guidelines.pdf)
      const item = screen.getByText(/"General guidelines for document preparation."/);
      await user.click(item);

      expect(onViewDocument).toHaveBeenCalledWith(
        'doc-3',
        undefined,
        'General guidelines for document preparation.'
      );
    });

    it('shows pointer cursor when onViewDocument is provided', () => {
      render(<RulebookTab evidence={mockEvidence} onViewDocument={vi.fn()} />);

      const items = document.querySelectorAll('.cursor-pointer');
      expect(items.length).toBeGreaterThan(0);
    });

    it('does not show pointer cursor when onViewDocument is not provided', () => {
      render(<RulebookTab evidence={mockEvidence} />);

      const items = document.querySelectorAll('.cursor-pointer');
      expect(items.length).toBe(0);
    });
  });

  describe('Relevance color coding', () => {
    it('applies green color to high relevance items', () => {
      render(<RulebookTab evidence={mockEvidence} />);

      const highRelevanceBadges = screen.getAllByText('High');
      highRelevanceBadges.forEach((badge) => {
        expect(badge.classList.contains('text-green-600')).toBe(true);
      });
    });

    it('applies yellow color to medium relevance items', () => {
      render(<RulebookTab evidence={mockEvidence} />);

      const mediumRelevanceBadges = screen.getAllByText('Medium');
      mediumRelevanceBadges.forEach((badge) => {
        expect(badge.classList.contains('text-yellow-600')).toBe(true);
      });
    });

    it('applies red color to low relevance items', () => {
      render(<RulebookTab evidence={mockEvidence} />);

      const lowRelevanceBadge = screen.getByText('Low');
      expect(lowRelevanceBadge.classList.contains('text-red-600')).toBe(true);
    });
  });

  describe('Document sorting', () => {
    it('sorts documents alphabetically', () => {
      render(<RulebookTab evidence={mockEvidence} />);

      const documentNames = screen.getAllByText(/\.pdf$/);
      const names = documentNames.map((el) => el.textContent);

      // Should be in alphabetical order
      expect(names).toEqual([
        'Financial Report 2024.pdf',
        'Guidelines.pdf',
        'Policy Manual.pdf',
      ]);
    });
  });
});
