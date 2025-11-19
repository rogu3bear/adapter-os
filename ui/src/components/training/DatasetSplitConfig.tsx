import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../ui/card';
import { Label } from '../ui/label';
import { Input } from '../ui/input';
import { Slider } from '../ui/slider';
import { Alert, AlertDescription } from '../ui/alert';
import { Badge } from '../ui/badge';
import { BarChart3, AlertCircle, CheckCircle2 } from 'lucide-react';

interface DatasetSplit {
  train: number;
  validation: number;
  test: number;
}

interface DatasetSplitConfigProps {
  totalExamples: number;
  initialSplit?: DatasetSplit;
  onChange: (split: DatasetSplit) => void;
  disabled?: boolean;
}

const DEFAULT_SPLIT: DatasetSplit = {
  train: 80,
  validation: 10,
  test: 10,
};

export const DatasetSplitConfig: React.FC<DatasetSplitConfigProps> = ({
  totalExamples,
  initialSplit = DEFAULT_SPLIT,
  onChange,
  disabled = false,
}) => {
  const [split, setSplit] = useState<DatasetSplit>(initialSplit);
  const [editMode, setEditMode] = useState<'slider' | 'input'>('slider');

  // Validate that splits sum to 100
  const total = split.train + split.validation + split.test;
  const isValid = Math.abs(total - 100) < 0.01; // Allow for floating point errors

  // Calculate actual example counts
  const trainCount = Math.round((split.train / 100) * totalExamples);
  const valCount = Math.round((split.validation / 100) * totalExamples);
  const testCount = totalExamples - trainCount - valCount; // Ensure total matches exactly

  useEffect(() => {
    if (isValid) {
      onChange(split);
    }
  }, [split, isValid, onChange]);

  const handleSliderChange = (values: number[]) => {
    // Update train percentage, adjust validation and test proportionally
    const newTrain = values[0];
    const remaining = 100 - newTrain;
    const valRatio = split.validation / (split.validation + split.test || 1);

    setSplit({
      train: newTrain,
      validation: remaining * valRatio,
      test: remaining * (1 - valRatio),
    });
  };

  const handleInputChange = (field: keyof DatasetSplit, value: string) => {
    const numValue = parseFloat(value);
    if (isNaN(numValue) || numValue < 0 || numValue > 100) return;

    setSplit(prev => ({
      ...prev,
      [field]: numValue,
    }));
  };

  const resetToDefault = () => {
    setSplit(DEFAULT_SPLIT);
  };

  const presets = [
    { name: 'Default', split: { train: 80, validation: 10, test: 10 } },
    { name: '70/15/15', split: { train: 70, validation: 15, test: 15 } },
    { name: '90/5/5', split: { train: 90, validation: 5, test: 5 } },
    { name: '85/10/5', split: { train: 85, validation: 10, test: 5 } },
  ];

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <BarChart3 className="h-5 w-5" />
          Train/Validation/Test Split
        </CardTitle>
        <CardDescription>
          Configure how the dataset will be divided for training
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-6">
        {/* Visual Split Representation */}
        <div className="space-y-3">
          <div className="flex h-12 rounded-lg overflow-hidden border">
            <div
              className="bg-blue-500 flex items-center justify-center text-white text-sm font-medium transition-all"
              style={{ width: `${split.train}%` }}
            >
              {split.train > 15 && `${split.train.toFixed(1)}%`}
            </div>
            <div
              className="bg-green-500 flex items-center justify-center text-white text-sm font-medium transition-all"
              style={{ width: `${split.validation}%` }}
            >
              {split.validation > 10 && `${split.validation.toFixed(1)}%`}
            </div>
            <div
              className="bg-purple-500 flex items-center justify-center text-white text-sm font-medium transition-all"
              style={{ width: `${split.test}%` }}
            >
              {split.test > 10 && `${split.test.toFixed(1)}%`}
            </div>
          </div>

          <div className="flex justify-between text-sm">
            <div className="flex items-center gap-2">
              <div className="w-3 h-3 rounded-full bg-blue-500" />
              <span>Train</span>
            </div>
            <div className="flex items-center gap-2">
              <div className="w-3 h-3 rounded-full bg-green-500" />
              <span>Validation</span>
            </div>
            <div className="flex items-center gap-2">
              <div className="w-3 h-3 rounded-full bg-purple-500" />
              <span>Test</span>
            </div>
          </div>
        </div>

        {/* Slider Control */}
        <div className="space-y-2">
          <Label>Training Set Size</Label>
          <div className="flex items-center gap-4">
            <Slider
              value={[split.train]}
              onValueChange={handleSliderChange}
              min={50}
              max={95}
              step={1}
              disabled={disabled}
              className="flex-1"
            />
            <Badge variant="outline" className="min-w-[60px] justify-center">
              {split.train.toFixed(1)}%
            </Badge>
          </div>
          <p className="text-xs text-muted-foreground">
            Adjust the training set size. Validation and test sets will adjust proportionally.
          </p>
        </div>

        {/* Numeric Inputs */}
        <div className="grid grid-cols-3 gap-4">
          <div className="space-y-2">
            <Label htmlFor="train-input">Train %</Label>
            <Input
              id="train-input"
              type="number"
              min="0"
              max="100"
              step="0.1"
              value={split.train.toFixed(1)}
              onChange={e => handleInputChange('train', e.target.value)}
              disabled={disabled}
              className="font-mono"
            />
            <p className="text-xs text-muted-foreground">
              {trainCount.toLocaleString()} examples
            </p>
          </div>
          <div className="space-y-2">
            <Label htmlFor="val-input">Validation %</Label>
            <Input
              id="val-input"
              type="number"
              min="0"
              max="100"
              step="0.1"
              value={split.validation.toFixed(1)}
              onChange={e => handleInputChange('validation', e.target.value)}
              disabled={disabled}
              className="font-mono"
            />
            <p className="text-xs text-muted-foreground">
              {valCount.toLocaleString()} examples
            </p>
          </div>
          <div className="space-y-2">
            <Label htmlFor="test-input">Test %</Label>
            <Input
              id="test-input"
              type="number"
              min="0"
              max="100"
              step="0.1"
              value={split.test.toFixed(1)}
              onChange={e => handleInputChange('test', e.target.value)}
              disabled={disabled}
              className="font-mono"
            />
            <p className="text-xs text-muted-foreground">
              {testCount.toLocaleString()} examples
            </p>
          </div>
        </div>

        {/* Validation Alert */}
        {!isValid && (
          <Alert variant="destructive">
            <AlertCircle className="h-4 w-4" />
            <AlertDescription>
              Splits must sum to 100%. Current total: {total.toFixed(1)}%
            </AlertDescription>
          </Alert>
        )}

        {isValid && (
          <Alert className="border-green-500/20 bg-green-500/10">
            <CheckCircle2 className="h-4 w-4 text-green-500" />
            <AlertDescription className="text-green-500">
              Split configuration is valid. Total: {totalExamples.toLocaleString()} examples
            </AlertDescription>
          </Alert>
        )}

        {/* Presets */}
        <div className="space-y-2">
          <Label>Quick Presets</Label>
          <div className="flex flex-wrap gap-2">
            {presets.map(preset => (
              <Badge
                key={preset.name}
                variant="outline"
                className="cursor-pointer hover:bg-accent"
                onClick={() => !disabled && setSplit(preset.split)}
              >
                {preset.name}
              </Badge>
            ))}
          </div>
        </div>

        {/* Summary Statistics */}
        <div className="border-t pt-4 space-y-2">
          <div className="text-sm font-medium">Split Summary</div>
          <div className="grid grid-cols-3 gap-4 text-sm">
            <div>
              <div className="text-muted-foreground">Training</div>
              <div className="font-mono font-bold text-blue-500">
                {trainCount.toLocaleString()}
              </div>
            </div>
            <div>
              <div className="text-muted-foreground">Validation</div>
              <div className="font-mono font-bold text-green-500">
                {valCount.toLocaleString()}
              </div>
            </div>
            <div>
              <div className="text-muted-foreground">Test</div>
              <div className="font-mono font-bold text-purple-500">
                {testCount.toLocaleString()}
              </div>
            </div>
          </div>
        </div>
      </CardContent>
    </Card>
  );
};

export default DatasetSplitConfig;
