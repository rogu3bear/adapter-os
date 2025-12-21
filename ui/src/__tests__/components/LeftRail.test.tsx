/**
 * Tests for LeftRail component
 *
 * Tests tab navigation, scroll position preservation, and content rendering.
 */

import React from 'react';
import { describe, it, expect, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { LeftRail } from '@/components/workbench/left-rail/LeftRail';
import { WorkbenchProvider, useWorkbench } from '@/contexts/WorkbenchContext';
import { renderHook, act } from '@testing-library/react';

function renderWithProvider(ui: React.ReactElement) {
  return render(<WorkbenchProvider>{ui}</WorkbenchProvider>);
}

describe('LeftRail', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  describe('Rendering', () => {
    it('renders the left rail container', () => {
      renderWithProvider(
        <LeftRail
          sessionsContent={<div>Sessions</div>}
          datasetsContent={<div>Datasets</div>}
          stacksContent={<div>Stacks</div>}
        />
      );

      expect(screen.getByTestId('left-rail')).toBeInTheDocument();
    });

    it('renders tab navigation', () => {
      renderWithProvider(
        <LeftRail
          sessionsContent={<div>Sessions</div>}
          datasetsContent={<div>Datasets</div>}
          stacksContent={<div>Stacks</div>}
        />
      );

      expect(screen.getByRole('tab', { name: /sessions/i })).toBeInTheDocument();
      expect(screen.getByRole('tab', { name: /datasets/i })).toBeInTheDocument();
      expect(screen.getByRole('tab', { name: /stacks/i })).toBeInTheDocument();
    });

    it('shows sessions content by default', () => {
      renderWithProvider(
        <LeftRail
          sessionsContent={<div>Sessions Content</div>}
          datasetsContent={<div>Datasets Content</div>}
          stacksContent={<div>Stacks Content</div>}
        />
      );

      const sessionsPanel = screen.getByTestId('sessions-panel');
      const datasetsPanel = screen.getByTestId('datasets-panel');
      const stacksPanel = screen.getByTestId('stacks-panel');

      expect(sessionsPanel).not.toHaveClass('hidden');
      expect(datasetsPanel).toHaveClass('hidden');
      expect(stacksPanel).toHaveClass('hidden');
    });
  });

  describe('Tab Switching', () => {
    it('switches to datasets tab when clicked', async () => {
      const user = userEvent.setup();

      renderWithProvider(
        <LeftRail
          sessionsContent={<div>Sessions Content</div>}
          datasetsContent={<div>Datasets Content</div>}
          stacksContent={<div>Stacks Content</div>}
        />
      );

      const datasetsTab = screen.getByRole('tab', { name: /datasets/i });
      await user.click(datasetsTab);

      const sessionsPanel = screen.getByTestId('sessions-panel');
      const datasetsPanel = screen.getByTestId('datasets-panel');

      expect(datasetsPanel).not.toHaveClass('hidden');
      expect(sessionsPanel).toHaveClass('hidden');
    });

    it('switches to stacks tab when clicked', async () => {
      const user = userEvent.setup();

      renderWithProvider(
        <LeftRail
          sessionsContent={<div>Sessions Content</div>}
          datasetsContent={<div>Datasets Content</div>}
          stacksContent={<div>Stacks Content</div>}
        />
      );

      const stacksTab = screen.getByRole('tab', { name: /stacks/i });
      await user.click(stacksTab);

      const sessionsPanel = screen.getByTestId('sessions-panel');
      const stacksPanel = screen.getByTestId('stacks-panel');

      expect(stacksPanel).not.toHaveClass('hidden');
      expect(sessionsPanel).toHaveClass('hidden');
    });

    it('switches between multiple tabs', async () => {
      const user = userEvent.setup();

      renderWithProvider(
        <LeftRail
          sessionsContent={<div>Sessions Content</div>}
          datasetsContent={<div>Datasets Content</div>}
          stacksContent={<div>Stacks Content</div>}
        />
      );

      const sessionsPanel = screen.getByTestId('sessions-panel');
      const datasetsPanel = screen.getByTestId('datasets-panel');
      const stacksPanel = screen.getByTestId('stacks-panel');

      // Switch to datasets
      await user.click(screen.getByRole('tab', { name: /datasets/i }));
      expect(datasetsPanel).not.toHaveClass('hidden');

      // Switch to stacks
      await user.click(screen.getByRole('tab', { name: /stacks/i }));
      expect(stacksPanel).not.toHaveClass('hidden');

      // Switch back to sessions
      await user.click(screen.getByRole('tab', { name: /sessions/i }));
      expect(sessionsPanel).not.toHaveClass('hidden');
    });

    it('persists active tab to localStorage', async () => {
      const user = userEvent.setup();

      renderWithProvider(
        <LeftRail
          sessionsContent={<div>Sessions</div>}
          datasetsContent={<div>Datasets</div>}
          stacksContent={<div>Stacks</div>}
        />
      );

      await user.click(screen.getByRole('tab', { name: /datasets/i }));

      expect(localStorage.getItem('workbench:leftRail:activeTab')).toBe('datasets');
    });
  });

  describe('Tab Panels', () => {
    it('renders all tab panels in DOM', () => {
      renderWithProvider(
        <LeftRail
          sessionsContent={<div>Sessions Content</div>}
          datasetsContent={<div>Datasets Content</div>}
          stacksContent={<div>Stacks Content</div>}
        />
      );

      expect(screen.getByTestId('sessions-panel')).toBeInTheDocument();
      expect(screen.getByTestId('datasets-panel')).toBeInTheDocument();
      expect(screen.getByTestId('stacks-panel')).toBeInTheDocument();
    });

    it('shows only active panel content', async () => {
      const user = userEvent.setup();

      renderWithProvider(
        <LeftRail
          sessionsContent={<div>Sessions Content</div>}
          datasetsContent={<div>Datasets Content</div>}
          stacksContent={<div>Stacks Content</div>}
        />
      );

      const sessionsPanel = screen.getByTestId('sessions-panel');
      const datasetsPanel = screen.getByTestId('datasets-panel');
      const stacksPanel = screen.getByTestId('stacks-panel');

      // Initially sessions is shown
      expect(sessionsPanel).not.toHaveClass('hidden');
      expect(datasetsPanel).toHaveClass('hidden');
      expect(stacksPanel).toHaveClass('hidden');

      // Switch to datasets
      await user.click(screen.getByRole('tab', { name: /datasets/i }));

      expect(sessionsPanel).toHaveClass('hidden');
      expect(datasetsPanel).not.toHaveClass('hidden');
      expect(stacksPanel).toHaveClass('hidden');
    });

    it('applies proper ARIA attributes to panels', () => {
      renderWithProvider(
        <LeftRail
          sessionsContent={<div>Sessions</div>}
          datasetsContent={<div>Datasets</div>}
          stacksContent={<div>Stacks</div>}
        />
      );

      const sessionsPanel = screen.getByTestId('sessions-panel');
      expect(sessionsPanel).toHaveAttribute('role', 'tabpanel');
      expect(sessionsPanel).toHaveAttribute('aria-labelledby', 'tab-sessions');
    });
  });

  describe('Scroll Position Management', () => {
    it('saves scroll position when scrolling', () => {
      renderWithProvider(
        <LeftRail
          sessionsContent={
            <div style={{ height: '2000px' }}>Tall Sessions Content</div>
          }
          datasetsContent={<div>Datasets</div>}
          stacksContent={<div>Stacks</div>}
        />
      );

      const sessionsPanel = screen.getByTestId('sessions-panel');

      // Simulate scroll
      fireEvent.scroll(sessionsPanel, { target: { scrollTop: 100 } });

      const stored = JSON.parse(
        localStorage.getItem('workbench:leftRail:scrollPositions') || '{}'
      );
      expect(stored.sessions).toBe(100);
    });

    it('restores scroll position when switching tabs', async () => {
      const user = userEvent.setup();

      const TestComponent = () => {
        const { saveScrollPosition } = useWorkbench();

        // Pre-save scroll position
        React.useEffect(() => {
          saveScrollPosition('datasets', 250);
        }, [saveScrollPosition]);

        return (
          <LeftRail
            sessionsContent={<div>Sessions</div>}
            datasetsContent={
              <div style={{ height: '2000px' }}>Datasets Content</div>
            }
            stacksContent={<div>Stacks</div>}
          />
        );
      };

      renderWithProvider(<TestComponent />);

      // Switch to datasets tab
      await user.click(screen.getByRole('tab', { name: /datasets/i }));

      const datasetsPanel = screen.getByTestId('datasets-panel');
      expect(datasetsPanel.scrollTop).toBe(250);
    });

    it('maintains independent scroll positions for each tab', async () => {
      const user = userEvent.setup();

      renderWithProvider(
        <LeftRail
          sessionsContent={
            <div style={{ height: '2000px' }}>Sessions Content</div>
          }
          datasetsContent={
            <div style={{ height: '2000px' }}>Datasets Content</div>
          }
          stacksContent={
            <div style={{ height: '2000px' }}>Stacks Content</div>
          }
        />
      );

      // Scroll sessions
      const sessionsPanel = screen.getByTestId('sessions-panel');
      fireEvent.scroll(sessionsPanel, { target: { scrollTop: 100 } });

      // Switch to datasets and scroll
      await user.click(screen.getByRole('tab', { name: /datasets/i }));
      const datasetsPanel = screen.getByTestId('datasets-panel');
      fireEvent.scroll(datasetsPanel, { target: { scrollTop: 200 } });

      // Switch to stacks and scroll
      await user.click(screen.getByRole('tab', { name: /stacks/i }));
      const stacksPanel = screen.getByTestId('stacks-panel');
      fireEvent.scroll(stacksPanel, { target: { scrollTop: 300 } });

      // Verify all positions are saved independently
      const stored = JSON.parse(
        localStorage.getItem('workbench:leftRail:scrollPositions') || '{}'
      );
      expect(stored.sessions).toBe(100);
      expect(stored.datasets).toBe(200);
      expect(stored.stacks).toBe(300);
    });
  });

  describe('Accessibility', () => {
    it('uses tabpanel role for content areas', () => {
      renderWithProvider(
        <LeftRail
          sessionsContent={<div>Sessions</div>}
          datasetsContent={<div>Datasets</div>}
          stacksContent={<div>Stacks</div>}
        />
      );

      const panels = screen.getAllByRole('tabpanel');
      expect(panels.length).toBeGreaterThan(0);
      expect(panels[0]).toBeInTheDocument();
    });

    it('links panels to tabs with aria-labelledby', () => {
      renderWithProvider(
        <LeftRail
          sessionsContent={<div>Sessions</div>}
          datasetsContent={<div>Datasets</div>}
          stacksContent={<div>Stacks</div>}
        />
      );

      const sessionsPanel = screen.getByTestId('sessions-panel');
      const datasetsPanel = screen.getByTestId('datasets-panel');
      const stacksPanel = screen.getByTestId('stacks-panel');

      expect(sessionsPanel).toHaveAttribute('aria-labelledby', 'tab-sessions');
      expect(datasetsPanel).toHaveAttribute('aria-labelledby', 'tab-datasets');
      expect(stacksPanel).toHaveAttribute('aria-labelledby', 'tab-stacks');
    });

    it('has scrollable overflow for content', () => {
      renderWithProvider(
        <LeftRail
          sessionsContent={<div>Sessions</div>}
          datasetsContent={<div>Datasets</div>}
          stacksContent={<div>Stacks</div>}
        />
      );

      const panels = [
        screen.getByTestId('sessions-panel'),
        screen.getByTestId('datasets-panel'),
        screen.getByTestId('stacks-panel'),
      ];

      panels.forEach((panel) => {
        expect(panel).toHaveClass('overflow-y-auto');
      });
    });
  });

  describe('Content Rendering', () => {
    it('renders complex React elements in tabs', () => {
      renderWithProvider(
        <LeftRail
          sessionsContent={
            <div>
              <h2>Sessions Header</h2>
              <ul>
                <li>Session 1</li>
                <li>Session 2</li>
              </ul>
            </div>
          }
          datasetsContent={<button>Load Dataset</button>}
          stacksContent={<input placeholder="Search stacks" />}
        />
      );

      expect(screen.getByText('Sessions Header')).toBeInTheDocument();
      expect(screen.getByText('Session 1')).toBeInTheDocument();
    });

    it('preserves content state when switching tabs', async () => {
      const user = userEvent.setup();

      const DatasetsContent = () => {
        const [count, setCount] = React.useState(0);
        return (
          <div>
            <button onClick={() => setCount(count + 1)}>
              Count: {count}
            </button>
          </div>
        );
      };

      renderWithProvider(
        <LeftRail
          sessionsContent={<div>Sessions</div>}
          datasetsContent={<DatasetsContent />}
          stacksContent={<div>Stacks</div>}
        />
      );

      // Switch to datasets and click button
      await user.click(screen.getByRole('tab', { name: /datasets/i }));
      const button = screen.getByRole('button', { name: /count/i });
      await user.click(button);
      expect(button).toHaveTextContent('Count: 1');

      // Switch away and back
      await user.click(screen.getByRole('tab', { name: /sessions/i }));
      await user.click(screen.getByRole('tab', { name: /datasets/i }));

      // State should be preserved
      expect(screen.getByRole('button', { name: /count/i })).toHaveTextContent(
        'Count: 1'
      );
    });
  });
});
