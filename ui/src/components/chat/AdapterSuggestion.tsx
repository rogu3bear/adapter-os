import { AlertTriangle, Link2, Loader2, Sparkles } from 'lucide-react';
import { Switch } from '@/components/ui/switch';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';
import type { SuggestedAdapter } from '@/contexts/ChatContext';
import { colorWithAlpha } from '@/utils/adapterMagnet';

interface AdapterSuggestionProps {
  suggestion: SuggestedAdapter | null;
  loading?: boolean;
  autoAttachEnabled: boolean;
  onToggleAutoAttach: (enabled: boolean) => void;
  onAccept: () => void;
  onDismiss: () => void;
  showSnap?: boolean;
  error?: string | null;
  className?: string;
  conflictInfo?: { conflicts: string[]; reason?: string | null; resolution?: string } | null;
  magnetColor?: string;
}

export function AdapterSuggestion({
  suggestion,
  loading = false,
  autoAttachEnabled,
  onToggleAutoAttach,
  onAccept,
  onDismiss,
  showSnap = false,
  error,
  className,
  conflictInfo,
  magnetColor,
}: AdapterSuggestionProps) {
  if (!suggestion && !loading) {
    return null;
  }

  const confidenceLabel = suggestion ? Math.round((suggestion.confidence ?? 0) * 100) : null;
  const accentColor = magnetColor ?? null;

  return (
    <div className={cn('mb-3 transition-all duration-300', className)}>
      <div
        className={cn(
          'relative overflow-hidden rounded-lg border bg-gradient-to-r from-primary/5 via-background to-background shadow-sm backdrop-blur',
          'transition-all duration-300 ease-out',
          suggestion || loading ? 'opacity-100 translate-y-0' : 'pointer-events-none opacity-0 translate-y-2',
          showSnap ? 'magnet-snap-card' : ''
        )}
        style={
          accentColor
            ? {
                borderColor: colorWithAlpha(accentColor, 0.35),
                boxShadow: `0 10px 28px ${colorWithAlpha(accentColor, 0.18)}`,
              }
            : undefined
        }
      >
        <div className="flex items-center justify-between gap-3 px-3 py-2">
          <div className="flex items-center gap-2">
            <span
              className="inline-flex items-center gap-1 rounded-full px-2 py-1 text-[11px] font-semibold uppercase tracking-wide"
              style={
                accentColor
                  ? {
                      color: accentColor,
                      background: colorWithAlpha(accentColor, 0.12),
                      boxShadow: `0 0 0 1px ${colorWithAlpha(accentColor, 0.16)}`,
                    }
                  : undefined
              }
            >
              <Sparkles className="h-3.5 w-3.5" aria-hidden />
              Magnet
            </span>
            {suggestion && confidenceLabel !== null && (
              <Badge variant="secondary" className="gap-1">
                {confidenceLabel}% match
              </Badge>
            )}
            {loading && (
              <span className="inline-flex items-center gap-1 text-xs text-muted-foreground">
                <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden />
                Listening
              </span>
            )}
          </div>
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            <span>Auto-Attach</span>
            <Switch
              checked={autoAttachEnabled}
              onCheckedChange={(val) => onToggleAutoAttach(Boolean(val))}
              aria-label="Toggle auto attach"
            />
          </div>
        </div>
        <div className="space-y-2 px-3 pb-3">
          <div className="flex items-center justify-between gap-3">
            <div className="min-w-0 text-sm leading-tight text-foreground">
              {suggestion ? (
                <>
                  <span className="font-semibold">{suggestion.id}</span>
                  <span className="text-muted-foreground">
                    {suggestion.reason ? ` • ${suggestion.reason}` : ' is ready to snap into this message.'}
                  </span>
                </>
              ) : (
                <span className="text-muted-foreground">Keep typing to surface the right adapter.</span>
              )}
            </div>
            <div className="flex items-center gap-2">
              <Button
                size="sm"
                onClick={onAccept}
                disabled={!suggestion}
              >
                Attach (Tab)
              </Button>
              <Button
                size="sm"
                variant="ghost"
                onClick={onDismiss}
                disabled={!suggestion}
              >
                Mute
              </Button>
            </div>
          </div>
          {showSnap && (
            <div className="flex items-center gap-2 text-xs text-primary">
              <div className="h-px w-6 bg-primary/50" />
              <Link2 className="h-4 w-4" aria-hidden />
              <span>Snapped to the input below</span>
            </div>
          )}
          {conflictInfo && conflictInfo.conflicts.length > 0 && (
            <div className="flex items-center gap-2 rounded-md border border-amber-200 bg-amber-50 px-2 py-1 text-xs text-amber-800">
              <AlertTriangle className="h-3.5 w-3.5" aria-hidden />
              <span className="truncate">
                {conflictInfo.reason || 'Conflicts with existing adapters'} • {conflictInfo.conflicts.join(', ')}
              </span>
            </div>
          )}
          {error && (
            <div className="text-xs text-amber-700">
              Using cached suggestion • {error}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

export default AdapterSuggestion;
