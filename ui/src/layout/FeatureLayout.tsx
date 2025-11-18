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
      <div className="space-y-4 md:space-y-6">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">{title}</h1>
          {description && <p className="text-muted-foreground">{description}</p>}
        </div>
        <div className="min-w-0 min-h-0 overflow-hidden">{children}</div>
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
    <div className="space-y-4 md:space-y-6">
      <div>
        <h1 className="text-3xl font-bold tracking-tight">{title}</h1>
        {description && <p className="text-muted-foreground">{description}</p>}
      </div>
      <div className="min-w-0 min-h-[60vh] overflow-hidden">
        <ResizablePanelGroup direction="horizontal" className="h-full w-full" layout={layout} onLayout={handleLayout}>
          <ResizablePanel minSize={20} className="min-w-0 min-h-0 overflow-hidden">
            <div className="h-full w-full overflow-auto p-4 md:p-6">{left}</div>
          </ResizablePanel>
          <ResizableHandle withHandle />
          <ResizablePanel minSize={20} className="min-w-0 min-h-0 overflow-hidden">
            <div className="h-full w-full overflow-auto p-4 md:p-6">{right}</div>
          </ResizablePanel>
        </ResizablePanelGroup>
      </div>
    </div>
  );
}


