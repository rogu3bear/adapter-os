import React from 'react';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Textarea } from '@/components/ui/textarea';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { useTrainingWizardContext } from '../context';
import { Database, GitBranch, FileText, Code, Folder, CheckCircle, AlertTriangle } from 'lucide-react';

export function DataSourceStep() {
  const {
    state,
    updateState,
    dataSourceLocked,
    repositories,
    templates,
    datasets,
  } = useTrainingWizardContext();

  return (
    <div className="space-y-4">
      {dataSourceLocked && (
        <p className="text-sm text-muted-foreground">
          Dataset source is locked by the workflow. Training will use the selected dataset.
        </p>
      )}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-5 gap-4">
        <Card
          className={`transition-all ${
            state.dataSourceType === 'template' ? 'border-primary bg-primary/5' : ''
          } ${dataSourceLocked ? 'opacity-50 cursor-not-allowed' : 'cursor-pointer'}`}
          onClick={() => {
            if (dataSourceLocked) return;
            updateState({ dataSourceType: 'template' });
          }}
        >
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Database className="h-5 w-5" />
              Template
              {state.dataSourceType === 'template' && <CheckCircle className="h-4 w-4 text-primary ml-auto" />}
            </CardTitle>
            <CardDescription>Use a pre-configured training template</CardDescription>
          </CardHeader>
        </Card>

        <Card
          className={`transition-all ${
            state.dataSourceType === 'repository' ? 'border-primary bg-primary/5' : ''
          } ${dataSourceLocked ? 'opacity-50 cursor-not-allowed' : 'cursor-pointer'}`}
          onClick={() => {
            if (dataSourceLocked) return;
            updateState({ dataSourceType: 'repository' });
          }}
        >
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <GitBranch className="h-5 w-5" />
              Repository
              {state.dataSourceType === 'repository' && <CheckCircle className="h-4 w-4 text-primary ml-auto" />}
            </CardTitle>
            <CardDescription>Train from a registered repository</CardDescription>
          </CardHeader>
        </Card>

        <Card
          className={`transition-all ${
            state.dataSourceType === 'dataset' ? 'border-primary bg-primary/5' : ''
          } cursor-pointer`}
          onClick={() => updateState({ dataSourceType: 'dataset' })}
        >
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <FileText className="h-5 w-5" />
              Dataset
              {state.dataSourceType === 'dataset' && <CheckCircle className="h-4 w-4 text-primary ml-auto" />}
            </CardTitle>
            <CardDescription>Use an uploaded dataset</CardDescription>
          </CardHeader>
        </Card>

        <Card
          className={`transition-all ${
            state.dataSourceType === 'custom' ? 'border-primary bg-primary/5' : ''
          } ${dataSourceLocked ? 'opacity-50 cursor-not-allowed' : 'cursor-pointer'}`}
          onClick={() => {
            if (dataSourceLocked) return;
            updateState({ dataSourceType: 'custom' });
          }}
        >
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Code className="h-5 w-5" />
              Custom
              {state.dataSourceType === 'custom' && <CheckCircle className="h-4 w-4 text-primary ml-auto" />}
            </CardTitle>
            <CardDescription>Provide custom training data</CardDescription>
          </CardHeader>
        </Card>

        <Card
          className={`transition-all ${
            state.dataSourceType === 'directory' ? 'border-primary bg-primary/5' : ''
          } ${dataSourceLocked ? 'opacity-50 cursor-not-allowed' : 'cursor-pointer'}`}
          onClick={() => {
            if (dataSourceLocked) return;
            updateState({ dataSourceType: 'directory' });
          }}
        >
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Folder className="h-5 w-5" />
              Directory
              {state.dataSourceType === 'directory' && <CheckCircle className="h-4 w-4 text-primary ml-auto" />}
            </CardTitle>
            <CardDescription>Train from a directory on the system</CardDescription>
          </CardHeader>
        </Card>
      </div>

      {state.dataSourceType === 'directory' && (
        <div className="space-y-4 pt-4 border-t">
          <Alert>
            <CheckCircle className="h-4 w-4" />
            <AlertDescription>
              Directory-based training uses the codegraph analyzer to automatically build training examples from your code directory.
              No pre-tokenized JSON required!
            </AlertDescription>
          </Alert>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div className="space-y-2">
              <Label htmlFor="directoryRoot">Directory Root (Absolute Path)</Label>
              <Input
                id="directoryRoot"
                placeholder="/absolute/path/to/repo"
                value={state.directoryRoot || ''}
                onChange={(e) => updateState({ directoryRoot: e.target.value })}
              />
              <p className="text-xs text-muted-foreground">
                Absolute path to the repository root directory. Required for directory-based training.
              </p>
            </div>
            <div className="space-y-2">
              <Label htmlFor="directoryPath">Subdirectory (Optional)</Label>
              <Input
                id="directoryPath"
                placeholder="src or . (for entire repo)"
                value={state.directoryPath || ''}
                onChange={(e) => updateState({ directoryPath: e.target.value })}
              />
              <p className="text-xs text-muted-foreground">
                Relative path under root. Defaults to "." (entire repository).
              </p>
            </div>
          </div>
          {state.directoryRoot && (
            <div className="p-3 bg-muted rounded-lg">
              <p className="text-sm font-medium">Training Path:</p>
              <p className="text-xs text-muted-foreground font-mono">
                {state.directoryRoot}
                {state.directoryPath ? `/${state.directoryPath}` : ''}
              </p>
            </div>
          )}
        </div>
      )}

      {state.dataSourceType === 'template' && (
        <div className="space-y-2">
          <Label htmlFor="template">Select Template</Label>
          <Select value={state.templateId} onValueChange={(value) => updateState({ templateId: value })}>
            <SelectTrigger id="template">
              <SelectValue placeholder="Choose a template..." />
            </SelectTrigger>
            <SelectContent>
              {templates.filter(template => template.id).map((template) => (
                <SelectItem key={template.id} value={template.id}>
                  {template.name} - {template.description}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      )}

      {state.dataSourceType === 'repository' && (
        <div className="space-y-2">
          <Label htmlFor="repository">Select Repository</Label>
          <Select value={state.repositoryId} onValueChange={(value) => updateState({ repositoryId: value })}>
            <SelectTrigger id="repository">
              <SelectValue placeholder="Choose a repository..." />
            </SelectTrigger>
            <SelectContent>
              {repositories.filter(repo => repo.id).map((repo) => (
                <SelectItem key={repo.id} value={repo.id}>
                  {repo.url} ({repo.branch})
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      )}

      {state.dataSourceType === 'dataset' && (
        <div className="space-y-2">
          <Label htmlFor="dataset">Select Dataset</Label>
          <Select value={state.datasetId} onValueChange={(value) => updateState({ datasetId: value })}>
            <SelectTrigger id="dataset">
              <SelectValue placeholder="Choose a dataset..." />
            </SelectTrigger>
            <SelectContent>
              {datasets.length === 0 ? (
                <SelectItem value="" disabled>No datasets available</SelectItem>
              ) : (
                datasets.map((dataset) => (
                  <SelectItem key={dataset.id} value={dataset.id}>
                    <div className="flex items-center justify-between gap-2 w-full">
                      <div className="flex items-center gap-2">
                        <span>{dataset.name}</span>
                        <Badge variant="outline" className="text-xs">
                          {dataset.validation_status}
                        </Badge>
                      </div>
                      <span className="text-xs text-muted-foreground ml-auto">
                        {dataset.file_count} files • {Math.round(dataset.total_size_bytes / 1024 / 1024)} MB
                      </span>
                    </div>
                  </SelectItem>
                ))
              )}
            </SelectContent>
          </Select>
          {datasets.length === 0 && (
            <p className="text-xs text-muted-foreground">
              No datasets available. Upload a dataset first from the Datasets page.
            </p>
          )}
          {state.datasetId && (() => {
            const selectedDataset = datasets.find(d => d.id === state.datasetId);
            if (selectedDataset && selectedDataset.validation_status !== 'valid') {
              return (
                <Alert variant="destructive">
                  <AlertTriangle className="h-4 w-4" />
                  <AlertDescription>
                    <p className="font-medium">Dataset "{selectedDataset.name}" must be validated before training.</p>
                    <p className="text-sm mt-1">Current status: {selectedDataset.validation_status}.</p>
                    {selectedDataset.validation_errors && (
                      <p className="text-sm mt-2 font-mono">{selectedDataset.validation_errors}</p>
                    )}
                    <p className="text-sm mt-2">Please validate the dataset from the Datasets page.</p>
                  </AlertDescription>
                </Alert>
              );
            }
            return null;
          })()}
        </div>
      )}

      {state.dataSourceType === 'custom' && (
        <div className="space-y-2">
          <Label htmlFor="customData">Custom Training Data</Label>
          <Textarea
            id="customData"
            placeholder="Paste or enter your training data here..."
            value={state.customData || ''}
            onChange={(e) => updateState({ customData: e.target.value })}
            rows={10}
          />
        </div>
      )}

      <div className="space-y-2">
        <Label htmlFor="datasetPath">Dataset Path (optional)</Label>
        <Input
          id="datasetPath"
          placeholder="e.g., data/code_to_db_training.json"
          value={state.datasetPath || ''}
          onChange={(e) => updateState({ datasetPath: e.target.value })}
        />
        <p className="text-xs text-muted-foreground">If provided, the orchestrator will load examples from this JSON file.</p>
      </div>
    </div>
  );
}
