import React from 'react';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { AlertTriangle } from 'lucide-react';
import type { AdapterScope } from '@/api/types';
import { useTrainingWizardContext } from '@/components/TrainingWizard/context';

export function BasicInfoStep() {
  const { state, updateState } = useTrainingWizardContext();

  return (
    <div className="space-y-4">
      <div className="space-y-2">
        <Label htmlFor="name">Adapter Name</Label>
        <Input
          id="name"
          placeholder="my-awesome-adapter"
          value={state.name}
          onChange={(e) => updateState({ name: e.target.value })}
        />
      </div>

      <div className="space-y-2">
        <Label htmlFor="description">Description</Label>
        <Textarea
          id="description"
          placeholder="What should this adapter be good at? (e.g., 'Answer questions about our API documentation')"
          value={state.description}
          onChange={(e) => updateState({ description: e.target.value })}
          rows={3}
        />
      </div>

      <div className="space-y-2">
        <Label htmlFor="scope">Scope</Label>
        <Select value={state.scope} onValueChange={(value: AdapterScope) => updateState({ scope: value })}>
          <SelectTrigger id="scope">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="global">Global - Available to all organizations</SelectItem>
            <SelectItem value="tenant">Workspace - Isolated to this workspace</SelectItem>
            <SelectItem value="repo">Repository - Scoped to a specific repository</SelectItem>
            <SelectItem value="commit">Commit - Scoped to a specific commit</SelectItem>
          </SelectContent>
        </Select>
      </div>

      {!state.name && (
        <Alert>
          <AlertTriangle className="h-4 w-4" />
          <AlertDescription>Adapter name is required</AlertDescription>
        </Alert>
      )}
    </div>
  );
}
