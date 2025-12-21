import React from 'react';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { Slider } from '@/components/ui/slider';
import { Input } from '@/components/ui/input';
import { Checkbox } from '@/components/ui/checkbox';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { Settings2, ChevronDown, HelpCircle } from 'lucide-react';

export interface AdvancedOptionsValues {
  max_tokens: number;
  temperature: number;
  top_k: number;
  top_p: number;
  backend?: 'auto' | 'mlx' | 'coreml' | 'metal';
  seed?: number;
  require_evidence: boolean;
}

export interface AdvancedOptionsProps {
  values: AdvancedOptionsValues;
  onChange: (values: AdvancedOptionsValues) => void;
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  /** Hide backend selector when a higher-level picker is shown */
  hideBackendSelect?: boolean;
}

export function AdvancedOptions({
  values,
  onChange,
  isOpen,
  onOpenChange,
  hideBackendSelect = false
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
        {!hideBackendSelect && (
          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <Label className="flex items-center gap-1">
                Backend
                <GlossaryTooltip termId="inference-backend">
                  <span className="cursor-help text-muted-foreground hover:text-foreground">
                    <HelpCircle className="h-3 w-3" />
                  </span>
                </GlossaryTooltip>
              </Label>
              <span className="text-xs text-muted-foreground">Auto-selects by default</span>
            </div>
            <Select
              value={values.backend || 'auto'}
              onValueChange={(backend) => onChange({ ...values, backend: backend as AdvancedOptionsValues['backend'] })}
            >
              <SelectTrigger>
                <SelectValue placeholder="Auto (router decides)" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="auto">Auto (router decides)</SelectItem>
                <SelectItem value="mlx">MLX (real backend if available)</SelectItem>
                <SelectItem value="coreml">CoreML (ANE priority)</SelectItem>
                <SelectItem value="metal">Metal (GPU fallback)</SelectItem>
              </SelectContent>
            </Select>
            <p className="text-xs text-muted-foreground">
              Choose a backend explicitly or leave on Auto to let the server select the best available (CoreML/MLX/Metal).
            </p>
          </div>
        )}

        <div className="space-y-2">
          <div className="flex justify-between">
            <Label className="flex items-center gap-1">
              Max Tokens
              <GlossaryTooltip termId="inference-max-tokens">
                <span className="cursor-help text-muted-foreground hover:text-foreground">
                  <HelpCircle className="h-3 w-3" />
                </span>
              </GlossaryTooltip>
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
              <GlossaryTooltip termId="inference-temperature">
                <span className="cursor-help text-muted-foreground hover:text-foreground">
                  <HelpCircle className="h-3 w-3" />
                </span>
              </GlossaryTooltip>
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
              <GlossaryTooltip termId="inference-top-k">
                <span className="cursor-help text-muted-foreground hover:text-foreground">
                  <HelpCircle className="h-3 w-3" />
                </span>
              </GlossaryTooltip>
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
              <GlossaryTooltip termId="inference-top-p">
                <span className="cursor-help text-muted-foreground hover:text-foreground">
                  <HelpCircle className="h-3 w-3" />
                </span>
              </GlossaryTooltip>
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
            <GlossaryTooltip termId="inference-seed">
              <span className="cursor-help text-muted-foreground hover:text-foreground">
                <HelpCircle className="h-3 w-3" />
              </span>
            </GlossaryTooltip>
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
            <GlossaryTooltip termId="inference-evidence">
              <span className="cursor-help text-muted-foreground hover:text-foreground">
                <HelpCircle className="h-3 w-3" />
              </span>
            </GlossaryTooltip>
          </Label>
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}
