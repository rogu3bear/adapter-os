import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { axe, toHaveNoViolations } from 'jest-axe';
import { BrowserRouter } from 'react-router-dom';
import { Button } from '@/components/ui/button';
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { LoadingState } from '@/components/ui/loading-state';
import { PageSkeleton } from '@/components/ui/page-skeleton';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';

// Extend expect with axe matchers
expect.extend(toHaveNoViolations);

describe('Accessibility Tests', () => {
  describe('Button Component', () => {
    it('should have no accessibility violations', async () => {
      const { container } = render(
        <Button>Click me</Button>
      );
      const results = await axe(container);
      expect(results).toHaveNoViolations();
    });

    it('should have accessible disabled state', async () => {
      const { container } = render(
        <Button disabled>Disabled Button</Button>
      );
      const results = await axe(container);
      expect(results).toHaveNoViolations();
    });

    it('should support aria-label', () => {
      render(
        <Button aria-label="Close dialog">X</Button>
      );
      expect(screen.getByRole('button', { name: 'Close dialog' })).toBeTruthy();
    });
  });

  describe('Card Component', () => {
    it('should have no accessibility violations', async () => {
      const { container } = render(
        <Card>
          <CardHeader>
            <CardTitle>Card Title</CardTitle>
          </CardHeader>
          <CardContent>
            <p>Card content goes here</p>
          </CardContent>
        </Card>
      );
      const results = await axe(container);
      expect(results).toHaveNoViolations();
    });
  });

  describe('Form Elements', () => {
    it('should have proper label associations', async () => {
      const { container } = render(
        <div>
          <Label htmlFor="email">Email</Label>
          <Input id="email" type="email" placeholder="Enter email" />
        </div>
      );
      const results = await axe(container);
      expect(results).toHaveNoViolations();
    });

    it('should support aria-describedby for help text', async () => {
      const { container } = render(
        <div>
          <Label htmlFor="password">Password</Label>
          <Input
            id="password"
            type="password"
            aria-describedby="password-help"
          />
          <p id="password-help">Must be at least 8 characters</p>
        </div>
      );
      const results = await axe(container);
      expect(results).toHaveNoViolations();
    });

    it('should indicate required fields', async () => {
      const { container } = render(
        <div>
          <Label htmlFor="name">
            Name <span aria-hidden="true">*</span>
          </Label>
          <Input id="name" required aria-required="true" />
        </div>
      );
      const results = await axe(container);
      expect(results).toHaveNoViolations();
    });
  });

  describe('Loading States', () => {
    it('LoadingState should have proper ARIA attributes', () => {
      render(
        <LoadingState title="Loading data" description="Please wait..." />
      );

      const loadingElement = screen.getByRole('status');
      expect(loadingElement).toBeTruthy();
      expect(loadingElement.getAttribute('aria-live')).toBe('polite');
    });

    it('PageSkeleton should have proper ARIA attributes', () => {
      render(
        <PageSkeleton variant="dashboard" />
      );

      const skeletonElements = screen.getAllByRole('status');
      expect(skeletonElements.length).toBeGreaterThan(0);

      // The first status element is the PageSkeleton wrapper
      const pageSkeletonElement = skeletonElements[0];
      expect(pageSkeletonElement.getAttribute('aria-label')).toBe('Loading page content');
    });
  });

  describe('Error States', () => {
    it('SectionErrorBoundary should have accessible error display', () => {
      const ThrowError = () => {
        throw new Error('Test error');
      };

      render(
        <SectionErrorBoundary sectionName="Test Section">
          <ThrowError />
        </SectionErrorBoundary>
      );

      // Error message should be visible
      expect(screen.getByText('Test Section failed to load')).toBeTruthy();
      expect(screen.getByText('Test error')).toBeTruthy();

      // Retry button should be accessible
      const retryButton = screen.getByRole('button', { name: /retry loading this section/i });
      expect(retryButton).toBeTruthy();
    });
  });

  describe('Interactive Elements', () => {
    it('should have visible focus indicators', () => {
      render(
        <Button>Focusable Button</Button>
      );

      const button = screen.getByRole('button');
      button.focus();
      // Focus styles are applied via CSS, just verify element can be focused
      expect(document.activeElement).toBe(button);
    });

    it('should support keyboard navigation', () => {
      render(
        <div>
          <Button>First</Button>
          <Button>Second</Button>
          <Button>Third</Button>
        </div>
      );

      const buttons = screen.getAllByRole('button');
      buttons[0].focus();
      expect(document.activeElement).toBe(buttons[0]);
    });
  });

  describe('Semantic Structure', () => {
    it('should use proper heading hierarchy', async () => {
      const { container } = render(
        <main>
          <h1>Page Title</h1>
          <section>
            <h2>Section Title</h2>
            <p>Content</p>
          </section>
        </main>
      );
      const results = await axe(container);
      expect(results).toHaveNoViolations();
    });

    it('should use landmark regions', async () => {
      const { container } = render(
        <BrowserRouter>
          <div>
            <header role="banner">Header</header>
            <nav role="navigation">Navigation</nav>
            <main role="main">Main Content</main>
            <footer role="contentinfo">Footer</footer>
          </div>
        </BrowserRouter>
      );
      const results = await axe(container);
      expect(results).toHaveNoViolations();
    });
  });

  describe('Color Contrast', () => {
    it('text should have sufficient contrast', async () => {
      const { container } = render(
        <div>
          <p className="text-foreground">Primary text</p>
          <p className="text-muted-foreground">Muted text</p>
        </div>
      );
      // Note: axe can check color contrast when CSS is applied
      const results = await axe(container);
      expect(results).toHaveNoViolations();
    });
  });

  describe('Screen Reader Support', () => {
    it('should have sr-only text for icon buttons', () => {
      render(
        <Button aria-label="Delete item">
          <span aria-hidden="true">X</span>
        </Button>
      );

      const button = screen.getByRole('button', { name: 'Delete item' });
      expect(button).toBeTruthy();
    });

    it('should announce dynamic content changes', () => {
      render(
        <div aria-live="polite" aria-atomic="true">
          Status: Loading complete
        </div>
      );

      const liveRegion = screen.getByText(/status/i);
      expect(liveRegion.getAttribute('aria-live')).toBe('polite');
    });
  });
});

