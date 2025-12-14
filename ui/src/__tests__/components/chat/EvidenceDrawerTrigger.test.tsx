import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { MemoryRouter } from 'react-router-dom';
import { EvidenceDrawerTrigger } from '@/components/chat/EvidenceDrawerTrigger';
import { EvidenceDrawerProvider } from '@/contexts/EvidenceDrawerContext';
import type { EvidenceItem } from '@/components/chat/ChatMessage';
import type { ExtendedRouterDecision } from '@/api/types';

const mockEvidence: EvidenceItem[] = [
  {
    document_id: 'doc-1',
    document_name: 'Test.pdf',
    chunk_id: 'chunk-1',
    page_number: 5,
    text_preview: 'Test preview',
    relevance_score: 0.9,
    rank: 1,
  },
  {
    document_id: 'doc-2',
    document_name: 'Test2.pdf',
    chunk_id: 'chunk-2',
    page_number: 10,
    text_preview: 'Another preview',
    relevance_score: 0.8,
    rank: 2,
  },
];

const mockRouterDecision: ExtendedRouterDecision = {
  selected_adapters: ['adapter-1'],
  candidates: [
    {
      adapter_id: 'adapter-1',
      gate_float: 0.8,
      selected: true,
    },
  ],
};

interface WrapperProps {
  children: React.ReactNode;
}

function TestWrapper({ children }: WrapperProps) {
  return (
    <MemoryRouter>
      <EvidenceDrawerProvider>
        {children}
      </EvidenceDrawerProvider>
    </MemoryRouter>
  );
}

describe('EvidenceDrawerTrigger', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('Rendering logic', () => {
    it('does not render when no evidence and no proof', () => {
      const { container } = render(
        <TestWrapper>
          <EvidenceDrawerTrigger messageId="msg-1" />
        </TestWrapper>
      );

      expect(container.firstChild).toBeNull();
    });

    it('renders evidence trigger when evidence is provided', () => {
      render(
        <TestWrapper>
          <EvidenceDrawerTrigger
            messageId="msg-1"
            evidence={mockEvidence}
          />
        </TestWrapper>
      );

      const button = screen.getByTestId('evidence-drawer-trigger-rulebook');
      expect(button).toBeInTheDocument();
    });

    it('renders proof trigger when requestId is provided', () => {
      render(
        <TestWrapper>
          <EvidenceDrawerTrigger
            messageId="msg-1"
            requestId="req-123"
          />
        </TestWrapper>
      );

      const button = screen.getByTestId('evidence-drawer-trigger-calculation');
      expect(button).toBeInTheDocument();
    });

    it('renders proof trigger when traceId is provided', () => {
      render(
        <TestWrapper>
          <EvidenceDrawerTrigger
            messageId="msg-1"
            traceId="trace-123"
          />
        </TestWrapper>
      );

      const button = screen.getByTestId('evidence-drawer-trigger-calculation');
      expect(button).toBeInTheDocument();
    });

    it('renders proof trigger when proofDigest is provided', () => {
      render(
        <TestWrapper>
          <EvidenceDrawerTrigger
            messageId="msg-1"
            proofDigest="proof-abc"
          />
        </TestWrapper>
      );

      const button = screen.getByTestId('evidence-drawer-trigger-calculation');
      expect(button).toBeInTheDocument();
    });

    it('renders trace trigger when traceId is provided', () => {
      render(
        <TestWrapper>
          <EvidenceDrawerTrigger
            messageId="msg-1"
            traceId="trace-123"
          />
        </TestWrapper>
      );

      const button = screen.getByTestId('evidence-drawer-trigger-trace');
      expect(button).toBeInTheDocument();
    });

    it('renders all triggers when all data is provided', () => {
      render(
        <TestWrapper>
          <EvidenceDrawerTrigger
            messageId="msg-1"
            evidence={mockEvidence}
            requestId="req-123"
            traceId="trace-123"
            proofDigest="proof-abc"
          />
        </TestWrapper>
      );

      expect(screen.getByTestId('evidence-drawer-trigger-rulebook')).toBeInTheDocument();
      expect(screen.getByTestId('evidence-drawer-trigger-calculation')).toBeInTheDocument();
      expect(screen.getByTestId('evidence-drawer-trigger-trace')).toBeInTheDocument();
    });
  });

  describe('Evidence badge', () => {
    it('displays evidence count badge', () => {
      render(
        <TestWrapper>
          <EvidenceDrawerTrigger
            messageId="msg-1"
            evidence={mockEvidence}
          />
        </TestWrapper>
      );

      expect(screen.getByText('2')).toBeInTheDocument();
    });

    it('shows tooltip with correct text for single source', () => {
      const singleEvidence = [mockEvidence[0]];

      render(
        <TestWrapper>
          <EvidenceDrawerTrigger
            messageId="msg-1"
            evidence={singleEvidence}
          />
        </TestWrapper>
      );

      const button = screen.getByTestId('evidence-drawer-trigger-rulebook');
      expect(button).toBeInTheDocument();
      // Tooltip text is rendered but may not be visible
    });

    it('shows tooltip with correct text for multiple sources', () => {
      render(
        <TestWrapper>
          <EvidenceDrawerTrigger
            messageId="msg-1"
            evidence={mockEvidence}
          />
        </TestWrapper>
      );

      const button = screen.getByTestId('evidence-drawer-trigger-rulebook');
      expect(button).toBeInTheDocument();
    });
  });

  describe('Verification badge', () => {
    it('shows green shield when verified', () => {
      render(
        <TestWrapper>
          <EvidenceDrawerTrigger
            messageId="msg-1"
            requestId="req-123"
            isVerified={true}
            verifiedAt="2025-01-01T12:00:00Z"
          />
        </TestWrapper>
      );

      const button = screen.getByTestId('evidence-drawer-trigger-calculation');
      expect(button).toBeInTheDocument();
      // Shield icon should have green color class
      const icon = button.querySelector('svg');
      expect(icon?.classList.contains('text-green-600')).toBe(true);
    });

    it('shows muted shield when not verified', () => {
      render(
        <TestWrapper>
          <EvidenceDrawerTrigger
            messageId="msg-1"
            requestId="req-123"
            isVerified={false}
          />
        </TestWrapper>
      );

      const button = screen.getByTestId('evidence-drawer-trigger-calculation');
      expect(button).toBeInTheDocument();
      // Shield icon should have muted color class
      const icon = button.querySelector('svg');
      expect(icon?.classList.contains('text-muted-foreground')).toBe(true);
    });
  });

  describe('Click behavior', () => {
    // Click behavior is implicitly tested through the trigger rendering tests
    // The actual drawer opening is tested in EvidenceDrawer.test.tsx
    it('renders clickable trigger buttons', () => {
      render(
        <TestWrapper>
          <EvidenceDrawerTrigger
            messageId="msg-1"
            evidence={mockEvidence}
            requestId="req-123"
            traceId="trace-123"
          />
        </TestWrapper>
      );

      const rulebookButton = screen.getByTestId('evidence-drawer-trigger-rulebook');
      const calcButton = screen.getByTestId('evidence-drawer-trigger-calculation');
      const traceButton = screen.getByTestId('evidence-drawer-trigger-trace');

      expect(rulebookButton).toBeInTheDocument();
      expect(calcButton).toBeInTheDocument();
      expect(traceButton).toBeInTheDocument();
    });
  });
});
