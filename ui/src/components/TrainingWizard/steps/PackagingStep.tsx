import React from 'react';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { useTrainingWizardContext } from '@/components/TrainingWizard/context';

export function PackagingStep() {
  const { state, updateState } = useTrainingWizardContext();

  return (
    <div className="space-y-4">
      <div className="flex items-center space-x-2">
        <Checkbox
          id="packageAfter"
          checked={!!state.packageAfter}
          onCheckedChange={(checked) => updateState({ packageAfter: !!checked })}
        />
        <Label htmlFor="packageAfter">Package adapter after training</Label>
      </div>

      <div className="flex items-center space-x-2">
        <Checkbox
          id="registerAfter"
          checked={!!state.registerAfter}
          onCheckedChange={(checked) => updateState({ registerAfter: !!checked })}
        />
        <Label htmlFor="registerAfter">Register adapter after packaging</Label>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        <div className="space-y-2">
          <Label htmlFor="adaptersRoot">Adapters Root</Label>
          <Input
            id="adaptersRoot"
            placeholder="./adapters"
            value={state.adaptersRoot || ''}
            onChange={(e) => updateState({ adaptersRoot: e.target.value })}
          />
        </div>
        <div className="space-y-2">
          <Label htmlFor="adapterId">Adapter ID (optional)</Label>
          <Input
            id="adapterId"
            placeholder="my-awesome-adapter"
            value={state.adapterId || ''}
            onChange={(e) => updateState({ adapterId: e.target.value })}
          />
        </div>
      </div>

      <div className="space-y-2">
        <Label htmlFor="tier">Tier</Label>
        <Select value={state.tier || 'warm'} onValueChange={(value) => updateState({ tier: value })}>
          <SelectTrigger id="tier">
            <SelectValue placeholder="Select tier" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="persistent">Persistent</SelectItem>
            <SelectItem value="warm">Warm</SelectItem>
            <SelectItem value="ephemeral">Ephemeral</SelectItem>
          </SelectContent>
        </Select>
        <p className="text-xs text-muted-foreground">Tier used for registration (persistent, warm, or ephemeral)</p>
      </div>
    </div>
  );
}
