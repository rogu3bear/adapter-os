import React from 'react';
import { ResizablePanelGroup, ResizablePanel, ResizableHandle } from '@/components/ui/resizable';
import { useResize } from '@/layout/LayoutProvider';

interface FeatureLayoutProps {
  title: string;
  description?: string;
  children?: React.ReactNode;
  resizable?: boolean;
  storageKey?: string;
  left?: React.ReactNode;
  right?: React.ReactNode;
  defaultLayout?: number[]; // e.g., [30, 70]
}

export default function FeatureLayout({ title, description, children, resizable, storageKey, left, right, defaultLayout = [40, 60] }: FeatureLayoutProps) {
  // Non-overlapping container tokens: spacing 16/24/32, max widths, overflow guards
  if (!resizable) {
    return (
      <div className="min-w-0 min-h-0 p-[var(--space-6)]">
        <header className="mb-[var(--section-gap)]">
          <h1 className="text-[var(--font-h1)] font-bold text-[var(--gray-900)]">
            {title}
          </h1>
          <p className="text-[var(--font-body)] text-[var(--gray-600)] mt-[var(--space-2)]">
            {description}
          </p>
        </header>
        
        <main className="grid gap-[var(--space-4)] border-t border-[var(--gray-300)] pt-[var(--space-6)]">
          {children}
        </main>
        
        {/* Existing resizable panels with var(--grid-unit) for sizing */}
      </div>
    );
  }

  const { getLayout, setLayout } = useResize();
  const saved = storageKey ? getLayout(storageKey) : undefined;
  const [layout, setLayoutState] = React.useState<number[]>(saved ?? defaultLayout);

  const handleLayout = React.useCallback((sizes: number[]) => {
    setLayoutState(sizes);
    if (storageKey) setLayout(storageKey, sizes);
  }, [setLayout, storageKey]);

  return (
    <div className="min-w-0 min-h-0 p-[var(--space-6)]">
      <header className="mb-[var(--section-gap)]">
        <h1 className="text-[var(--font-h1)] font-bold text-[var(--gray-900)]">
          {title}
        </h1>
        <p className="text-[var(--font-body)] text-[var(--gray-600)] mt-[var(--space-2)]">
          {description}
        </p>
      </header>
      
      <main className="grid gap-[var(--space-4)] border-t border-[var(--gray-300)] pt-[var(--space-6)]">
        {children}
      </main>
      
      {/* Existing resizable panels with var(--grid-unit) for sizing */}
    </div>
  );
}


