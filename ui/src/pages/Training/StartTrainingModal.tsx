import { useState } from 'react';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { useTraining } from '@/hooks/useTraining';
import { Brain, AlertCircle } from 'lucide-react';
import type { StartTrainingRequest, TrainingConfigRequest } from '@/api/training-types';

interface StartTrainingModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSuccess?: (jobId: string) => void;
}

export function StartTrainingModal({ open, onOpenChange, onSuccess }: StartTrainingModalProps) {
  const [adapterName, setAdapterName] = useState('');
  const [datasetId, setDatasetId] = useState('');
  const [templateId, setTemplateId] = useState('');
  const [repoId, setRepoId] = useState('');
  const [learningRate, setLearningRate] = useState('0.0001');
  const [epochs, setEpochs] = useState('3');
  const [batchSize, setBatchSize] = useState('8');
  const [rank, setRank] = useState('16');
  const [alpha, setAlpha] = useState('32');

  const { data: datasetsData } = useTraining.useDatasets();
  const { data: templatesData } = useTraining.useTemplates();

  const { mutateAsync: startTraining, isPending, error } = useTraining.useStartTraining({
    onSuccess: (job) => {
      onSuccess?.(job.id);
      onOpenChange(false);
      resetForm();
    },
  });

  const resetForm = () => {
    setAdapterName('');
    setDatasetId('');
    setTemplateId('');
    setRepoId('');
    setLearningRate('0.0001');
    setEpochs('3');
    setBatchSize('8');
    setRank('16');
    setAlpha('32');
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    const config: TrainingConfigRequest = {
      learning_rate: parseFloat(learningRate),
      epochs: parseInt(epochs),
      batch_size: parseInt(batchSize),
      rank: parseInt(rank),
      alpha: parseInt(alpha),
    };

    const request: StartTrainingRequest = {
      adapter_name: adapterName,
      config,
      ...(datasetId && { dataset_id: datasetId }),
      ...(templateId && { template_id: templateId }),
      ...(repoId && { repo_id: repoId }),
    };

    await startTraining(request);
  };

  const datasets = datasetsData?.datasets || [];
  const templates = templatesData || [];

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Brain className="h-5 w-5" />
            Start New Training Job
          </DialogTitle>
        </DialogHeader>

        {error && (
          <Alert variant="destructive">
            <AlertCircle className="h-4 w-4" />
            <AlertDescription>
              Failed to start training: {error.message}
            </AlertDescription>
          </Alert>
        )}

        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <Label htmlFor="adapterName">
              Adapter Name <span className="text-destructive">*</span>
            </Label>
            <Input
              id="adapterName"
              value={adapterName}
              onChange={(e) => setAdapterName(e.target.value)}
              placeholder="tenant-a/engineering/code-review/r001"
              required
            />
            <p className="text-xs text-muted-foreground mt-1">
              Format: tenant/domain/purpose/revision
            </p>
          </div>

          <div>
            <Label htmlFor="datasetId">Dataset</Label>
            <Select value={datasetId} onValueChange={setDatasetId}>
              <SelectTrigger id="datasetId">
                <SelectValue placeholder="Select dataset (optional)" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="">None</SelectItem>
                {datasets.map((dataset) => (
                  <SelectItem key={dataset.id} value={dataset.id}>
                    {dataset.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          <div>
            <Label htmlFor="templateId">Template</Label>
            <Select value={templateId} onValueChange={setTemplateId}>
              <SelectTrigger id="templateId">
                <SelectValue placeholder="Select template (optional)" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="">None</SelectItem>
                {templates.map((template) => (
                  <SelectItem key={template.id} value={template.id}>
                    {template.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          <div>
            <Label htmlFor="repoId">Repository ID</Label>
            <Input
              id="repoId"
              value={repoId}
              onChange={(e) => setRepoId(e.target.value)}
              placeholder="optional"
            />
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div>
              <Label htmlFor="learningRate">
                Learning Rate <span className="text-destructive">*</span>
              </Label>
              <Input
                id="learningRate"
                type="number"
                step="0.0001"
                value={learningRate}
                onChange={(e) => setLearningRate(e.target.value)}
                required
              />
            </div>

            <div>
              <Label htmlFor="epochs">
                Epochs <span className="text-destructive">*</span>
              </Label>
              <Input
                id="epochs"
                type="number"
                min="1"
                value={epochs}
                onChange={(e) => setEpochs(e.target.value)}
                required
              />
            </div>

            <div>
              <Label htmlFor="batchSize">
                Batch Size <span className="text-destructive">*</span>
              </Label>
              <Input
                id="batchSize"
                type="number"
                min="1"
                value={batchSize}
                onChange={(e) => setBatchSize(e.target.value)}
                required
              />
            </div>

            <div>
              <Label htmlFor="rank">
                LoRA Rank <span className="text-destructive">*</span>
              </Label>
              <Input
                id="rank"
                type="number"
                min="1"
                value={rank}
                onChange={(e) => setRank(e.target.value)}
                required
              />
            </div>

            <div>
              <Label htmlFor="alpha">
                LoRA Alpha <span className="text-destructive">*</span>
              </Label>
              <Input
                id="alpha"
                type="number"
                min="1"
                value={alpha}
                onChange={(e) => setAlpha(e.target.value)}
                required
              />
            </div>
          </div>

          <div className="flex justify-end gap-2 pt-4">
            <Button
              type="button"
              variant="outline"
              onClick={() => {
                onOpenChange(false);
                resetForm();
              }}
              disabled={isPending}
            >
              Cancel
            </Button>
            <Button type="submit" disabled={isPending}>
              {isPending ? (
                <>
                  <Brain className="h-4 w-4 mr-2 animate-pulse" />
                  Starting...
                </>
              ) : (
                <>
                  <Brain className="h-4 w-4 mr-2" />
                  Start Training
                </>
              )}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}
