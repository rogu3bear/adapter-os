// Example integration of DatasetBuilder component
// This demonstrates how to use DatasetBuilder in a training workflow

import React, { useState } from 'react';
import { DatasetBuilder } from './DatasetBuilder';
import { Card, CardContent, CardHeader, CardTitle } from '../ui/card';
import { Button } from '../ui/button';
import { CheckCircle } from 'lucide-react';
import { useNavigate } from 'react-router-dom';

interface DatasetConfig {
  name: string;
  description?: string;
  strategy: 'identity' | 'question_answer' | 'masked_lm';
  maxSequenceLength: number;
  validationSplit: number;
  tokenizer?: string;
}

export function DatasetBuilderExample() {
  const navigate = useNavigate();
  const [createdDatasetId, setCreatedDatasetId] = useState<string | null>(null);
  const [datasetConfig, setDatasetConfig] = useState<DatasetConfig | null>(null);

  const handleDatasetCreated = (datasetId: string, config: DatasetConfig) => {
    setCreatedDatasetId(datasetId);
    setDatasetConfig(config);
    // Dataset creation logged for debugging
  };

  const handleCancel = () => {
    if (window.confirm('Are you sure you want to cancel? Any unsaved changes will be lost.')) {
      navigate('/training');
    }
  };

  const handleStartTraining = () => {
    if (createdDatasetId) {
      navigate(`/training/start?dataset=${createdDatasetId}`);
    }
  };

  // If dataset is created, show success state
  if (createdDatasetId && datasetConfig) {
    return (
      <div className="space-y-6 max-w-4xl mx-auto p-6">
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <CheckCircle className="w-6 h-6 text-green-500" />
              Dataset Created Successfully
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="grid grid-cols-2 gap-4 text-sm">
              <div>
                <p className="text-muted-foreground">Dataset ID</p>
                <p className="font-mono">{createdDatasetId}</p>
              </div>
              <div>
                <p className="text-muted-foreground">Name</p>
                <p className="font-medium">{datasetConfig.name}</p>
              </div>
              <div>
                <p className="text-muted-foreground">Strategy</p>
                <p className="font-medium capitalize">{datasetConfig.strategy.replace('_', ' ')}</p>
              </div>
              <div>
                <p className="text-muted-foreground">Max Sequence Length</p>
                <p className="font-medium">{datasetConfig.maxSequenceLength}</p>
              </div>
            </div>

            <div className="flex gap-3 pt-4">
              <Button variant="outline" onClick={() => setCreatedDatasetId(null)}>
                Create Another Dataset
              </Button>
              <Button onClick={handleStartTraining} className="flex-1">
                Start Training with this Dataset
              </Button>
            </div>
          </CardContent>
        </Card>
      </div>
    );
  }

  // Show dataset builder
  return (
    <div className="max-w-6xl mx-auto p-6">
      <DatasetBuilder
        onDatasetCreated={handleDatasetCreated}
        onCancel={handleCancel}
      />
    </div>
  );
}

// Example with pre-filled configuration
export function DatasetBuilderWithDefaults() {
  const navigate = useNavigate();

  return (
    <div className="max-w-6xl mx-auto p-6">
      <DatasetBuilder
        initialConfig={{
          name: 'my-code-dataset',
          description: 'Dataset for code completion training',
          strategy: 'identity',
          maxSequenceLength: 2048,
          validationSplit: 0.1
        }}
        onDatasetCreated={(datasetId, _config) => {
          navigate(`/training/configure?dataset=${datasetId}`);
        }}
        onCancel={() => navigate('/datasets')}
      />
    </div>
  );
}
