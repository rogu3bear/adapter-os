import { useMemo } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { cn } from '@/lib/utils';

type SparklineProps = {
  title: string;
  data: number[];
  unit?: string;
  windowMinutes: number;
  onWindowChange: (minutes: number) => void;
};

function Sparkline({ data }: { data: number[] }) {
  const points = useMemo(() => {
    if (!data.length) return '';
    const width = 120;
    const height = 40;
    const max = Math.max(...data);
    const min = Math.min(...data);
    const range = max - min || 1;
    return data
      .map((value, idx) => {
        const x = (idx / Math.max(data.length - 1, 1)) * width;
        const y = height - ((value - min) / range) * height;
        return `${x},${y}`;
      })
      .join(' ');
  }, [data]);

  const latest = data.at(-1);

  return (
    <div className="flex items-center gap-3">
      <svg viewBox="0 0 120 40" className="h-10 w-32 text-muted-foreground">
        <polyline
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          points={points}
        />
      </svg>
      <div className="text-sm text-muted-foreground">
        <div className="text-xs">Latest</div>
        <div className="text-base font-medium text-foreground">{latest ?? '—'}</div>
      </div>
    </div>
  );
}

export function MetricsSparklinePanel({ title, data, unit, windowMinutes, onWindowChange }: SparklineProps) {
  return (
    <Card className="border-border/70">
      <CardHeader className="flex flex-row items-center justify-between gap-3">
        <CardTitle className="text-base">{title}</CardTitle>
        <Select
          value={String(windowMinutes)}
          onValueChange={(value) => onWindowChange(Number(value))}
        >
          <SelectTrigger className="w-28">
            <SelectValue placeholder="Window" />
          </SelectTrigger>
          <SelectContent>
            {[5, 15, 60].map((minutes) => (
              <SelectItem key={minutes} value={String(minutes)}>
                {minutes} min
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </CardHeader>
      <CardContent className={cn('flex items-center justify-between', unit && 'text-sm text-muted-foreground')}>
        <Sparkline data={data} />
        {unit && <div className="text-xs text-muted-foreground">Unit: {unit}</div>}
      </CardContent>
    </Card>
  );
}

