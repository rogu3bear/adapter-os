import React from 'react';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { HelpTooltip } from '@/components/ui/help-tooltip';
import { Checkbox } from '@/components/ui/checkbox';
import { useTrainingWizardContext } from '../context';
import { LORA_TARGETS } from '../constants';

export function TrainingParamsStep() {
  const { state, updateState } = useTrainingWizardContext();

  return (
    <div className="space-y-6">
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        <div className="space-y-2">
          <div className="flex items-center gap-1">
            <Label htmlFor="rank">Rank (r)</Label>
            <HelpTooltip content="Controls capacity of learned patterns. Higher = more expressive but slower. Start with 8-16 for most tasks." />
          </div>
          <Input
            id="rank"
            type="number"
            value={state.rank}
            onChange={(e) => updateState({ rank: parseInt(e.target.value) || 8 })}
          />
          <p className="text-xs text-muted-foreground">LoRA rank dimension (typically 4-32)</p>
        </div>

        <div className="space-y-2">
          <div className="flex items-center gap-1">
            <Label htmlFor="alpha">Alpha</Label>
            <HelpTooltip content="Controls how strongly adapter influences model. Usually keep at 2x your Rank value." />
          </div>
          <Input
            id="alpha"
            type="number"
            value={state.alpha}
            onChange={(e) => updateState({ alpha: parseInt(e.target.value) || 16 })}
          />
          <p className="text-xs text-muted-foreground">LoRA scaling factor (typically 2r)</p>
        </div>

        <div className="space-y-2">
          <div className="flex items-center gap-1">
            <Label htmlFor="epochs">Epochs</Label>
            <HelpTooltip content="Number of times to repeat training data. More = better learning but risk of overfitting. Start with 3-5." />
          </div>
          <Input
            id="epochs"
            type="number"
            value={state.epochs}
            onChange={(e) => updateState({ epochs: parseInt(e.target.value) || 3 })}
          />
        </div>

        <div className="space-y-2">
          <div className="flex items-center gap-1">
            <Label htmlFor="learningRate">Learning Rate</Label>
            <HelpTooltip content="How fast model learns. Too high = unstable, too low = slow. Default 0.0003 is safe for most cases." />
          </div>
          <Input
            id="learningRate"
            type="number"
            step="0.0001"
            value={state.learningRate}
            onChange={(e) => updateState({ learningRate: parseFloat(e.target.value) || 3e-4 })}
          />
        </div>

        <div className="space-y-2">
          <div className="flex items-center gap-1">
            <Label htmlFor="batchSize">Batch Size</Label>
            <HelpTooltip content="Number of examples processed together. Larger = faster but needs more memory. Default 4 is conservative." />
          </div>
          <Input
            id="batchSize"
            type="number"
            value={state.batchSize}
            onChange={(e) => updateState({ batchSize: parseInt(e.target.value) || 4 })}
          />
        </div>

        <div className="space-y-2">
          <div className="flex items-center gap-1">
            <Label htmlFor="warmupSteps">Warmup Steps (Optional)</Label>
            <HelpTooltip content="Gradually increase learning rate at start to stabilize training. Optional; helps with some datasets." />
          </div>
          <Input
            id="warmupSteps"
            type="number"
            placeholder="100"
            value={state.warmupSteps || ''}
            onChange={(e) => updateState({ warmupSteps: parseInt(e.target.value) || undefined })}
          />
        </div>
      </div>

      <div className="space-y-2">
        <Label>LoRA Target Modules</Label>
        <div className="grid grid-cols-2 md:grid-cols-3 gap-2">
          {LORA_TARGETS.map((target) => (
            <div key={target} className="flex items-center space-x-2">
              <Checkbox
                id={target}
                checked={state.targets.includes(target)}
                onCheckedChange={(checked) => {
                  if (checked) {
                    updateState({ targets: [...state.targets, target] });
                  } else {
                    updateState({ targets: state.targets.filter((t) => t !== target) });
                  }
                }}
              />
              <Label htmlFor={target} className="text-sm font-mono">
                {target}
              </Label>
            </div>
          ))}
        </div>
        <p className="text-xs text-muted-foreground mt-2">
          Selected: {state.targets.length} module{state.targets.length !== 1 ? 's' : ''}
        </p>
      </div>
    </div>
  );
}
