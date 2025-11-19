// 【ui/src/components/DatasetBuilder.tsx】 - Dataset building with preprocessing and tokenization
import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './ui/card';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Button } from './ui/button';
import { Checkbox } from './ui/checkbox';
import { Textarea } from './ui/textarea';
import { Alert, AlertDescription } from './ui/alert';
import { Badge } from './ui/badge';
import { Slider } from './ui/slider';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from './ui/accordion';
import {
  Code,
  AlertTriangle,
  CheckCircle,
  Info,
  RefreshCw,
  Zap,
  Copy,
} from 'lucide-react';
import { toast } from 'sonner';
import apiClient from '../api/client';
import { logger, toError } from '../utils/logger';

export interface PreprocessingOptions {
  removeComments: boolean;
  removeWhitespace: boolean;
  removeSpecialChars: boolean;
  minTokenLength: number;
  maxTokenLength: number;
  deduplication: 'none' | 'exact' | 'fuzzy';
  normalizeIndentation: boolean;
  removeBlankLines: boolean;
}

export interface DatasetBuilderState {
  trainingData: string;
  datasetName: string;
  preprocessing: PreprocessingOptions;
  tokenizationPreview: {
    originalTokenCount: number;
    processedTokenCount: number;
    sampleTokens: Array<{ id: number; token: string; decoded: string }>;
    statistics: {
      avgTokenLength: number;
      maxTokenLength: number;
      minTokenLength: number;
      uniqueTokens: number;
    };
  };
}

const DEFAULT_PREPROCESSING: PreprocessingOptions = {
  removeComments: false,
  removeWhitespace: false,
  removeSpecialChars: false,
  minTokenLength: 1,
  maxTokenLength: 512,
  deduplication: 'none',
  normalizeIndentation: false,
  removeBlankLines: false,
};

export interface DatasetBuilderProps {
  onDatasetCreated?: (datasetId: string, config: DatasetBuilderState) => void;
  initialData?: string;
}