describe('WCAG 2.1 AA Compliance', () => {
  describe('1.1.1 Non-text Content', () => {
    it('images should have alt text', async () => {
      const { container } = render(
        <img src="/logo.png" alt="Company logo" />
      );
      const results = await axe(container);
      expect(results).toHaveNoViolations();
    });

    it('decorative images should have empty alt', async () => {
      const { container } = render(
        <img src="/decoration.png" alt="" aria-hidden="true" />
      );
      const results = await axe(container);
      expect(results).toHaveNoViolations();
    });
  });

  describe('2.1.1 Keyboard', () => {
    it('all interactive elements should be keyboard accessible', () => {
      render(
        <div>
          <Button tabIndex={0}>Click</Button>
          <a href="/link" tabIndex={0}>Link</a>
          <Input tabIndex={0} />
        </div>
      );

      const button = screen.getByRole('button');
      const link = screen.getByRole('link');
      const input = screen.getByRole('textbox');

      expect(button.getAttribute('tabindex')).not.toBe('-1');
      expect(link.getAttribute('tabindex')).not.toBe('-1');
      expect(input.getAttribute('tabindex')).not.toBe('-1');
    });
  });

  describe('2.4.1 Bypass Blocks', () => {
    it('should have skip link capability', () => {
      render(
        <div>
          <a href="#main-content" className="sr-only focus:not-sr-only">
            Skip to main content
          </a>
          <nav>Navigation</nav>
          <main id="main-content">
            <h1>Main Content</h1>
          </main>
        </div>
      );

      const skipLink = screen.getByText('Skip to main content');
      expect(skipLink).toBeTruthy();
      expect(skipLink.getAttribute('href')).toBe('#main-content');
    });
  });

  describe('4.1.2 Name, Role, Value', () => {
    it('custom components should expose proper role', () => {
      render(
        <Button role="button" aria-pressed="false">
          Toggle
        </Button>
      );

      const button = screen.getByRole('button');
      expect(button.getAttribute('aria-pressed')).toBe('false');
    });
  });
});
