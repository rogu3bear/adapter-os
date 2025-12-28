import React from 'react';
import { Label } from '@/components/ui/label';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { HelpCircle, Layers } from 'lucide-react';

export interface Stack {
  id: string;
  name: string;
  adapter_ids?: string[];
  lifecycle_state?: string;
}

export interface StackSelectorProps {
  /** Available stacks */
  stacks: Stack[];
  /** Selected stack ID */
  selectedStackId: string;
  /** Default stack ID */
  defaultStackId?: string;
  /** Callback when stack is selected */
  onSelect: (id: string) => void;
  /** Callback to set stack as default */
  onSetDefault: (id: string) => Promise<void>;
  /** Callback to clear selection */
  onClear: () => void;
  /** Whether selector is disabled */
  disabled?: boolean;
}

/**
 * Stack selector with default badge and lifecycle state indicators.
 */
export function StackSelector({
  stacks,
  selectedStackId,
  defaultStackId,
  onSelect,
  onSetDefault,
  onClear,
  disabled = false,
}: StackSelectorProps) {
  const hasStacks = stacks.length > 0;
  const isDefault = selectedStackId && defaultStackId === selectedStackId;

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <Label htmlFor="stack" className="flex items-center gap-1">
          Stack{' '}
          {isDefault && (
            <Badge variant="outline" className="text-xs ml-1">
              Default
            </Badge>
          )}
          <GlossaryTooltip termId="inference-stack">
            <span className="cursor-help text-muted-foreground hover:text-foreground">
              <HelpCircle className="h-3 w-3" />
            </span>
          </GlossaryTooltip>
        </Label>
        <div className="flex items-center gap-2">
          {selectedStackId && selectedStackId !== defaultStackId && (
            <Button
              variant="outline"
              size="sm"
              onClick={() => onSetDefault(selectedStackId)}
              className="h-6 text-xs"
              title="Set as default stack for this workspace"
            >
              Set Default
            </Button>
          )}
          {selectedStackId && (
            <Button variant="ghost" size="sm" onClick={onClear} className="h-6 text-xs">
              Clear
            </Button>
          )}
        </div>
      </div>

      <Select
        value={selectedStackId || '_none'}
        onValueChange={(value) => onSelect(value === '_none' ? '' : value)}
        disabled={disabled}
      >
        <SelectTrigger id="stack">
          <SelectValue placeholder={hasStacks ? 'Select stack...' : 'No stacks available'} />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="_none">None (Use individual adapters)</SelectItem>
          {stacks
            .filter((stack) => {
              const state = stack.lifecycle_state?.toLowerCase() || 'active';
              return state === 'active' || state === 'draft';
            })
            .map((stack) => {
              const state = stack.lifecycle_state?.toLowerCase() || 'active';
              const stateConfig: Record<
                string,
                { variant: 'default' | 'secondary' | 'outline'; className: string }
              > = {
                active: { variant: 'default', className: 'bg-green-500 text-white' },
                draft: { variant: 'secondary', className: 'bg-blue-500 text-white' },
              };
              const config = stateConfig[state] || stateConfig.active;

              return (
                <SelectItem key={stack.id} value={stack.id}>
                  <div className="flex items-center gap-2">
                    <Layers className="h-4 w-4" aria-hidden="true" />
                    <span>{stack.name}</span>
                    <Badge variant={config.variant} className={`text-xs ${config.className}`}>
                      {state.charAt(0).toUpperCase() + state.slice(1)}
                    </Badge>
                    {defaultStackId === stack.id && (
                      <Badge variant="secondary" className="text-xs">
                        Default
                      </Badge>
                    )}
                    <span className="text-xs text-muted-foreground ml-auto">
                      ({stack.adapter_ids?.length || 0} adapters)
                    </span>
                  </div>
                </SelectItem>
              );
            })}
        </SelectContent>
      </Select>

      <p className="text-xs text-muted-foreground">
        {selectedStackId
          ? 'Using adapters from selected stack. Stack adapters will be shown below.'
          : 'Stacks are reusable combinations of adapters. Select a stack to use its configured adapters for inference.'}
      </p>
    </div>
  );
}
