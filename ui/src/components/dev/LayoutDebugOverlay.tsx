import { useEffect, useMemo, useState } from 'react';
import { cn } from '@/components/ui/utils';

interface LayoutDebugOverlayProps {
  enabled: boolean;
  onToggle: () => void;
}

const OVERFLOW_CLASS = 'aos-layout-debug-overflow';
const ROOT_CLASS = 'aos-layout-debug';

export function LayoutDebugOverlay({ enabled, onToggle }: LayoutDebugOverlayProps) {
  const [viewportWidth, setViewportWidth] = useState<number>(() => (typeof window !== 'undefined' ? window.innerWidth : 0));
  const [maxWidth, setMaxWidth] = useState<string>('—');
  const [hasOverflow, setHasOverflow] = useState(false);

  useEffect(() => {
    if (!enabled || typeof document === 'undefined') {
      return;
    }

    const doc = document.documentElement;
    doc.classList.add(ROOT_CLASS);

    const markOverflow = () => {
      const vw = window.innerWidth;
      setViewportWidth(vw);

      const computed = getComputedStyle(doc).getPropertyValue('--layout-content-width-xl')?.trim();
      if (computed) {
        setMaxWidth(computed);
      }

      // Remove existing outlines first
      const previous = Array.from(document.querySelectorAll(`.${OVERFLOW_CLASS}`));
      previous.forEach((el) => el.classList.remove(OVERFLOW_CLASS));

      const bodyElements = Array.from(document.body.querySelectorAll<HTMLElement>('*'));
      bodyElements.forEach((el) => {
        const rect = el.getBoundingClientRect();
        const scrollW = el.scrollWidth;
        if (rect.width - vw > 1 || scrollW - vw > 1) {
          el.classList.add(OVERFLOW_CLASS);
        }
      });

      setHasOverflow(doc.scrollWidth - doc.clientWidth > 1);
    };

    markOverflow();

    const handleResize = () => markOverflow();
    window.addEventListener('resize', handleResize);
    const interval = window.setInterval(markOverflow, 1200);

    return () => {
      doc.classList.remove(ROOT_CLASS);
      window.removeEventListener('resize', handleResize);
      window.clearInterval(interval);
      const cleanup = Array.from(document.querySelectorAll(`.${OVERFLOW_CLASS}`));
      cleanup.forEach((el) => el.classList.remove(OVERFLOW_CLASS));
    };
  }, [enabled]);

  const badgeTone = useMemo(() => (hasOverflow ? 'text-destructive' : 'text-emerald-600'), [hasOverflow]);

  if (!enabled) return null;

  return (
    <div
      className={cn(
        'fixed bottom-3 right-3 z-[60]',
        'rounded-md border border-dashed border-border bg-card/90 shadow-lg backdrop-blur px-3 py-2',
        'text-xs text-foreground space-y-1'
      )}
    >
      <div className="flex items-center justify-between gap-2">
        <span className="font-semibold">Layout Debug</span>
        <button
          type="button"
          onClick={onToggle}
          className="text-muted-foreground hover:text-foreground underline decoration-dotted"
        >
          Toggle
        </button>
      </div>
      <div className="flex gap-2">
        <span className="text-muted-foreground">viewport</span>
        <span>{viewportWidth}px</span>
      </div>
      <div className="flex gap-2">
        <span className="text-muted-foreground">maxWidth</span>
        <span>{maxWidth}</span>
      </div>
      <div className={cn('flex gap-2 items-center', badgeTone)}>
        <span className="text-muted-foreground">overflow</span>
        <span>{hasOverflow ? 'yes' : 'no'}</span>
      </div>
      <div className="text-muted-foreground">
        Red outlines mark elements wider than the viewport.
      </div>
    </div>
  );
}

export default LayoutDebugOverlay;

