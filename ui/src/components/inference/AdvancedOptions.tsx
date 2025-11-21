import React from 'react';
import { Button } from '../ui/button';
import { Label } from '../ui/label';
import { Slider } from '../ui/slider';
import { Input } from '../ui/input';
import { Checkbox } from '../ui/checkbox';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '../ui/collapsible';
import { HelpTooltip } from '@/components/ui/help-tooltip';
import { Settings2, ChevronDown, HelpCircle } from 'lucide-react';

export interface AdvancedOptionsValues {
  max_tokens: number;
  temperature: number;
  top_k: number;
  top_p: number;
  seed?: number;
  require_evidence: boolean;
}

export interface AdvancedOptionsProps {
  values: AdvancedOptionsValues;
  onChange: (values: AdvancedOptionsValues) => void;
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
}

export function AdvancedOptions({
  values,
  onChange,
  isOpen,
  onOpenChange
}: AdvancedOptionsProps) {
  return (
    <Collapsible open={isOpen} onOpenChange={onOpenChange}>
      <CollapsibleTrigger asChild>
        <Button
          variant="ghost"
          className="w-full justify-between"
          aria-label="Toggle advanced options"
          aria-expanded={isOpen}
        >
          <span className="flex items-center gap-2">
            <Settings2 className="h-4 w-4" aria-hidden="true" />
            Advanced Options
          </span>
          <ChevronDown className={`h-4 w-4 transition-transform ${isOpen ? 'rotate-180' : ''}`} />
        </Button>
      </CollapsibleTrigger>
      <CollapsibleContent className="space-y-4 pt-4">
        <div className="space-y-2">
          <div className="flex justify-between">
            <Label className="flex items-center gap-1">
              Max Tokens
              <HelpTooltip helpId="inference-max-tokens">
                <span className="cursor-help text-muted-foreground hover:text-foreground">
                  <HelpCircle className="h-3 w-3" />
                </span>
              </HelpTooltip>
            </Label>
            <span className="text-sm text-muted-foreground">{values.max_tokens}</span>
          </div>
          <Slider
            value={[values.max_tokens || 100]}
            onValueChange={(v) => onChange({ ...values, max_tokens: v[0] })}
            min={10}
            max={2000}
            step={10}
          />
        </div>

        <div className="space-y-2">
          <div className="flex justify-between">
            <Label className="flex items-center gap-1">
              Temperature
              <HelpTooltip helpId="inference-temperature">
                <span className="cursor-help text-muted-foreground hover:text-foreground">
                  <HelpCircle className="h-3 w-3" />
                </span>
              </HelpTooltip>
            </Label>
            <span className="text-sm text-muted-foreground">{values.temperature?.toFixed(2)}</span>
          </div>
          <Slider
            value={[values.temperature || 0.7]}
            onValueChange={(v) => onChange({ ...values, temperature: v[0] })}
            min={0}
            max={2}
            step={0.1}
          />
        </div>

        <div className="space-y-2">
          <div className="flex justify-between">
            <Label className="flex items-center gap-1">
              Top K
              <HelpTooltip helpId="inference-top-k">
                <span className="cursor-help text-muted-foreground hover:text-foreground">
                  <HelpCircle className="h-3 w-3" />
                </span>
              </HelpTooltip>
            </Label>
            <span className="text-sm text-muted-foreground">{values.top_k}</span>
          </div>
          <Slider
            value={[values.top_k || 50]}
            onValueChange={(v) => onChange({ ...values, top_k: v[0] })}
            min={1}
            max={100}
            step={1}
          />
        </div>

        <div className="space-y-2">
          <div className="flex justify-between">
            <Label className="flex items-center gap-1">
              Top P
              <HelpTooltip helpId="inference-top-p">
                <span className="cursor-help text-muted-foreground hover:text-foreground">
                  <HelpCircle className="h-3 w-3" />
                </span>
              </HelpTooltip>
            </Label>
            <span className="text-sm text-muted-foreground">{values.top_p?.toFixed(2)}</span>
          </div>
          <Slider
            value={[values.top_p || 0.9]}
            onValueChange={(v) => onChange({ ...values, top_p: v[0] })}
            min={0}
            max={1}
            step={0.05}
          />
        </div>

        <div className="space-y-2">
          <Label htmlFor="seed" className="flex items-center gap-1">
            Seed (Optional)
            <HelpTooltip helpId="inference-seed">
              <span className="cursor-help text-muted-foreground hover:text-foreground">
                <HelpCircle className="h-3 w-3" />
              </span>
            </HelpTooltip>
          </Label>
          <Input
            id="seed"
            type="number"
            placeholder="Random seed"
            value={values.seed || ''}
            onChange={(e) => onChange({ ...values, seed: parseInt(e.target.value) || undefined })}
          />
        </div>

        <div className="flex items-center space-x-2">
          <Checkbox
            id="evidence"
            checked={values.require_evidence || false}
            onCheckedChange={(checked) => onChange({ ...values, require_evidence: !!checked })}
          />
          <Label htmlFor="evidence" className="flex items-center gap-1">
            Require Evidence (RAG)
            <HelpTooltip helpId="inference-evidence">
              <span className="cursor-help text-muted-foreground hover:text-foreground">
                <HelpCircle className="h-3 w-3" />
              </span>
            </HelpTooltip>
          </Label>
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}
