import React from 'react';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { useTrainingWizardContext } from '@/components/TrainingWizard/context';

export function SimpleTrainingParamsStep() {
  const { state, updateState } = useTrainingWizardContext();

  const handleNumberChange = (field: 'rank' | 'alpha' | 'epochs', value: string) => {
    updateState({ [field]: parseInt(value) || (field === 'epochs' ? 3 : field === 'rank' ? 8 : 16) });
  };

  return (
    <div className="space-y-6">
      <p className="text-sm text-muted-foreground">
        Configure essential training parameters. Advanced options are available in advanced mode.
      </p>
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <div className="space-y-2">
          <div className="flex items-center gap-1">
            <Label htmlFor="rank">Rank (r)</Label>
            <GlossaryTooltip brief="Controls capacity of learned patterns. Higher = more expressive but slower. Start with 8-16 for most tasks." />
          </div>
          <Input
            id="rank"
            type="number"
            value={state.rank}
            onChange={(e) => handleNumberChange('rank', e.target.value)}
          />
          <p className="text-xs text-muted-foreground">LoRA rank dimension (typically 4-32)</p>
        </div>

        <div className="space-y-2">
          <div className="flex items-center gap-1">
            <Label htmlFor="alpha">Alpha</Label>
            <GlossaryTooltip brief="Controls how strongly adapter influences model. Usually keep at 2x your Rank value." />
          </div>
          <Input
            id="alpha"
            type="number"
            value={state.alpha}
            onChange={(e) => handleNumberChange('alpha', e.target.value)}
          />
          <p className="text-xs text-muted-foreground">LoRA scaling factor (typically 2r)</p>
        </div>

        <div className="space-y-2">
          <div className="flex items-center gap-1">
            <Label htmlFor="epochs">Epochs</Label>
            <GlossaryTooltip brief="Number of times to repeat training data. More = better learning but risk of overfitting. Start with 3-5." />
          </div>
          <Input
            id="epochs"
            type="number"
            value={state.epochs}
            onChange={(e) => handleNumberChange('epochs', e.target.value)}
          />
        </div>
      </div>
    </div>
  );
}
