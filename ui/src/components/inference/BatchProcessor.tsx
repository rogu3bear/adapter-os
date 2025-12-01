import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import { Input } from '@/components/ui/input';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Layers, AlertTriangle } from 'lucide-react';
import { BatchResults } from './BatchResults';
import { logger } from '@/utils/logger';
import { toast } from 'sonner';

export interface ValidationResult {
  valid: boolean;
  error?: string;
  warning?: string;
  suggestion?: string;
}

export interface BatchProcessorProps {
  prompts: string[];
  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- dynamic batch results from API
  results: any[];
  validation: ValidationResult[];
  isProcessing: boolean;
  config: {
    max_tokens: number;
    temperature: number;
    top_k: number;
    top_p?: number;
  };
  canExecute: boolean;
  onPromptsChange: (prompts: string[]) => void;
  onProcess: (prompts: string[]) => Promise<void>;
  onRetry: (itemId: string) => Promise<void>;
  onExportJSON: () => void;
  onExportCSV: () => void;
}

export function BatchProcessor({
  prompts,
  results,
  validation,
  isProcessing,
  config,
  canExecute,
  onPromptsChange,
  onProcess,
  onRetry,
  onExportJSON,
  onExportCSV,
}: BatchProcessorProps) {
  const handleFileUpload = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;

    const reader = new FileReader();
    reader.onload = (event) => {
      const text = event.target?.result as string;
      if (file.name.endsWith('.csv')) {
        // Parse CSV (simple approach - assumes prompts in first column)
        const lines = text.split('\n').slice(1); // Skip header
        const parsedPrompts = lines
          .map(line => line.split(',')[0].replace(/^"|"$/g, '').trim())
          .filter(p => p);
        onPromptsChange(parsedPrompts);
        logger.info('CSV file uploaded', {
          component: 'BatchProcessor',
          operation: 'uploadCSV',
          count: parsedPrompts.length,
        });
        toast.success(`Loaded ${parsedPrompts.length} prompts from file`);
      } else {
        // Plain text file
        const parsedPrompts = text.split('\n').filter(p => p.trim());
        onPromptsChange(parsedPrompts);
        logger.info('Text file uploaded', {
          component: 'BatchProcessor',
          operation: 'uploadText',
          count: parsedPrompts.length,
        });
        toast.success(`Loaded ${parsedPrompts.length} prompts from file`);
      }
    };
    reader.readAsText(file);
  };

  const validPromptCount = prompts.filter(p => p.trim()).length;

  return (
    <div className="space-y-6">
      <Card>
        <CardHeader>
          <CardTitle className="text-base flex items-center gap-2">
            <Layers className="h-5 w-5" />
            Batch Inference
          </CardTitle>
          <p className="text-sm text-muted-foreground">
            Process multiple prompts simultaneously with shared configuration
          </p>
        </CardHeader>
        <CardContent className="space-y-4">
          {/* Batch Prompts Input */}
          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <Label>Prompts (one per line or upload CSV)</Label>
              <Input
                type="file"
                accept=".csv,.txt"
                onChange={handleFileUpload}
                className="w-48 h-9 text-xs"
              />
            </div>
            <Textarea
              placeholder="Enter one prompt per line...
Write a Python function to calculate fibonacci
Explain quantum computing in simple terms
What is the capital of France?"
              value={prompts.join('\n')}
              onChange={(e) => onPromptsChange(e.target.value.split('\n').filter(p => p.trim()))}
              rows={8}
              className={validation.some(v => !v.valid) ? 'border-destructive' : ''}
            />
            <div className="flex items-center justify-between text-xs text-muted-foreground">
              <span>{validPromptCount} prompts ready for batch processing</span>
              {prompts.length > 100 && (
                <span className="text-yellow-600">Warning: Recommended max: 100 prompts</span>
              )}
            </div>

            {/* Batch validation errors */}
            {validation.some(v => !v.valid) && (
              <Alert variant="destructive" className="text-sm">
                <AlertTriangle className="h-4 w-4" />
                <AlertDescription>
                  <strong>Validation Errors:</strong>
                  <ul className="mt-1 space-y-1">
                    {validation
                      .map((v, index) => ({ validation: v, index }))
                      .filter(({ validation: v }) => !v.valid)
                      .slice(0, 3) // Show first 3 errors
                      .map(({ validation: v, index }) => (
                        <li key={index}>
                          Prompt {index + 1}: {v.error}
                        </li>
                      ))}
                    {validation.filter(v => !v.valid).length > 3 && (
                      <li>... and {validation.filter(v => !v.valid).length - 3} more</li>
                    )}
                  </ul>
                </AlertDescription>
              </Alert>
            )}

            {/* Batch validation warnings */}
            {validation.some(v => v.warning) && (
              <Alert variant="default" className="text-sm border-yellow-200 bg-yellow-50">
                <AlertTriangle className="h-4 w-4 text-yellow-600" />
                <AlertDescription className="text-yellow-800">
                  <strong>Warnings:</strong> Some prompts have warnings (long content, etc.)
                </AlertDescription>
              </Alert>
            )}
          </div>

          {/* Shared Configuration Preview */}
          <div className="p-3 bg-muted rounded-md">
            <h4 className="text-sm font-medium mb-2">Shared Configuration</h4>
            <div className="grid grid-cols-2 md:grid-cols-4 gap-2 text-xs">
              <div>Max Tokens: {config.max_tokens}</div>
              <div>Temperature: {config.temperature}</div>
              <div>Top K: {config.top_k}</div>
              <div>Top P: {config.top_p?.toFixed(2)}</div>
            </div>
          </div>

          <Button
            onClick={() => onProcess(prompts)}
            disabled={validPromptCount === 0 || isProcessing || !canExecute}
            className={`w-full ${!canExecute ? 'opacity-50 cursor-not-allowed' : ''}`}
            title={!canExecute ? 'Requires inference:execute permission' : undefined}
          >
            {isProcessing ? (
              <>
                <div className="animate-spin rounded-full h-4 w-4 border-b-2 border-white mr-2" />
                Processing Batch...
              </>
            ) : (
              <>
                <Layers className="h-4 w-4 mr-2" />
                Run Batch Inference ({validPromptCount} prompts)
              </>
            )}
          </Button>
        </CardContent>
      </Card>

      {/* Batch Results */}
      {results && results.length > 0 && (
        <BatchResults
          results={results}
          prompts={prompts}
          onRetry={onRetry}
          onExportJSON={onExportJSON}
          onExportCSV={onExportCSV}
        />
      )}
    </div>
  );
}
