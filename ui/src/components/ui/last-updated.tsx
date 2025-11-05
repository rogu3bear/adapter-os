// 【ui/src/hooks/usePolling.ts】 - lastUpdated timestamp
import { Clock } from 'lucide-react';
import { useTimestamp } from '@/hooks/useTimestamp';

interface LastUpdatedProps {
  timestamp: Date | null;
  className?: string;
}

export function LastUpdated({ timestamp, className = '' }: LastUpdatedProps) {
  const relativeTime = useTimestamp(timestamp?.toISOString());
  
  if (!timestamp) return null;
  
  return (
    <div className={`flex items-center gap-1 text-xs text-muted-foreground ${className}`}>
      <Clock className="h-3 w-3" />
      <span>Updated {relativeTime}</span>
    </div>
  );
}

