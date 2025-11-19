import React, { useState } from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../ui/card';
import { Input } from '../ui/input';
import { Label } from '../ui/label';
import { Textarea } from '../ui/textarea';
import { Checkbox } from '../ui/checkbox';
import { Badge } from '../ui/badge';
import { Slider } from '../ui/slider';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '../ui/select';
import { Separator } from '../ui/separator';
import { Settings, FileText, Filter, Sparkles } from 'lucide-react';

interface PreprocessingOptions {
  removeDuplicates: boolean;
  normalizeWhitespace: boolean;
  filterByTokenCount: boolean;
  minTokens?: number;
  maxTokens?: number;
  removeEmptyFiles: boolean;
  stripComments: boolean;
}

interface TokenizationSettings {
  model?: string;
  maxLength?: number;
  truncation: boolean;
  padding: boolean;
}

interface DatasetConfigData {
  name: string;
  description: string;
  format: 'patches' | 'jsonl' | 'txt' | 'custom';
  preprocessing: PreprocessingOptions;
  tokenization?: TokenizationSettings;
  metadata?: Record<string, string>;
}

interface DatasetConfigProps {
  initialConfig?: Partial<DatasetConfigData>;
  onChange: (config: DatasetConfigData) => void;
  disabled?: boolean;
}

const DEFAULT_CONFIG: DatasetConfigData = {
  name: '',
  description: '',
  format: 'jsonl',
  preprocessing: {
    removeDuplicates: true,
    normalizeWhitespace: true,
    filterByTokenCount: false,
    minTokens: 10,
    maxTokens: 8192,
    removeEmptyFiles: true,
    stripComments: false,
  },
  tokenization: {
    maxLength: 2048,
    truncation: true,
    padding: false,
  },
};

