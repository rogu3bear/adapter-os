import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import React from 'react';
import { render, screen } from '@testing-library/react';
import { HighlightOverlay, type Highlight } from '@/components/documents/HighlightOverlay';

describe('HighlightOverlay', () => {
  describe('Overlay Positioning', () => {
    it('renders overlay at correct position with scale 1.0', () => {
      const highlights: Highlight[] = [
        {
          id: 'h1',
          page: 1,
          bbox: { x: 100, y: 200, width: 300, height: 50 },
          style: 'citation',
        },
      ];

      const { container } = render(
        <HighlightOverlay highlights={highlights} currentPage={1} scale={1.0} />
      );

      const highlightBox = container.querySelector('[title*="Citation on page 1"]');
      expect(highlightBox).toBeTruthy();

      const styles = (highlightBox as HTMLElement).style;
      expect(styles.left).toBe('100px');
      expect(styles.top).toBe('200px');
      expect(styles.width).toBe('300px');
      expect(styles.height).toBe('50px');
    });

    it('scales position and size correctly with scale 2.0', () => {
      const highlights: Highlight[] = [
        {
          id: 'h1',
          page: 1,
          bbox: { x: 50, y: 100, width: 200, height: 40 },
          style: 'citation',
        },
      ];

      const { container } = render(
        <HighlightOverlay highlights={highlights} currentPage={1} scale={2.0} />
      );

      const highlightBox = container.querySelector('[title*="Citation on page 1"]');
      const styles = (highlightBox as HTMLElement).style;

      expect(styles.left).toBe('100px'); // 50 * 2.0
      expect(styles.top).toBe('200px'); // 100 * 2.0
      expect(styles.width).toBe('400px'); // 200 * 2.0
      expect(styles.height).toBe('80px'); // 40 * 2.0
    });

    it('scales position and size correctly with scale 0.5', () => {
      const highlights: Highlight[] = [
        {
          id: 'h1',
          page: 1,
          bbox: { x: 200, y: 400, width: 600, height: 100 },
          style: 'citation',
        },
      ];

      const { container } = render(
        <HighlightOverlay highlights={highlights} currentPage={1} scale={0.5} />
      );

      const highlightBox = container.querySelector('[title*="Citation on page 1"]');
      const styles = (highlightBox as HTMLElement).style;

      expect(styles.left).toBe('100px'); // 200 * 0.5
      expect(styles.top).toBe('200px'); // 400 * 0.5
      expect(styles.width).toBe('300px'); // 600 * 0.5
      expect(styles.height).toBe('50px'); // 100 * 0.5
    });

    it('handles fractional scale values correctly', () => {
      const highlights: Highlight[] = [
        {
          id: 'h1',
          page: 1,
          bbox: { x: 100, y: 100, width: 100, height: 100 },
          style: 'citation',
        },
      ];

      const { container } = render(
        <HighlightOverlay highlights={highlights} currentPage={1} scale={1.5} />
      );

      const highlightBox = container.querySelector('[title*="Citation on page 1"]');
      const styles = (highlightBox as HTMLElement).style;

      expect(styles.left).toBe('150px'); // 100 * 1.5
      expect(styles.top).toBe('150px'); // 100 * 1.5
      expect(styles.width).toBe('150px'); // 100 * 1.5
      expect(styles.height).toBe('150px'); // 100 * 1.5
    });
  });

  describe('Highlight Styling', () => {
    it('applies yellow styling for citation highlights', () => {
      const highlights: Highlight[] = [
        {
          id: 'citation1',
          page: 1,
          bbox: { x: 10, y: 20, width: 100, height: 30 },
          style: 'citation',
        },
      ];

      const { container } = render(
        <HighlightOverlay highlights={highlights} currentPage={1} scale={1.0} />
      );

      const highlightBox = container.querySelector('[title*="Citation"]');
      expect(highlightBox).toBeTruthy();
      expect(highlightBox?.className).toMatch(/bg-yellow-300\/40/);
      expect(highlightBox?.className).toMatch(/border-yellow-500/);
    });

    it('applies blue styling for search highlights', () => {
      const highlights: Highlight[] = [
        {
          id: 'search1',
          page: 1,
          bbox: { x: 10, y: 20, width: 100, height: 30 },
          style: 'search',
        },
      ];

      const { container } = render(
        <HighlightOverlay highlights={highlights} currentPage={1} scale={1.0} />
      );

      const highlightBox = container.querySelector('[title*="Search result"]');
      expect(highlightBox).toBeTruthy();
      expect(highlightBox?.className).toMatch(/bg-blue-300\/40/);
      expect(highlightBox?.className).toMatch(/border-blue-500/);
    });

    it('applies purple styling for undefined style', () => {
      const highlights: Highlight[] = [
        {
          id: 'default1',
          page: 1,
          bbox: { x: 10, y: 20, width: 100, height: 30 },
        },
      ];

      const { container } = render(
        <HighlightOverlay highlights={highlights} currentPage={1} scale={1.0} />
      );

      const highlightBox = container.querySelector('[title*="Highlight"]');
      expect(highlightBox).toBeTruthy();
      expect(highlightBox?.className).toMatch(/bg-purple-300\/40/);
      expect(highlightBox?.className).toMatch(/border-purple-500/);
    });

    it('applies transition and rounded classes to all highlights', () => {
      const highlights: Highlight[] = [
        {
          id: 'h1',
          page: 1,
          bbox: { x: 10, y: 20, width: 100, height: 30 },
          style: 'citation',
        },
      ];

      const { container } = render(
        <HighlightOverlay highlights={highlights} currentPage={1} scale={1.0} />
      );

      const highlightBox = container.querySelector('[title*="Citation"]');
      expect(highlightBox?.className).toMatch(/rounded/);
      expect(highlightBox?.className).toMatch(/transition-all/);
      expect(highlightBox?.className).toMatch(/duration-200/);
    });

    it('sets correct title attribute for citation', () => {
      const highlights: Highlight[] = [
        {
          id: 'h1',
          page: 3,
          bbox: { x: 10, y: 20, width: 100, height: 30 },
          style: 'citation',
        },
      ];

      const { container } = render(
        <HighlightOverlay highlights={highlights} currentPage={3} scale={1.0} />
      );

      const highlightBox = container.querySelector('div[title]');
      expect(highlightBox?.getAttribute('title')).toBe('Citation on page 3');
    });

    it('sets correct title attribute for search', () => {
      const highlights: Highlight[] = [
        {
          id: 'h1',
          page: 5,
          bbox: { x: 10, y: 20, width: 100, height: 30 },
          style: 'search',
        },
      ];

      const { container } = render(
        <HighlightOverlay highlights={highlights} currentPage={5} scale={1.0} />
      );

      const highlightBox = container.querySelector('div[title]');
      expect(highlightBox?.getAttribute('title')).toBe('Search result on page 5');
    });

    it('sets correct title attribute for default highlight', () => {
      const highlights: Highlight[] = [
        {
          id: 'h1',
          page: 2,
          bbox: { x: 10, y: 20, width: 100, height: 30 },
        },
      ];

      const { container } = render(
        <HighlightOverlay highlights={highlights} currentPage={2} scale={1.0} />
      );

      const highlightBox = container.querySelector('div[title]');
      expect(highlightBox?.getAttribute('title')).toBe('Highlight on page 2');
    });
  });

  describe('Multiple Highlights', () => {
    it('renders multiple highlights on the same page', () => {
      const highlights: Highlight[] = [
        {
          id: 'h1',
          page: 1,
          bbox: { x: 10, y: 20, width: 100, height: 30 },
          style: 'citation',
        },
        {
          id: 'h2',
          page: 1,
          bbox: { x: 200, y: 100, width: 150, height: 40 },
          style: 'search',
        },
        {
          id: 'h3',
          page: 1,
          bbox: { x: 50, y: 300, width: 200, height: 25 },
        },
      ];

      const { container } = render(
        <HighlightOverlay highlights={highlights} currentPage={1} scale={1.0} />
      );

      const highlightBoxes = container.querySelectorAll('div[title]');
      expect(highlightBoxes.length).toBe(3);
    });

    it('maintains unique keys for multiple highlights', () => {
      const highlights: Highlight[] = [
        { id: 'h1', page: 1, bbox: { x: 10, y: 20, width: 100, height: 30 } },
        { id: 'h2', page: 1, bbox: { x: 50, y: 60, width: 120, height: 35 } },
        { id: 'h3', page: 1, bbox: { x: 100, y: 150, width: 80, height: 20 } },
      ];

      const { container } = render(
        <HighlightOverlay highlights={highlights} currentPage={1} scale={1.0} />
      );

      const highlightBoxes = container.querySelectorAll('div[title]');
      expect(highlightBoxes.length).toBe(3);

      // Verify each has different positions (proving they're distinct elements)
      const positions = Array.from(highlightBoxes).map((box) => ({
        left: (box as HTMLElement).style.left,
        top: (box as HTMLElement).style.top,
      }));

      expect(positions[0]).toEqual({ left: '10px', top: '20px' });
      expect(positions[1]).toEqual({ left: '50px', top: '60px' });
      expect(positions[2]).toEqual({ left: '100px', top: '150px' });
    });

    it('renders mixed highlight styles correctly', () => {
      const highlights: Highlight[] = [
        { id: 'h1', page: 1, bbox: { x: 10, y: 20, width: 100, height: 30 }, style: 'citation' },
        { id: 'h2', page: 1, bbox: { x: 50, y: 60, width: 120, height: 35 }, style: 'search' },
        { id: 'h3', page: 1, bbox: { x: 100, y: 150, width: 80, height: 20 } },
      ];

      const { container } = render(
        <HighlightOverlay highlights={highlights} currentPage={1} scale={1.0} />
      );

      const citationBox = container.querySelector('[title*="Citation"]');
      const searchBox = container.querySelector('[title*="Search result"]');
      const defaultBox = container.querySelector('[title="Highlight on page 1"]');

      expect(citationBox?.className).toMatch(/bg-yellow-300\/40/);
      expect(searchBox?.className).toMatch(/bg-blue-300\/40/);
      expect(defaultBox?.className).toMatch(/bg-purple-300\/40/);
    });

    it('filters highlights to only show current page', () => {
      const highlights: Highlight[] = [
        { id: 'h1', page: 1, bbox: { x: 10, y: 20, width: 100, height: 30 } },
        { id: 'h2', page: 2, bbox: { x: 50, y: 60, width: 120, height: 35 } },
        { id: 'h3', page: 3, bbox: { x: 100, y: 150, width: 80, height: 20 } },
      ];

      const { container } = render(
        <HighlightOverlay highlights={highlights} currentPage={2} scale={1.0} />
      );

      const highlightBoxes = container.querySelectorAll('div[title]');
      expect(highlightBoxes.length).toBe(1);
      expect(highlightBoxes[0].getAttribute('title')).toBe('Highlight on page 2');
    });

    it('skips highlights without bbox', () => {
      const highlights: Highlight[] = [
        { id: 'h1', page: 1, bbox: { x: 10, y: 20, width: 100, height: 30 } },
        { id: 'h2', page: 1 }, // No bbox
        { id: 'h3', page: 1, bbox: { x: 100, y: 150, width: 80, height: 20 } },
      ];

      const { container } = render(
        <HighlightOverlay highlights={highlights} currentPage={1} scale={1.0} />
      );

      const highlightBoxes = container.querySelectorAll('div[title]');
      expect(highlightBoxes.length).toBe(2);
    });
  });

  describe('Page Filtering', () => {
    it('shows highlights only for current page', () => {
      const highlights: Highlight[] = [
        { id: 'h1', page: 1, bbox: { x: 10, y: 20, width: 100, height: 30 } },
        { id: 'h2', page: 2, bbox: { x: 50, y: 60, width: 120, height: 35 } },
      ];

      const { container: container1 } = render(
        <HighlightOverlay highlights={highlights} currentPage={1} scale={1.0} />
      );

      const page1Boxes = container1.querySelectorAll('div[title]');
      expect(page1Boxes.length).toBe(1);
      expect(page1Boxes[0].getAttribute('title')).toBe('Highlight on page 1');

      const { container: container2 } = render(
        <HighlightOverlay highlights={highlights} currentPage={2} scale={1.0} />
      );

      const page2Boxes = container2.querySelectorAll('div[title]');
      expect(page2Boxes.length).toBe(1);
      expect(page2Boxes[0].getAttribute('title')).toBe('Highlight on page 2');
    });

    it('returns null when no highlights match current page', () => {
      const highlights: Highlight[] = [
        { id: 'h1', page: 1, bbox: { x: 10, y: 20, width: 100, height: 30 } },
        { id: 'h2', page: 2, bbox: { x: 50, y: 60, width: 120, height: 35 } },
      ];

      const { container } = render(
        <HighlightOverlay highlights={highlights} currentPage={3} scale={1.0} />
      );

      expect(container.firstChild).toBeNull();
    });

    it('returns null when highlights array is empty', () => {
      const { container } = render(
        <HighlightOverlay highlights={[]} currentPage={1} scale={1.0} />
      );

      expect(container.firstChild).toBeNull();
    });

    it('returns null when all highlights lack bbox', () => {
      const highlights: Highlight[] = [
        { id: 'h1', page: 1 },
        { id: 'h2', page: 1 },
      ];

      const { container } = render(
        <HighlightOverlay highlights={highlights} currentPage={1} scale={1.0} />
      );

      expect(container.firstChild).toBeNull();
    });
  });

  describe('Overlay Container Behavior', () => {
    it('applies correct container classes', () => {
      const highlights: Highlight[] = [
        { id: 'h1', page: 1, bbox: { x: 10, y: 20, width: 100, height: 30 } },
      ];

      const { container } = render(
        <HighlightOverlay highlights={highlights} currentPage={1} scale={1.0} />
      );

      const overlay = container.querySelector('[role="presentation"]');
      expect(overlay?.className).toMatch(/absolute/);
      expect(overlay?.className).toMatch(/inset-0/);
      expect(overlay?.className).toMatch(/pointer-events-none/);
    });

    it('applies custom className to container', () => {
      const highlights: Highlight[] = [
        { id: 'h1', page: 1, bbox: { x: 10, y: 20, width: 100, height: 30 } },
      ];

      const { container } = render(
        <HighlightOverlay
          highlights={highlights}
          currentPage={1}
          scale={1.0}
          className="custom-overlay"
        />
      );

      const overlay = container.querySelector('[role="presentation"]');
      expect(overlay?.className).toMatch(/custom-overlay/);
    });

    it('sets aria-hidden on container', () => {
      const highlights: Highlight[] = [
        { id: 'h1', page: 1, bbox: { x: 10, y: 20, width: 100, height: 30 } },
      ];

      const { container } = render(
        <HighlightOverlay highlights={highlights} currentPage={1} scale={1.0} />
      );

      const overlay = container.querySelector('[role="presentation"]');
      expect(overlay?.getAttribute('aria-hidden')).toBe('true');
    });

    it('sets role="presentation" on container', () => {
      const highlights: Highlight[] = [
        { id: 'h1', page: 1, bbox: { x: 10, y: 20, width: 100, height: 30 } },
      ];

      const { container } = render(
        <HighlightOverlay highlights={highlights} currentPage={1} scale={1.0} />
      );

      const overlay = container.querySelector('[role="presentation"]');
      expect(overlay).toBeTruthy();
    });
  });

  describe('Scale Updates (Simulating Scroll/Zoom)', () => {
    it('updates highlight positions when scale changes', () => {
      const highlights: Highlight[] = [
        { id: 'h1', page: 1, bbox: { x: 100, y: 100, width: 100, height: 100 } },
      ];

      const { container, rerender } = render(
        <HighlightOverlay highlights={highlights} currentPage={1} scale={1.0} />
      );

      let highlightBox = container.querySelector('div[title]') as HTMLElement;
      expect(highlightBox.style.left).toBe('100px');
      expect(highlightBox.style.width).toBe('100px');

      // Zoom in (scale up)
      rerender(
        <HighlightOverlay highlights={highlights} currentPage={1} scale={2.0} />
      );

      highlightBox = container.querySelector('div[title]') as HTMLElement;
      expect(highlightBox.style.left).toBe('200px');
      expect(highlightBox.style.width).toBe('200px');

      // Zoom out (scale down)
      rerender(
        <HighlightOverlay highlights={highlights} currentPage={1} scale={0.5} />
      );

      highlightBox = container.querySelector('div[title]') as HTMLElement;
      expect(highlightBox.style.left).toBe('50px');
      expect(highlightBox.style.width).toBe('50px');
    });

    it('updates when highlights change', () => {
      const highlights1: Highlight[] = [
        { id: 'h1', page: 1, bbox: { x: 10, y: 20, width: 100, height: 30 } },
      ];

      const highlights2: Highlight[] = [
        { id: 'h1', page: 1, bbox: { x: 10, y: 20, width: 100, height: 30 } },
        { id: 'h2', page: 1, bbox: { x: 50, y: 60, width: 120, height: 35 } },
      ];

      const { container, rerender } = render(
        <HighlightOverlay highlights={highlights1} currentPage={1} scale={1.0} />
      );

      let highlightBoxes = container.querySelectorAll('div[title]');
      expect(highlightBoxes.length).toBe(1);

      rerender(
        <HighlightOverlay highlights={highlights2} currentPage={1} scale={1.0} />
      );

      highlightBoxes = container.querySelectorAll('div[title]');
      expect(highlightBoxes.length).toBe(2);
    });

    it('updates when page changes', () => {
      const highlights: Highlight[] = [
        { id: 'h1', page: 1, bbox: { x: 10, y: 20, width: 100, height: 30 } },
        { id: 'h2', page: 2, bbox: { x: 50, y: 60, width: 120, height: 35 } },
      ];

      const { container, rerender } = render(
        <HighlightOverlay highlights={highlights} currentPage={1} scale={1.0} />
      );

      let highlightBoxes = container.querySelectorAll('div[title]');
      expect(highlightBoxes.length).toBe(1);
      expect(highlightBoxes[0].getAttribute('title')).toBe('Highlight on page 1');

      rerender(
        <HighlightOverlay highlights={highlights} currentPage={2} scale={1.0} />
      );

      highlightBoxes = container.querySelectorAll('div[title]');
      expect(highlightBoxes.length).toBe(1);
      expect(highlightBoxes[0].getAttribute('title')).toBe('Highlight on page 2');
    });
  });

  describe('Cleanup on Unmount', () => {
    it('removes overlay when component unmounts', () => {
      const highlights: Highlight[] = [
        { id: 'h1', page: 1, bbox: { x: 10, y: 20, width: 100, height: 30 } },
      ];

      const { container, unmount } = render(
        <HighlightOverlay highlights={highlights} currentPage={1} scale={1.0} />
      );

      expect(container.querySelector('[role="presentation"]')).toBeTruthy();

      unmount();

      expect(container.querySelector('[role="presentation"]')).toBeNull();
      expect(container.firstChild).toBeNull();
    });

    it('cleans up multiple highlights on unmount', () => {
      const highlights: Highlight[] = [
        { id: 'h1', page: 1, bbox: { x: 10, y: 20, width: 100, height: 30 } },
        { id: 'h2', page: 1, bbox: { x: 50, y: 60, width: 120, height: 35 } },
        { id: 'h3', page: 1, bbox: { x: 100, y: 150, width: 80, height: 20 } },
      ];

      const { container, unmount } = render(
        <HighlightOverlay highlights={highlights} currentPage={1} scale={1.0} />
      );

      expect(container.querySelectorAll('div[title]').length).toBe(3);

      unmount();

      expect(container.querySelectorAll('div[title]').length).toBe(0);
      expect(container.firstChild).toBeNull();
    });
  });

  describe('Edge Cases', () => {
    it('handles zero dimensions gracefully', () => {
      const highlights: Highlight[] = [
        { id: 'h1', page: 1, bbox: { x: 10, y: 20, width: 0, height: 0 } },
      ];

      const { container } = render(
        <HighlightOverlay highlights={highlights} currentPage={1} scale={1.0} />
      );

      const highlightBox = container.querySelector('div[title]') as HTMLElement;
      expect(highlightBox.style.width).toBe('0px');
      expect(highlightBox.style.height).toBe('0px');
    });

    it('handles negative coordinates', () => {
      const highlights: Highlight[] = [
        { id: 'h1', page: 1, bbox: { x: -10, y: -20, width: 100, height: 30 } },
      ];

      const { container } = render(
        <HighlightOverlay highlights={highlights} currentPage={1} scale={1.0} />
      );

      const highlightBox = container.querySelector('div[title]') as HTMLElement;
      expect(highlightBox.style.left).toBe('-10px');
      expect(highlightBox.style.top).toBe('-20px');
    });

    it('handles very large coordinates', () => {
      const highlights: Highlight[] = [
        { id: 'h1', page: 1, bbox: { x: 10000, y: 20000, width: 5000, height: 3000 } },
      ];

      const { container } = render(
        <HighlightOverlay highlights={highlights} currentPage={1} scale={1.0} />
      );

      const highlightBox = container.querySelector('div[title]') as HTMLElement;
      expect(highlightBox.style.left).toBe('10000px');
      expect(highlightBox.style.top).toBe('20000px');
      expect(highlightBox.style.width).toBe('5000px');
      expect(highlightBox.style.height).toBe('3000px');
    });

    it('handles scale of 0', () => {
      const highlights: Highlight[] = [
        { id: 'h1', page: 1, bbox: { x: 100, y: 100, width: 100, height: 100 } },
      ];

      const { container } = render(
        <HighlightOverlay highlights={highlights} currentPage={1} scale={0} />
      );

      const highlightBox = container.querySelector('div[title]') as HTMLElement;
      expect(highlightBox.style.left).toBe('0px');
      expect(highlightBox.style.top).toBe('0px');
      expect(highlightBox.style.width).toBe('0px');
      expect(highlightBox.style.height).toBe('0px');
    });

    it('handles very large scale values', () => {
      const highlights: Highlight[] = [
        { id: 'h1', page: 1, bbox: { x: 10, y: 10, width: 10, height: 10 } },
      ];

      const { container } = render(
        <HighlightOverlay highlights={highlights} currentPage={1} scale={100} />
      );

      const highlightBox = container.querySelector('div[title]') as HTMLElement;
      expect(highlightBox.style.left).toBe('1000px');
      expect(highlightBox.style.top).toBe('1000px');
      expect(highlightBox.style.width).toBe('1000px');
      expect(highlightBox.style.height).toBe('1000px');
    });

    it('handles duplicate highlight IDs', () => {
      const highlights: Highlight[] = [
        { id: 'h1', page: 1, bbox: { x: 10, y: 20, width: 100, height: 30 } },
        { id: 'h1', page: 1, bbox: { x: 50, y: 60, width: 120, height: 35 } },
      ];

      // Component should still render, but React may warn about duplicate keys
      const { container } = render(
        <HighlightOverlay highlights={highlights} currentPage={1} scale={1.0} />
      );

      const highlightBoxes = container.querySelectorAll('div[title]');
      expect(highlightBoxes.length).toBe(2);
    });
  });
});