export function DatasetBuilder({
  onDatasetCreated,
  initialData = '',
}: DatasetBuilderProps) {
  const [state, setState] = useState<DatasetBuilderState>({
    trainingData: initialData,
    datasetName: '',
    preprocessing: DEFAULT_PREPROCESSING,
    tokenizationPreview: {
      originalTokenCount: 0,
      processedTokenCount: 0,
      sampleTokens: [],
      statistics: {
        avgTokenLength: 0,
        maxTokenLength: 0,
        minTokenLength: 0,
        uniqueTokens: 0,
      },
    },
  });

  const [isProcessing, setIsProcessing] = useState(false);
  const [error, setError] = useState<Error | null>(null);

  // Generate tokenization preview
  useEffect(() => {
    if (state.trainingData) {
      generateTokenizationPreview();
    }
  }, [state.trainingData, state.preprocessing]);

  const generateTokenizationPreview = async () => {
    try {
      setIsProcessing(true);
      setError(null);

      // Simulate tokenization (in real implementation, call backend API)
      const preview = simulateTokenization(
        state.trainingData,
        state.preprocessing
      );

      setState((prev) => ({
        ...prev,
        tokenizationPreview: preview,
      }));
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Tokenization failed');
      setError(error);
      logger.error('Tokenization preview failed', {
        component: 'DatasetBuilder',
        operation: 'generateTokenizationPreview',
      }, toError(err));
    } finally {
      setIsProcessing(false);
    }
  };

  const simulateTokenization = (
    text: string,
    options: PreprocessingOptions
  ) => {
    let processed = text;

    // Apply preprocessing
    if (options.removeComments) {
      processed = processed.replace(/\/\/.*$/gm, '').replace(/\/\*[\s\S]*?\*\//g, '');
    }
    if (options.removeBlankLines) {
      processed = processed.replace(/^\s*[\r\n]/gm, '');
    }
    if (options.normalizeIndentation) {
      processed = processed.replace(/^\s+/gm, '  ');
    }
    if (options.removeWhitespace) {
      processed = processed.replace(/\s+/g, ' ');
    }

    // Tokenize (split by whitespace and punctuation)
    const tokens = processed
      .split(/[\s\W]+/)
      .filter((t) => t.length > 0)
      .filter((t) => t.length >= options.minTokenLength && t.length <= options.maxTokenLength);

    // Calculate statistics
    const uniqueTokens = new Set(tokens);
    const avgTokenLength = tokens.length > 0
      ? tokens.reduce((sum, t) => sum + t.length, 0) / tokens.length
      : 0;
    const tokenLengths = tokens.map((t) => t.length);
    const maxTokenLength = tokenLengths.length > 0 ? Math.max(...tokenLengths) : 0;
    const minTokenLength = tokenLengths.length > 0 ? Math.min(...tokenLengths) : 0;

    // Generate sample tokens (first 10 unique)
    const sampleTokens = Array.from(uniqueTokens)
      .slice(0, 10)
      .map((token, idx) => ({
        id: idx,
        token,
        decoded: token,
      }));

    return {
      originalTokenCount: text.split(/\s+/).length,
      processedTokenCount: tokens.length,
      sampleTokens,
      statistics: {
        avgTokenLength: parseFloat(avgTokenLength.toFixed(2)),
        maxTokenLength,
        minTokenLength,
        uniqueTokens: uniqueTokens.size,
      },
    };
  };

  const handlePreprocessingChange = (
    key: keyof PreprocessingOptions,
    value: any
  ) => {
    setState((prev) => ({
      ...prev,
      preprocessing: {
        ...prev.preprocessing,
        [key]: value,
      },
    }));
  };

  const handleCreateDataset = async () => {
    try {
      if (!state.datasetName.trim()) {
        toast.error('Please enter a dataset name');
        return;
      }
      if (!state.trainingData.trim()) {
        toast.error('Please provide training data');
        return;
      }

      setIsProcessing(true);
      setError(null);

      // In a real implementation, call backend to create dataset
      // const dataset = await apiClient.createDataset({
      //   name: state.datasetName,
      //   content: state.trainingData,
      //   preprocessing: state.preprocessing,
      // });

      // For now, simulate success
      const datasetId = `dataset_${Date.now()}`;

      toast.success(`Dataset "${state.datasetName}" created successfully`);

      if (onDatasetCreated) {
        onDatasetCreated(datasetId, state);
      }
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Failed to create dataset');
      setError(error);
      logger.error('Dataset creation failed', {
        component: 'DatasetBuilder',
        operation: 'createDataset',
        datasetName: state.datasetName,
      }, toError(err));
      toast.error(error.message);
    } finally {
      setIsProcessing(false);
    }
  };

  const copyTokenSample = () => {
    const sample = state.tokenizationPreview.sampleTokens
      .map((t) => t.token)
      .join(', ');
    navigator.clipboard.writeText(sample);
    toast.success('Sample tokens copied to clipboard');
  };

  return (
    <div className="space-y-6">
      {/* Training Data Input */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Code className="h-5 w-5" />
            Training Data
          </CardTitle>
          <CardDescription>
            Provide the raw text or code data to train on
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="dataset-name">Dataset Name</Label>
            <Input
              id="dataset-name"
              placeholder="e.g., my-training-dataset"
              value={state.datasetName}
              onChange={(e) =>
                setState((prev) => ({
                  ...prev,
                  datasetName: e.target.value,
                }))
              }
            />
          </div>

          <div className="space-y-2">
            <Label htmlFor="training-data">
              Raw Data ({state.trainingData.length} characters)
            </Label>
            <Textarea
              id="training-data"
              placeholder="Paste your training data here..."
              value={state.trainingData}
              onChange={(e) =>
                setState((prev) => ({
                  ...prev,
                  trainingData: e.target.value,
                }))
              }
              rows={8}
              className="font-mono text-sm"
            />
            <p className="text-xs text-muted-foreground">
              Minimum 50 characters recommended for meaningful training
            </p>
          </div>
        </CardContent>
      </Card>

      {/* Preprocessing Options */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Zap className="h-5 w-5" />
            Preprocessing Options
          </CardTitle>
          <CardDescription>
            Configure how the raw data will be processed
          </CardDescription>
        </CardHeader>
        <CardContent>
          <Accordion type="single" collapsible defaultValue="text-cleaning">
            {/* Text Cleaning */}
            <AccordionItem value="text-cleaning">
              <AccordionTrigger>
                <span>Text Cleaning</span>
              </AccordionTrigger>
              <AccordionContent className="space-y-4 pt-4">
                <div className="space-y-3">
                  <div className="flex items-center space-x-2">
                    <Checkbox
                      id="remove-comments"
                      checked={state.preprocessing.removeComments}
                      onCheckedChange={(checked) =>
                        handlePreprocessingChange('removeComments', checked)
                      }
                    />
                    <Label
                      htmlFor="remove-comments"
                      className="font-normal cursor-pointer"
                    >
                      Remove Comments
                    </Label>
                  </div>

                  <div className="flex items-center space-x-2">
                    <Checkbox
                      id="remove-blank-lines"
                      checked={state.preprocessing.removeBlankLines}
                      onCheckedChange={(checked) =>
                        handlePreprocessingChange('removeBlankLines', checked)
                      }
                    />
                    <Label
                      htmlFor="remove-blank-lines"
                      className="font-normal cursor-pointer"
                    >
                      Remove Blank Lines
                    </Label>
                  </div>

                  <div className="flex items-center space-x-2">
                    <Checkbox
                      id="normalize-indentation"
                      checked={state.preprocessing.normalizeIndentation}
                      onCheckedChange={(checked) =>
                        handlePreprocessingChange('normalizeIndentation', checked)
                      }
                    />
                    <Label
                      htmlFor="normalize-indentation"
                      className="font-normal cursor-pointer"
                    >
                      Normalize Indentation (to 2 spaces)
                    </Label>
                  </div>

                  <div className="flex items-center space-x-2">
                    <Checkbox
                      id="remove-whitespace"
                      checked={state.preprocessing.removeWhitespace}
                      onCheckedChange={(checked) =>
                        handlePreprocessingChange('removeWhitespace', checked)
                      }
                    />
                    <Label
                      htmlFor="remove-whitespace"
                      className="font-normal cursor-pointer"
                    >
                      Collapse Whitespace
                    </Label>
                  </div>
                </div>
              </AccordionContent>
            </AccordionItem>

            {/* Token Filtering */}
            <AccordionItem value="token-filtering">
              <AccordionTrigger>
                <span>Token Filtering</span>
              </AccordionTrigger>
              <AccordionContent className="space-y-4 pt-4">
                <div className="space-y-4">
                  <div className="space-y-2">
                    <Label htmlFor="min-token-length">
                      Minimum Token Length: {state.preprocessing.minTokenLength}
                    </Label>
                    <Slider
                      id="min-token-length"
                      min={1}
                      max={20}
                      step={1}
                      value={[state.preprocessing.minTokenLength]}
                      onValueChange={(value) =>
                        handlePreprocessingChange('minTokenLength', value[0])
                      }
                    />
                    <p className="text-xs text-muted-foreground">
                      Tokens shorter than this will be filtered out
                    </p>
                  </div>

                  <div className="space-y-2">
                    <Label htmlFor="max-token-length">
                      Maximum Token Length: {state.preprocessing.maxTokenLength}
                    </Label>
                    <Slider
                      id="max-token-length"
                      min={50}
                      max={512}
                      step={10}
                      value={[state.preprocessing.maxTokenLength]}
                      onValueChange={(value) =>
                        handlePreprocessingChange('maxTokenLength', value[0])
                      }
                    />
                    <p className="text-xs text-muted-foreground">
                      Tokens longer than this will be filtered out
                    </p>
                  </div>
                </div>
              </AccordionContent>
            </AccordionItem>

            {/* Deduplication */}
            <AccordionItem value="deduplication">
              <AccordionTrigger>
                <span>Deduplication</span>
              </AccordionTrigger>
              <AccordionContent className="space-y-4 pt-4">
                <div className="space-y-2">
                  <Label htmlFor="deduplication">Strategy</Label>
                  <Select
                    value={state.preprocessing.deduplication}
                    onValueChange={(value: any) =>
                      handlePreprocessingChange(
                        'deduplication',
                        value
                      )
                    }
                  >
                    <SelectTrigger id="deduplication">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="none">No Deduplication</SelectItem>
                      <SelectItem value="exact">Exact Match Removal</SelectItem>
                      <SelectItem value="fuzzy">Fuzzy Matching (Experimental)</SelectItem>
                    </SelectContent>
                  </Select>
                  <p className="text-xs text-muted-foreground">
                    Remove duplicate tokens to improve training efficiency
                  </p>
                </div>
              </AccordionContent>
            </AccordionItem>
          </Accordion>
        </CardContent>
      </Card>

      {/* Tokenization Preview */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <RefreshCw className="h-5 w-5" />
            Tokenization Preview
          </CardTitle>
          <CardDescription>
            Preview of how your data will be tokenized
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-6">
          {error && (
            <Alert variant="destructive">
              <AlertTriangle className="h-4 w-4" />
              <AlertDescription>{error.message}</AlertDescription>
            </Alert>
          )}

          {/* Statistics Cards */}
          <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
            <Card className="bg-muted">
              <CardContent className="pt-6">
                <div className="space-y-1">
                  <p className="text-xs text-muted-foreground">Original Tokens</p>
                  <p className="text-2xl font-bold">
                    {state.tokenizationPreview.originalTokenCount}
                  </p>
                </div>
              </CardContent>
            </Card>

            <Card className="bg-muted">
              <CardContent className="pt-6">
                <div className="space-y-1">
                  <p className="text-xs text-muted-foreground">Processed Tokens</p>
                  <p className="text-2xl font-bold">
                    {state.tokenizationPreview.processedTokenCount}
                  </p>
                </div>
              </CardContent>
            </Card>

            <Card className="bg-muted">
              <CardContent className="pt-6">
                <div className="space-y-1">
                  <p className="text-xs text-muted-foreground">Unique Tokens</p>
                  <p className="text-2xl font-bold">
                    {state.tokenizationPreview.statistics.uniqueTokens}
                  </p>
                </div>
              </CardContent>
            </Card>

            <Card className="bg-muted">
              <CardContent className="pt-6">
                <div className="space-y-1">
                  <p className="text-xs text-muted-foreground">Avg Length</p>
                  <p className="text-2xl font-bold">
                    {state.tokenizationPreview.statistics.avgTokenLength.toFixed(1)}
                  </p>
                </div>
              </CardContent>
            </Card>
          </div>

          {/* Sample Tokens */}
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <Label className="text-base">Sample Tokens (First 10 Unique)</Label>
              <Button
                size="sm"
                variant="outline"
                onClick={copyTokenSample}
                disabled={state.tokenizationPreview.sampleTokens.length === 0}
              >
                <Copy className="h-4 w-4 mr-2" />
                Copy
              </Button>
            </div>

            {state.tokenizationPreview.sampleTokens.length > 0 ? (
              <div className="space-y-2">
                {state.tokenizationPreview.sampleTokens.map((sample) => (
                  <div
                    key={sample.id}
                    className="flex items-center gap-2 p-2 bg-muted rounded-lg"
                  >
                    <Badge variant="outline" className="font-mono text-xs">
                      ID: {sample.id}
                    </Badge>
                    <code className="text-sm flex-1">{sample.token}</code>
                    <span className="text-xs text-muted-foreground">
                      → {sample.decoded}
                    </span>
                  </div>
                ))}
              </div>
            ) : (
              <p className="text-sm text-muted-foreground">
                No tokens generated yet. Add training data to see preview.
              </p>
            )}
          </div>

          {/* Token Statistics */}
          <div className="grid grid-cols-2 gap-4 text-sm">
            <div className="p-3 bg-muted rounded-lg">
              <p className="text-muted-foreground">Max Token Length</p>
              <p className="font-mono font-bold">
                {state.tokenizationPreview.statistics.maxTokenLength}
              </p>
            </div>
            <div className="p-3 bg-muted rounded-lg">
              <p className="text-muted-foreground">Min Token Length</p>
              <p className="font-mono font-bold">
                {state.tokenizationPreview.statistics.minTokenLength}
              </p>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Action Buttons */}
      <div className="flex justify-end gap-2">
        <Button
          variant="outline"
          onClick={() => {
            setState((prev) => ({
              ...prev,
              preprocessing: DEFAULT_PREPROCESSING,
            }));
            toast.success('Preprocessing options reset');
          }}
          disabled={isProcessing}
        >
          Reset Preprocessing
        </Button>
        <Button
          onClick={handleCreateDataset}
          disabled={
            isProcessing ||
            !state.trainingData.trim() ||
            !state.datasetName.trim()
          }
        >
          {isProcessing ? 'Creating Dataset...' : 'Create Dataset'}
        </Button>
      </div>

      {/* Info Alert */}
      <Alert>
        <Info className="h-4 w-4" />
        <AlertDescription>
          The dataset will be validated after creation. Processing may take a few moments
          depending on the size of your training data.
        </AlertDescription>
      </Alert>
    </div>
  );
}
