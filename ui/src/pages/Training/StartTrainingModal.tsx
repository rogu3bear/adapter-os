import { useEffect, useState } from 'react';
import { FormModal } from '@/components/shared/Modal';
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
import { Badge } from '@/components/ui/badge';
import { useTraining } from '@/hooks/training';
import { AlertCircle } from 'lucide-react';
import type { StartTrainingRequest, TrainingConfigRequest, DatasetVersionSelection } from '@/api/training-types';
import { useTenant } from '@/providers/FeatureProviders';

interface StartTrainingModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSuccess?: (jobId: string) => void;
}

export function StartTrainingModal({ open, onOpenChange, onSuccess }: StartTrainingModalProps) {
  const [adapterName, setAdapterName] = useState('');
  const [datasetId, setDatasetId] = useState('');
  const [templateId, setTemplateId] = useState('');
  const [learningRate, setLearningRate] = useState('0.0001');
  const [epochs, setEpochs] = useState('3');
  const [batchSize, setBatchSize] = useState('8');
  const [rank, setRank] = useState('16');
  const [alpha, setAlpha] = useState('32');
  const [datasetVersionSelections, setDatasetVersionSelections] = useState<DatasetVersionSelection[]>([]);

  const { data: datasetsData } = useTraining.useDatasets();
  const { data: templatesData } = useTraining.useTemplates();
  const { selectedTenant, tenants } = useTenant();

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
    setLearningRate('0.0001');
    setEpochs('3');
    setBatchSize('8');
    setRank('16');
    setAlpha('32');
  };

  const handleSubmit = async () => {
    // Check dataset validation status
    if (datasetId && selectedDataset && selectedDataset.validation_status !== 'valid') {
      return; // Will be prevented by disabled button, but add check for safety
    }
    if (!datasetId) {
      return;
    }
    if (datasetVersionMissing) {
      return;
    }

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
      ...(datasetVersionSelections.length > 0 && { dataset_version_ids: datasetVersionSelections }),
      ...(templateId && { template_id: templateId }),
    };

    await startTraining(request);
  };

  const datasets = datasetsData?.datasets || [];
  const templates = templatesData || [];
  const selectedDataset = datasets.find(d => d.id === datasetId);
  const selectedDatasetVersionId = selectedDataset?.dataset_version_id;
  const datasetVersionMissing = Boolean(datasetId) && Boolean(selectedDataset) && !selectedDatasetVersionId;
  const isDatasetValid = !datasetId || (selectedDataset?.validation_status === 'valid' && !datasetVersionMissing);
  const selectedTenantStatus = tenants.find((t) => t.id === selectedTenant)?.status?.toLowerCase();
  const isHighAssuranceTenant =
    selectedTenantStatus === 'production' || selectedTenantStatus === 'high_assurance';

  useEffect(() => {
    if (!datasetId && datasets.length > 0) {
      setDatasetId(datasets[0].id);
    }
  }, [datasets, datasetId]);

  useEffect(() => {
    if (selectedDatasetVersionId) {
      setDatasetVersionSelections([{ dataset_version_id: selectedDatasetVersionId, weight: 1 }]);
    } else {
      setDatasetVersionSelections([]);
    }
  }, [selectedDatasetVersionId]);

  return (
    <FormModal
      open={open}
      onOpenChange={onOpenChange}
      title="Start New Training Job"
      size="xl"
      onSubmit={handleSubmit}
      submitText="Start Training"
      isSubmitting={isPending}
      isValid={isDatasetValid && !!datasetId && !datasetVersionMissing}
      onCancel={resetForm}
    >
      {error && (
        <Alert variant="destructive">
          <AlertCircle className="h-4 w-4" />
          <AlertDescription>
            Failed to start training: {error.message}
          </AlertDescription>
        </Alert>
      )}
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
              <SelectValue placeholder="Select dataset (required)" />
              </SelectTrigger>
              <SelectContent>
                {datasets.map((dataset) => (
                  <SelectItem key={dataset.id} value={dataset.id}>
                    <div className="flex items-center gap-2">
                      <span>{dataset.name}</span>
                      <Badge variant="outline" className="text-xs">
                        {dataset.validation_status}
                      </Badge>
              <Badge variant="outline" className="text-xs">
                {dataset.dataset_version_id ?? 'no-version'}
              </Badge>
                    </div>
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            {selectedDataset && selectedDataset.validation_status !== 'valid' && (
              <Alert variant="destructive" className="mt-2">
                <AlertCircle className="h-4 w-4" />
                <AlertDescription>
                  Dataset must be validated before training. Current status: {selectedDataset.validation_status}
                </AlertDescription>
              </Alert>
            )}
            {datasetVersionMissing && (
              <Alert variant="destructive" className="mt-2">
                <AlertCircle className="h-4 w-4" />
                <AlertDescription>
                  This dataset has no version bound. Please create a dataset version before training.
                  {isHighAssuranceTenant ? ' High-assurance tenants require versioned lineage.' : ''}
                </AlertDescription>
              </Alert>
            )}
          </div>

          <div>
            <Label htmlFor="templateId">Template</Label>
            <Select value={templateId || "__none__"} onValueChange={(v) => setTemplateId(v === "__none__" ? "" : v)}>
              <SelectTrigger id="templateId">
                <SelectValue placeholder="Select template (optional)" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="__none__">None</SelectItem>
                {templates.map((template) => (
                  <SelectItem key={template.id} value={template.id}>
                    {template.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
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
    </FormModal>
  );
}
