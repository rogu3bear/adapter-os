import React from 'react';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Settings2, Check, AlertTriangle } from 'lucide-react';
import { PromptTemplate as PromptTemplateType } from '@/hooks/chat/usePromptTemplates';

export interface TemplateManagerProps {
  templates: PromptTemplateType[];
  recentTemplates: PromptTemplateType[];
  selectedTemplate: PromptTemplateType | null;
  templateVariables: Record<string, string>;
  showTemplates: boolean;
  showVariableInputs: boolean;
  promptModifiedSinceTemplate: boolean;
  onSelect: (template: PromptTemplateType) => void;
  onApplyVariables: () => void;
  onResetToTemplate: () => void;
  onSaveAsTemplate: () => void;
  onManageTemplates: () => void;
  onToggleTemplates: () => void;
  onCancelVariables: () => void;
  onVariableChange: (variable: string, value: string) => void;
  substituteVariables: (templateId: string, variables: Record<string, string>) => string | null;
}

export function TemplateManager({
  templates,
  recentTemplates,
  selectedTemplate,
  templateVariables,
  showTemplates,
  showVariableInputs,
  promptModifiedSinceTemplate,
  onSelect,
  onApplyVariables,
  onResetToTemplate,
  onSaveAsTemplate,
  onManageTemplates,
  onToggleTemplates,
  onCancelVariables,
  onVariableChange,
  substituteVariables,
}: TemplateManagerProps) {
  return (
    <>
      {/* Template Status Indicator */}
      {selectedTemplate && !promptModifiedSinceTemplate && (
        <Alert className="bg-blue-50 border-blue-200 text-sm">
          <Check className="h-4 w-4 text-blue-600" />
          <AlertDescription className="text-blue-800">
            Using template: <strong>{selectedTemplate.name}</strong>
            {selectedTemplate.variables.length > 0 && (
              <span className="ml-2">
                ({selectedTemplate.variables.length} variable{selectedTemplate.variables.length !== 1 ? 's' : ''})
              </span>
            )}
          </AlertDescription>
        </Alert>
      )}

      {selectedTemplate && promptModifiedSinceTemplate && (
        <Alert className="bg-yellow-50 border-yellow-200 text-sm">
          <AlertTriangle className="h-4 w-4 text-yellow-600" />
          <AlertDescription className="text-yellow-800">
            Prompt has been modified from template: <strong>{selectedTemplate.name}</strong>
            <Button
              variant="ghost"
              size="sm"
              onClick={onResetToTemplate}
              className="ml-2 h-6 text-xs"
            >
              Reset
            </Button>
          </AlertDescription>
        </Alert>
      )}

      {/* Template Selection and Management */}
      {showTemplates && (
        <div className="border rounded-md p-3 bg-muted/50 space-y-3">
          <div className="flex items-center justify-between">
            <div className="text-sm font-medium">Prompt Templates</div>
            <Button
              variant="outline"
              size="sm"
              onClick={onManageTemplates}
              className="h-7 text-xs gap-1"
            >
              <Settings2 className="h-3 w-3" />
              Manage
            </Button>
          </div>

          {/* Quick Access to Recent Templates */}
          {recentTemplates.length > 0 && (
            <div className="space-y-2">
              <div className="text-xs font-medium text-muted-foreground">Recent</div>
              <div className="space-y-1 max-h-32 overflow-y-auto">
                {recentTemplates.map((template) => (
                  <Button
                    key={template.id}
                    variant="ghost"
                    className="w-full justify-start text-left h-auto p-2 text-xs hover:bg-background"
                    onClick={() => onSelect(template)}
                  >
                    <div className="truncate">
                      <div className="font-medium">{template.name}</div>
                      <div className="text-xs text-muted-foreground line-clamp-1">
                        {template.description}
                      </div>
                    </div>
                  </Button>
                ))}
              </div>
            </div>
          )}

          <Button
            variant="outline"
            className="w-full text-xs"
            onClick={onManageTemplates}
          >
            View All Templates
          </Button>
        </div>
      )}

      {/* Variable Substitution Inputs */}
      {showVariableInputs && selectedTemplate && selectedTemplate.variables.length > 0 && (
        <div className="border rounded-md p-3 bg-blue-50 border-blue-200 space-y-3">
          <div className="text-sm font-medium">Enter Template Variables</div>
          <div className="space-y-2">
            {selectedTemplate.variables.map((variable) => (
              <div key={variable}>
                <Label htmlFor={`var-${variable}`} className="text-xs">
                  {variable}
                </Label>
                <Textarea
                  id={`var-${variable}`}
                  placeholder={`Enter ${variable}...`}
                  value={templateVariables[variable] || ''}
                  onChange={(e) => onVariableChange(variable, e.target.value)}
                  rows={2}
                  className="text-xs"
                />
              </div>
            ))}
          </div>

          {/* Real-time preview */}
          <div className="text-xs space-y-1">
            <div className="font-medium">Preview:</div>
            <pre className="bg-white p-2 rounded text-xs overflow-auto max-h-24 text-muted-foreground border">
              {substituteVariables(selectedTemplate.id, templateVariables) || selectedTemplate.prompt}
            </pre>
          </div>

          <div className="flex gap-2">
            <Button
              size="sm"
              onClick={onApplyVariables}
              className="flex-1 text-xs h-8"
            >
              Apply Template
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={onCancelVariables}
              className="text-xs h-8"
            >
              Cancel
            </Button>
          </div>
        </div>
      )}
    </>
  );
}
