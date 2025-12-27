import { useDemoMode } from '@/hooks/demo/DemoProvider';

export function DemoWatermark() {
  const { enabled } = useDemoMode();
  if (!enabled) return null;

  return (
    <div className="pointer-events-none fixed bottom-4 right-4 z-40 text-[11px] font-semibold uppercase tracking-[0.28em] text-muted-foreground/60">
      DEMO MODE
    </div>
  );
}

export default DemoWatermark;
