import React from 'react';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { HelpTooltip } from '@/components/ui/help-tooltip';
import { useTrainingWizardContext } from '../context';
import { LANGUAGES } from '../constants';

export function CategoryConfigStep() {
  const { state, updateState } = useTrainingWizardContext();

  if (!state.category) {
    return <div>No category selected</div>;
  }

  if (state.category === 'code') {
    return (
      <div className="space-y-4">
        <div className="space-y-2">
          <Label htmlFor="language">Programming Language</Label>
          <Select
            value={state.language}
            onValueChange={(value) => updateState({ language: value })}
          >
            <SelectTrigger id="language">
              <SelectValue placeholder="Select language..." />
            </SelectTrigger>
            <SelectContent>
              {LANGUAGES.map((lang) => (
                <SelectItem key={lang} value={lang.toLowerCase()}>
                  {lang}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        <div className="space-y-2">
          <Label>Symbol Targets (Optional)</Label>
          <Input
            placeholder="Enter symbols to target, comma-separated"
            value={state.symbolTargets?.join(', ') || ''}
            onChange={(e) =>
              updateState({
                symbolTargets: e.target.value.split(',').map((s) => s.trim()).filter(Boolean),
              })
            }
          />
          <p className="text-xs text-muted-foreground">
            Specific functions, classes, or modules to focus training on
          </p>
        </div>
      </div>
    );
  }

  if (state.category === 'framework') {
    return (
      <div className="space-y-4">
        <div className="space-y-2">
          <Label htmlFor="frameworkId">Framework</Label>
          <Input
            id="frameworkId"
            placeholder="e.g., react, django, rails"
            value={state.frameworkId || ''}
            onChange={(e) => updateState({ frameworkId: e.target.value })}
          />
        </div>

        <div className="space-y-2">
          <Label htmlFor="frameworkVersion">Version</Label>
          <Input
            id="frameworkVersion"
            placeholder="e.g., 18.0.0, 4.2, 7.0"
            value={state.frameworkVersion || ''}
            onChange={(e) => updateState({ frameworkVersion: e.target.value })}
          />
        </div>

        <div className="space-y-2">
          <Label>API Patterns (Optional)</Label>
          <Input
            placeholder="Enter API patterns, comma-separated"
            value={state.apiPatterns?.join(', ') || ''}
            onChange={(e) =>
              updateState({
                apiPatterns: e.target.value.split(',').map((s) => s.trim()).filter(Boolean),
              })
            }
          />
        </div>
      </div>
    );
  }

  if (state.category === 'codebase') {
    return (
      <div className="space-y-4">
        <div className="space-y-2">
          <Label htmlFor="repoScope">Repository Scope</Label>
          <Input
            id="repoScope"
            placeholder="e.g., src/, lib/, entire repo"
            value={state.repoScope || ''}
            onChange={(e) => updateState({ repoScope: e.target.value })}
          />
        </div>

        <div className="space-y-2">
          <Label>File Patterns (Include)</Label>
          <Input
            placeholder="e.g., **/*.ts, **/*.tsx"
            value={state.filePatterns?.join(', ') || ''}
            onChange={(e) =>
              updateState({
                filePatterns: e.target.value.split(',').map((s) => s.trim()).filter(Boolean),
              })
            }
          />
        </div>

        <div className="space-y-2">
          <Label>Exclude Patterns (Optional)</Label>
          <Input
            placeholder="e.g., **/node_modules/**, **/*.test.ts"
            value={state.excludePatterns?.join(', ') || ''}
            onChange={(e) =>
              updateState({
                excludePatterns: e.target.value.split(',').map((s) => s.trim()).filter(Boolean),
              })
            }
          />
        </div>
      </div>
    );
  }

  if (state.category === 'ephemeral') {
    return (
      <div className="space-y-4">
        <div className="space-y-2">
          <Label htmlFor="ttl">Time to Live (seconds)</Label>
          <Input
            id="ttl"
            type="number"
            placeholder="3600"
            value={state.ttlSeconds || ''}
            onChange={(e) => updateState({ ttlSeconds: parseInt(e.target.value) || undefined })}
          />
          <p className="text-xs text-muted-foreground">
            Adapter will be automatically evicted after this duration
          </p>
        </div>

        <div className="space-y-2">
          <div className="flex items-center gap-1">
            <Label htmlFor="contextWindow">Context Window (tokens)</Label>
            <HelpTooltip content="Maximum input length. 4096 tokens = ~3000 words. Longer = more context but more memory." />
          </div>
          <Input
            id="contextWindow"
            type="number"
            placeholder="4096"
            value={state.contextWindow || ''}
            onChange={(e) => updateState({ contextWindow: parseInt(e.target.value) || undefined })}
          />
        </div>
      </div>
    );
  }

  return null;
}
