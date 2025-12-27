import { useEffect, useState } from 'react';
import { Link2, X } from 'lucide-react';
import { cn } from '@/lib/utils';

interface AdapterAttachmentChipProps {
  adapterId: string;
  confidence?: number;
  onRemove?: () => void;
  onClick?: () => void;
  variant?: 'attached' | 'suggested';
  flash?: boolean;
}

export function AdapterAttachmentChip({
  adapterId,
  confidence,
  onRemove,
  onClick,
  variant = 'attached',
  flash = false,
}: AdapterAttachmentChipProps) {
  const [isFlashing, setIsFlashing] = useState(flash);

  useEffect(() => {
    if (!flash) return;
    setIsFlashing(true);
    const timer = setTimeout(() => setIsFlashing(false), 650);
    return () => clearTimeout(timer);
  }, [flash]);

  const confidenceLabel = typeof confidence === 'number' ? `${Math.round(confidence * 100)}%` : null;

  return (
    <div
      className={cn(
        'inline-flex items-center gap-2 rounded-full border px-3 py-1 text-sm shadow-sm transition-colors magnet-chip',
        variant === 'attached' ? 'bg-primary/5 border-primary/30 text-foreground' : 'bg-muted text-muted-foreground',
        onClick ? 'cursor-pointer hover:bg-primary/10' : 'cursor-default',
        isFlashing ? 'ring-2 ring-primary/40 magnet-incoming' : ''
      )}
      onClick={onClick}
      aria-label={`Adapter ${adapterId}${confidenceLabel ? ` (${confidenceLabel})` : ''}`}
    >
      <Link2 className="h-4 w-4 text-primary" aria-hidden="true" />
      <span className="font-medium truncate max-w-[12ch]" title={adapterId}>
        {adapterId}
      </span>
      {confidenceLabel && (
        <span className="text-xs text-muted-foreground">{confidenceLabel}</span>
      )}
      {onRemove && (
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            onRemove();
          }}
          className="ml-1 rounded-full p-1 hover:bg-background/70"
          aria-label={`Remove ${adapterId}`}
        >
          <X className="h-3 w-3" aria-hidden="true" />
        </button>
      )}
    </div>
  );
}

export default AdapterAttachmentChip;