export const DatasetConfig: React.FC<DatasetConfigProps> = ({
  initialConfig,
  onChange,
  disabled = false,
}) => {
  const [config, setConfig] = useState<DatasetConfigData>({
    ...DEFAULT_CONFIG,
    ...initialConfig,
    preprocessing: {
      ...DEFAULT_CONFIG.preprocessing,
      ...initialConfig?.preprocessing,
    },
    tokenization: {
      ...DEFAULT_CONFIG.tokenization,
      ...initialConfig?.tokenization,
    },
  });

  const updateConfig = (updates: Partial<DatasetConfigData>) => {
    const newConfig = { ...config, ...updates };
    setConfig(newConfig);
    onChange(newConfig);
  };

  const updatePreprocessing = (updates: Partial<PreprocessingOptions>) => {
    updateConfig({
      preprocessing: { ...config.preprocessing, ...updates },
    });
  };

  const updateTokenization = (updates: Partial<TokenizationSettings>) => {
    updateConfig({
      tokenization: { ...config.tokenization!, ...updates },
    });
  };

  return (
    <div className="space-y-6">
      {/* Basic Information */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <FileText className="h-5 w-5" />
            Dataset Information
          </CardTitle>
          <CardDescription>
            Basic metadata for the training dataset
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="dataset-name">Dataset Name *</Label>
            <Input
              id="dataset-name"
              placeholder="e.g., code-review-dataset-v1"
              value={config.name}
              onChange={e => updateConfig({ name: e.target.value })}
              disabled={disabled}
            />
            <p className="text-xs text-muted-foreground">
              A unique identifier for this dataset
            </p>
          </div>

          <div className="space-y-2">
            <Label htmlFor="dataset-description">Description</Label>
            <Textarea
              id="dataset-description"
              placeholder="Describe the purpose and contents of this dataset..."
              value={config.description}
              onChange={e => updateConfig({ description: e.target.value })}
              disabled={disabled}
              rows={3}
            />
          </div>

          <div className="space-y-2">
            <Label htmlFor="dataset-format">Output Format</Label>
            <Select
              value={config.format}
              onValueChange={(value: any) => updateConfig({ format: value })}
              disabled={disabled}
            >
              <SelectTrigger id="dataset-format">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="jsonl">JSONL (Recommended)</SelectItem>
                <SelectItem value="patches">Git Patches</SelectItem>
                <SelectItem value="txt">Plain Text</SelectItem>
                <SelectItem value="custom">Custom Format</SelectItem>
              </SelectContent>
            </Select>
            <p className="text-xs text-muted-foreground">
              Format for the processed training data
            </p>
          </div>
        </CardContent>
      </Card>

      {/* Preprocessing Options */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Filter className="h-5 w-5" />
            Preprocessing Options
          </CardTitle>
          <CardDescription>
            Configure data cleaning and filtering
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="space-y-4">
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label htmlFor="remove-duplicates" className="cursor-pointer">
                  Remove Duplicates
                </Label>
                <p className="text-xs text-muted-foreground">
                  Filter out duplicate files based on content hash
                </p>
              </div>
              <Checkbox
                id="remove-duplicates"
                checked={config.preprocessing.removeDuplicates}
                onCheckedChange={checked =>
                  updatePreprocessing({ removeDuplicates: checked as boolean })
                }
                disabled={disabled}
              />
            </div>

            <Separator />

            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label htmlFor="normalize-whitespace" className="cursor-pointer">
                  Normalize Whitespace
                </Label>
                <p className="text-xs text-muted-foreground">
                  Standardize spacing and line endings
                </p>
              </div>
              <Checkbox
                id="normalize-whitespace"
                checked={config.preprocessing.normalizeWhitespace}
                onCheckedChange={checked =>
                  updatePreprocessing({ normalizeWhitespace: checked as boolean })
                }
                disabled={disabled}
              />
            </div>

            <Separator />

            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label htmlFor="remove-empty" className="cursor-pointer">
                  Remove Empty Files
                </Label>
                <p className="text-xs text-muted-foreground">
                  Skip files with no content
                </p>
              </div>
              <Checkbox
                id="remove-empty"
                checked={config.preprocessing.removeEmptyFiles}
                onCheckedChange={checked =>
                  updatePreprocessing({ removeEmptyFiles: checked as boolean })
                }
                disabled={disabled}
              />
            </div>

            <Separator />

            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label htmlFor="strip-comments" className="cursor-pointer">
                  Strip Comments
                </Label>
                <p className="text-xs text-muted-foreground">
                  Remove code comments (experimental)
                </p>
              </div>
              <Checkbox
                id="strip-comments"
                checked={config.preprocessing.stripComments}
                onCheckedChange={checked =>
                  updatePreprocessing({ stripComments: checked as boolean })
                }
                disabled={disabled}
              />
            </div>

            <Separator />

            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <div className="space-y-0.5">
                  <Label htmlFor="filter-tokens" className="cursor-pointer">
                    Filter by Token Count
                  </Label>
                  <p className="text-xs text-muted-foreground">
                    Only include files within token range
                  </p>
                </div>
                <Checkbox
                  id="filter-tokens"
                  checked={config.preprocessing.filterByTokenCount}
                  onCheckedChange={checked =>
                    updatePreprocessing({ filterByTokenCount: checked as boolean })
                  }
                  disabled={disabled}
                />
              </div>

              {config.preprocessing.filterByTokenCount && (
                <div className="pl-6 space-y-4 border-l-2 border-muted">
                  <div className="space-y-2">
                    <div className="flex items-center justify-between">
                      <Label htmlFor="min-tokens">Minimum Tokens</Label>
                      <Badge variant="outline">
                        {config.preprocessing.minTokens}
                      </Badge>
                    </div>
                    <Input
                      id="min-tokens"
                      type="number"
                      min="1"
                      max={config.preprocessing.maxTokens}
                      value={config.preprocessing.minTokens}
                      onChange={e =>
                        updatePreprocessing({ minTokens: parseInt(e.target.value) || 10 })
                      }
                      disabled={disabled}
                    />
                  </div>

                  <div className="space-y-2">
                    <div className="flex items-center justify-between">
                      <Label htmlFor="max-tokens">Maximum Tokens</Label>
                      <Badge variant="outline">
                        {config.preprocessing.maxTokens}
                      </Badge>
                    </div>
                    <Input
                      id="max-tokens"
                      type="number"
                      min={config.preprocessing.minTokens}
                      max="100000"
                      value={config.preprocessing.maxTokens}
                      onChange={e =>
                        updatePreprocessing({ maxTokens: parseInt(e.target.value) || 8192 })
                      }
                      disabled={disabled}
                    />
                  </div>
                </div>
              )}
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Tokenization Settings */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Sparkles className="h-5 w-5" />
            Tokenization Settings
          </CardTitle>
          <CardDescription>
            Configure how text is tokenized for training
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <Label htmlFor="max-length">Maximum Sequence Length</Label>
              <Badge variant="outline">
                {config.tokenization?.maxLength} tokens
              </Badge>
            </div>
            <Slider
              id="max-length"
              value={[config.tokenization?.maxLength || 2048]}
              onValueChange={([value]) => updateTokenization({ maxLength: value })}
              min={128}
              max={8192}
              step={128}
              disabled={disabled}
            />
            <p className="text-xs text-muted-foreground">
              Maximum number of tokens per sequence
            </p>
          </div>

          <Separator />

          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <Label htmlFor="truncation" className="cursor-pointer">
                Enable Truncation
              </Label>
              <p className="text-xs text-muted-foreground">
                Truncate sequences longer than max length
              </p>
            </div>
            <Checkbox
              id="truncation"
              checked={config.tokenization?.truncation}
              onCheckedChange={checked =>
                updateTokenization({ truncation: checked as boolean })
              }
              disabled={disabled}
            />
          </div>

          <Separator />

          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <Label htmlFor="padding" className="cursor-pointer">
                Enable Padding
              </Label>
              <p className="text-xs text-muted-foreground">
                Pad sequences shorter than max length
              </p>
            </div>
            <Checkbox
              id="padding"
              checked={config.tokenization?.padding}
              onCheckedChange={checked =>
                updateTokenization({ padding: checked as boolean })
              }
              disabled={disabled}
            />
          </div>
        </CardContent>
      </Card>

      {/* Configuration Summary */}
      <Card className="border-primary/20 bg-primary/5">
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            <Settings className="h-4 w-4" />
            Configuration Summary
          </CardTitle>
        </CardHeader>
        <CardContent className="text-sm space-y-2">
          <div className="grid grid-cols-2 gap-2">
            <div className="text-muted-foreground">Format:</div>
            <div className="font-medium">{config.format.toUpperCase()}</div>

            <div className="text-muted-foreground">Remove Duplicates:</div>
            <div className="font-medium">
              {config.preprocessing.removeDuplicates ? 'Yes' : 'No'}
            </div>

            <div className="text-muted-foreground">Filter by Tokens:</div>
            <div className="font-medium">
              {config.preprocessing.filterByTokenCount
                ? `${config.preprocessing.minTokens}-${config.preprocessing.maxTokens}`
                : 'No'}
            </div>

            <div className="text-muted-foreground">Max Sequence Length:</div>
            <div className="font-medium">{config.tokenization?.maxLength} tokens</div>
          </div>
        </CardContent>
      </Card>
    </div>
  );
};

export default DatasetConfig;
