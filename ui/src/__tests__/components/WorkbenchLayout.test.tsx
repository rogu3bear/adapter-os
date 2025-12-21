/**
 * Tests for WorkbenchLayout component
 *
 * Tests layout rendering, right rail collapse behavior, and slot composition.
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import { WorkbenchLayout } from '@/components/workbench/WorkbenchLayout';
import { WorkbenchProvider } from '@/contexts/WorkbenchContext';

function renderWithProvider(ui: React.ReactElement) {
  return render(<WorkbenchProvider>{ui}</WorkbenchProvider>);
}

describe('WorkbenchLayout', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  describe('Rendering', () => {
    it('renders three-column layout', () => {
      renderWithProvider(
        <WorkbenchLayout
          leftRail={<div data-testid="left-content">Left Rail</div>}
          center={<div data-testid="center-content">Center</div>}
          rightRail={<div data-testid="right-content">Right Rail</div>}
        />
      );

      expect(screen.getByTestId('workbench-layout')).toBeInTheDocument();
      expect(screen.getByTestId('workbench-left-rail')).toBeInTheDocument();
      expect(screen.getByTestId('workbench-center')).toBeInTheDocument();
      expect(screen.getByTestId('workbench-right-rail')).toBeInTheDocument();
    });

    it('renders left rail content', () => {
      renderWithProvider(
        <WorkbenchLayout
          leftRail={<div>Sessions Tab Content</div>}
          center={<div>Chat</div>}
          rightRail={<div>Evidence</div>}
        />
      );

      expect(screen.getByText('Sessions Tab Content')).toBeInTheDocument();
    });

    it('renders center content', () => {
      renderWithProvider(
        <WorkbenchLayout
          leftRail={<div>Left</div>}
          center={<div>Chat Interface Content</div>}
          rightRail={<div>Right</div>}
        />
      );

      expect(screen.getByText('Chat Interface Content')).toBeInTheDocument();
    });

    it('renders right rail content', () => {
      renderWithProvider(
        <WorkbenchLayout
          leftRail={<div>Left</div>}
          center={<div>Center</div>}
          rightRail={<div>Evidence Panel Content</div>}
        />
      );

      expect(screen.getByText('Evidence Panel Content')).toBeInTheDocument();
    });

    it('renders optional top bar when provided', () => {
      renderWithProvider(
        <WorkbenchLayout
          leftRail={<div>Left</div>}
          center={<div>Center</div>}
          rightRail={<div>Right</div>}
          topBar={<div data-testid="top-bar">Top Bar Content</div>}
        />
      );

      expect(screen.getByTestId('top-bar')).toBeInTheDocument();
      expect(screen.getByText('Top Bar Content')).toBeInTheDocument();
    });

    it('does not render top bar when not provided', () => {
      renderWithProvider(
        <WorkbenchLayout
          leftRail={<div>Left</div>}
          center={<div>Center</div>}
          rightRail={<div>Right</div>}
        />
      );

      expect(screen.queryByText('Top Bar Content')).not.toBeInTheDocument();
    });

    it('applies custom className when provided', () => {
      renderWithProvider(
        <WorkbenchLayout
          leftRail={<div>Left</div>}
          center={<div>Center</div>}
          rightRail={<div>Right</div>}
          className="custom-class"
        />
      );

      const layout = screen.getByTestId('workbench-layout');
      expect(layout).toHaveClass('custom-class');
    });
  });

  describe('Right Rail Collapse Behavior', () => {
    it('shows right rail expanded by default', () => {
      renderWithProvider(
        <WorkbenchLayout
          leftRail={<div>Left</div>}
          center={<div>Center</div>}
          rightRail={<div>Right Content</div>}
        />
      );

      const rightRail = screen.getByTestId('workbench-right-rail');
      expect(rightRail).not.toHaveClass('w-0');
      expect(screen.getByText('Right Content')).toBeInTheDocument();
    });

    it('collapses right rail when state is collapsed', () => {
      // Pre-set localStorage to collapsed state
      localStorage.setItem('workbench:rightRail:collapsed', 'true');

      renderWithProvider(
        <WorkbenchLayout
          leftRail={<div>Left</div>}
          center={<div>Center</div>}
          rightRail={<div>Right Content</div>}
        />
      );

      const rightRail = screen.getByTestId('workbench-right-rail');
      expect(rightRail).toHaveClass('w-0');
    });
  });

  describe('Layout Structure', () => {
    it('has correct fixed width for left rail', () => {
      renderWithProvider(
        <WorkbenchLayout
          leftRail={<div>Left</div>}
          center={<div>Center</div>}
          rightRail={<div>Right</div>}
        />
      );

      const leftRail = screen.getByTestId('workbench-left-rail');
      expect(leftRail).toHaveClass('w-80');
      expect(leftRail).toHaveClass('flex-none');
    });

    it('has flexible width for center', () => {
      renderWithProvider(
        <WorkbenchLayout
          leftRail={<div>Left</div>}
          center={<div>Center</div>}
          rightRail={<div>Right</div>}
        />
      );

      const center = screen.getByTestId('workbench-center');
      expect(center).toHaveClass('flex-1');
      expect(center).toHaveClass('min-w-0');
    });

    it('has overflow hidden for proper scrolling', () => {
      renderWithProvider(
        <WorkbenchLayout
          leftRail={<div>Left</div>}
          center={<div>Center</div>}
          rightRail={<div>Right</div>}
        />
      );

      const leftRail = screen.getByTestId('workbench-left-rail');
      const center = screen.getByTestId('workbench-center');
      const rightRail = screen.getByTestId('workbench-right-rail');

      expect(leftRail).toHaveClass('overflow-hidden');
      expect(center).toHaveClass('overflow-hidden');
      expect(rightRail).toHaveClass('overflow-hidden');
    });
  });

  describe('Slot Composition', () => {
    it('supports complex component trees in slots', () => {
      renderWithProvider(
        <WorkbenchLayout
          leftRail={
            <div>
              <header>Left Header</header>
              <nav>Left Nav</nav>
              <main>Left Main</main>
            </div>
          }
          center={
            <div>
              <div>Center Top</div>
              <div>Center Bottom</div>
            </div>
          }
          rightRail={
            <div>
              <section>Right Section 1</section>
              <section>Right Section 2</section>
            </div>
          }
        />
      );

      expect(screen.getByText('Left Header')).toBeInTheDocument();
      expect(screen.getByText('Left Nav')).toBeInTheDocument();
      expect(screen.getByText('Center Top')).toBeInTheDocument();
      expect(screen.getByText('Right Section 1')).toBeInTheDocument();
    });

    it('handles empty content in slots', () => {
      renderWithProvider(
        <WorkbenchLayout
          leftRail={<div />}
          center={<div />}
          rightRail={<div />}
        />
      );

      expect(screen.getByTestId('workbench-layout')).toBeInTheDocument();
      expect(screen.getByTestId('workbench-left-rail')).toBeInTheDocument();
      expect(screen.getByTestId('workbench-center')).toBeInTheDocument();
      expect(screen.getByTestId('workbench-right-rail')).toBeInTheDocument();
    });
  });

  describe('Accessibility', () => {
    it('uses semantic HTML elements', () => {
      renderWithProvider(
        <WorkbenchLayout
          leftRail={<div>Left</div>}
          center={<div>Center</div>}
          rightRail={<div>Right</div>}
        />
      );

      const leftRail = screen.getByTestId('workbench-left-rail');
      const center = screen.getByTestId('workbench-center');
      const rightRail = screen.getByTestId('workbench-right-rail');

      expect(leftRail.tagName).toBe('ASIDE');
      expect(center.tagName).toBe('MAIN');
      expect(rightRail.tagName).toBe('ASIDE');
    });
  });
});
