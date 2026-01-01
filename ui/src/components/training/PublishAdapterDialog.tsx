/**
 * PublishAdapterDialog - Dialog for publishing an adapter version after training.
 *
 * Allows configuring:
 * - Display name
 * - Short description
 * - Attach mode (Free or Requires Dataset)
 * - Required dataset version (when attach mode is requires_dataset)
 */

import { useState, useEffect } from 'react';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Loader2, Send } from 'lucide-react';
import { usePublishAdapter } from '@/hooks/adapters';
import type { AttachMode, PublishAdapterResponse } from '@/api/adapter-types';
import type { TrainingJob } from '@/api/training-types';

interface PublishAdapterDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  trainingJob: TrainingJob;
  onPublished?: (response: PublishAdapterResponse) => void;
}

export function PublishAdapterDialog({
  open,
  onOpenChange,
  trainingJob,
  onPublished,
}: PublishAdapterDialogProps) {
  const publishAdapter = usePublishAdapter();

  // Form state
  const [name, setName] = useState('');
  const [shortDescription, setShortDescription] = useState('');
  const [attachMode, setAttachMode] = useState<AttachMode>('free');
  const [requiredScopeDatasetVersionId, setRequiredScopeDatasetVersionId] = useState('');

  // Reset form when dialog opens
  useEffect(() => {
    if (open) {
      // Pre-fill name from training job if available
      setName(trainingJob.adapter_name || '');
      setShortDescription('');
      setAttachMode('free');
      setRequiredScopeDatasetVersionId('');
    }
  }, [open, trainingJob]);

  // Get linked dataset versions from training job
  const linkedDatasetVersions = trainingJob.dataset_version_ids || [];

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    if (!trainingJob.repo_id || !trainingJob.produced_version_id) {
      return;
    }

    try {
      const response = await publishAdapter.mutateAsync({
        repoId: trainingJob.repo_id,
        versionId: trainingJob.produced_version_id,
        data: {
          name: name || undefined,
          short_description: shortDescription || undefined,
          attach_mode: attachMode,
          required_scope_dataset_version_id:
            attachMode === 'requires_dataset' ? requiredScopeDatasetVersionId : undefined,
        },
      });

      onPublished?.(response);
      onOpenChange(false);
    } catch {
      // Error is handled by the hook
    }
  };

  const isValid =
    attachMode === 'free' ||
    (attachMode === 'requires_dataset' && requiredScopeDatasetVersionId);

  const canPublish =
    trainingJob.repo_id &&
    trainingJob.produced_version_id &&
    isValid &&
    !publishAdapter.isPending;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md">
        <form onSubmit={handleSubmit}>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <Send className="h-5 w-5" />
              Publish Adapter
            </DialogTitle>
            <DialogDescription>
              Publish your trained adapter to make it available in inference stacks.
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4 py-4">
            {/* Name field */}
            <div className="space-y-2">
              <Label htmlFor="name">Name</Label>
              <Input
                id="name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="my-adapter-v1"
              />
              <p className="text-xs text-muted-foreground">
                Display name for the adapter (optional)
              </p>
            </div>

            {/* Short description field */}
            <div className="space-y-2">
              <Label htmlFor="description">Short Description</Label>
              <Textarea
                id="description"
                value={shortDescription}
                onChange={(e) => setShortDescription(e.target.value)}
                placeholder="Brief description of what this adapter does..."
                maxLength={280}
                rows={3}
              />
              <p className="text-xs text-muted-foreground">
                {shortDescription.length}/280 characters
              </p>
            </div>

            {/* Attach mode selector */}
            <div className="space-y-2">
              <Label htmlFor="attach-mode">Attach Mode</Label>
              <Select
                value={attachMode}
                onValueChange={(value) => setAttachMode(value as AttachMode)}
              >
                <SelectTrigger
                  id="attach-mode"
                  aria-required="true"
                  aria-label="Attach Mode"
                  aria-describedby="attach-mode-description"
                >
                  <SelectValue placeholder="Select attach mode" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="free">
                    Free - Can be attached to any stack
                  </SelectItem>
                  <SelectItem value="requires_dataset">
                    Requires Dataset - Must be scoped to a dataset
                  </SelectItem>
                </SelectContent>
              </Select>
              <p id="attach-mode-description" className="text-xs text-muted-foreground">
                {attachMode === 'free'
                  ? 'Adapter can be used in any stack without restrictions.'
                  : 'Adapter requires a specific dataset context when attached.'}
              </p>
            </div>

            {/* Dataset version selector (conditional) */}
            {attachMode === 'requires_dataset' && (
              <div className="space-y-2">
                <Label htmlFor="dataset-version">Required Dataset Version</Label>
                {linkedDatasetVersions.length > 0 ? (
                  <Select
                    value={requiredScopeDatasetVersionId}
                    onValueChange={setRequiredScopeDatasetVersionId}
                  >
                    <SelectTrigger
                      id="dataset-version"
                      aria-required="true"
                      aria-label="Required Dataset Version"
                      aria-describedby="dataset-version-description"
                    >
                      <SelectValue placeholder="Select dataset version" />
                    </SelectTrigger>
                    <SelectContent>
                      {linkedDatasetVersions.map((dsv) => (
                        <SelectItem
                          key={dsv.dataset_version_id}
                          value={dsv.dataset_version_id}
                        >
                          {dsv.dataset_version_id}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                ) : (
                  <Input
                    id="dataset-version"
                    value={requiredScopeDatasetVersionId}
                    onChange={(e) => setRequiredScopeDatasetVersionId(e.target.value)}
                    placeholder="Enter dataset version ID"
                    aria-required="true"
                    aria-label="Required Dataset Version"
                    aria-describedby="dataset-version-description"
                  />
                )}
                <p id="dataset-version-description" className="text-xs text-muted-foreground">
                  The dataset version this adapter was trained on.
                </p>
              </div>
            )}
          </div>

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
              disabled={publishAdapter.isPending}
            >
              Cancel
            </Button>
            <Button type="submit" disabled={!canPublish}>
              {publishAdapter.isPending ? (
                <>
                  <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                  Publishing...
                </>
              ) : (
                <>
                  <Send className="h-4 w-4 mr-2" />
                  Publish
                </>
              )}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
