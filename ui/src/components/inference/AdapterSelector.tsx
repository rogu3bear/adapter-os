import React from 'react';
import { Label } from '@/components/ui/label';
import { Button } from '@/components/ui/button';
import { Slider } from '@/components/ui/slider';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { Code, HelpCircle, Loader2 } from 'lucide-react';
import { Adapter } from '@/api/types';
import { ADAPTER_STATE_COLORS, ADAPTER_STRENGTH_PRESETS } from './constants';

export interface AdapterSelectorProps {
  /** Available adapters */
  adapters: Adapter[];
  /** Selected adapter ID */
  selectedId: string;
  /** Callback when adapter is selected */
  onSelect: (id: string) => void;
  /** Current adapter strength */
  strength: number | null;
  /** Callback when strength changes locally */
  onStrengthChange: (value: number) => void;
  /** Callback when strength should be committed to server */
  onStrengthCommit: (value: number) => Promise<void>;
  /** Whether strength update is in progress */
  isStrengthUpdating: boolean;
  /** Whether selector is disabled */
  disabled?: boolean;
}

/**
 * Adapter selector with state indicators and strength adjustment.
 */
export function AdapterSelector({
  adapters,
  selectedId,
  onSelect,
  strength,
  onStrengthChange,
  onStrengthCommit,
  isStrengthUpdating,
  disabled = false,
}: AdapterSelectorProps) {
  const hasAdapters = adapters.length > 0;
  const hasSelectedAdapter = selectedId && selectedId !== 'none';

  return (
    <div className="space-y-4">
      {/* Adapter Selection */}
      <div className="space-y-2">
        <Label htmlFor="adapter" className="flex items-center gap-1">
          Adapter (Optional){' '}
          {!hasAdapters && <span className="text-muted-foreground text-xs">(None available)</span>}
          <GlossaryTooltip termId="inference-adapter-stack">
            <span className="cursor-help text-muted-foreground hover:text-foreground">
              <HelpCircle className="h-3 w-3" />
            </span>
          </GlossaryTooltip>
        </Label>

        <Select value={selectedId} onValueChange={onSelect} disabled={!hasAdapters || disabled}>
          <SelectTrigger id="adapter">
            <SelectValue
              placeholder={
                hasAdapters ? 'Select adapter... (or use base model only)' : 'No adapters available'
              }
            />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="none">Default (No adapter)</SelectItem>
            {adapters
              .filter((adapter) => adapter.id && adapter.id !== '')
              .map((adapter) => {
                const stateIndicator =
                  (adapter.current_state && ADAPTER_STATE_COLORS[adapter.current_state]) ||
                  { color: 'bg-gray-300', label: adapter.current_state || 'Unknown' };

                return (
                  <SelectItem key={adapter.id} value={adapter.id}>
                    <div className="flex items-start gap-2">
                      <span
                        className={`h-2 w-2 rounded-full ${stateIndicator.color} mt-1`}
                        title={stateIndicator.label}
                        aria-label={`State: ${stateIndicator.label}`}
                      />
                      <Code className="h-4 w-4 mt-[2px]" aria-hidden="true" />
                      <div className="flex flex-col">
                        <div className="flex items-center gap-2">
                          <span>{adapter.name}</span>
                          <span className="text-xs text-muted-foreground">
                            ({stateIndicator.label})
                          </span>
                        </div>
                        <div className="text-[11px] text-muted-foreground">
                          Tier: {adapter.lora_tier ?? adapter.tier ?? 'unknown'} · Scope:{' '}
                          {adapter.lora_scope ?? adapter.scope ?? 'unspecified'}
                        </div>
                      </div>
                    </div>
                  </SelectItem>
                );
              })}
          </SelectContent>
        </Select>

        <p className="text-xs text-muted-foreground">
          {!hasAdapters
            ? 'No adapters available. Inference will use base model only.'
            : 'Adapters are trained LoRA modules that specialize the model for specific tasks. Select one to enhance inference quality. Base model runs without any adapter.'}
        </p>
      </div>

      {/* Strength Adjustment */}
      {hasSelectedAdapter && strength !== null && (
        <div className="space-y-2">
          <div className="flex items-center gap-2">
            <Label>Strength</Label>
            {isStrengthUpdating && (
              <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
            )}
            <span className="text-sm text-muted-foreground">{strength.toFixed(2)}</span>
          </div>

          <Slider
            min={0.2}
            max={2}
            step={0.05}
            value={[strength]}
            onValueChange={([value]) => onStrengthChange(value)}
            onValueCommit={([value]) => onStrengthCommit(value)}
            aria-label="Adapter strength"
            aria-valuetext={`${strength.toFixed(2)} strength`}
          />

          <div className="flex gap-2" role="group" aria-label="Strength presets">
            <Button
              size="sm"
              variant="outline"
              onClick={() => onStrengthCommit(ADAPTER_STRENGTH_PRESETS.light)}
              aria-label={`Set strength to light (${ADAPTER_STRENGTH_PRESETS.light})`}
            >
              Light
            </Button>
            <Button
              size="sm"
              variant="outline"
              onClick={() => onStrengthCommit(ADAPTER_STRENGTH_PRESETS.medium)}
              aria-label={`Set strength to medium (${ADAPTER_STRENGTH_PRESETS.medium})`}
            >
              Medium
            </Button>
            <Button
              size="sm"
              variant="outline"
              onClick={() => onStrengthCommit(ADAPTER_STRENGTH_PRESETS.strong)}
              aria-label={`Set strength to strong (${ADAPTER_STRENGTH_PRESETS.strong})`}
            >
              Strong
            </Button>
          </div>

          <p className="text-xs text-muted-foreground">
            Adjusts runtime scale for this adapter only.
          </p>
        </div>
      )}
    </div>
  );
}
