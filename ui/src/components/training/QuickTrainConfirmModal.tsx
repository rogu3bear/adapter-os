/**
 * QuickTrainConfirmModal - Fast-path training confirmation for valid datasets
 *
 * Bypasses the full training wizard for validated datasets, providing:
 * - Dataset summary
 * - Preflight checks (client + server)
 * - Quick config options
 * - Collapsible advanced settings
 *
 * Achieves ≤3 click training from dataset detail page.
 */

import { useState, useMemo, useCallback, useId } from 'react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible';
import {
  Database,
  Play,
  Settings2,
  ChevronDown,
  ChevronRight,
  CheckCircle,
  XCircle,
  AlertTriangle,
  Info,
  Loader2,
  Shield,
} from 'lucide-react';
import { cn } from '@/lib/utils';
import { useTrainingPreflight } from '@/hooks/training';
import type { Dataset } from '@/api/training-types';
import type { PolicyCheck } from '@/components/PolicyPreflightDialog';
import { formatBytes } from '@/lib/formatters';

export interface QuickTrainConfig {
  adapterName: string;
  rank: number;
  alpha: number;
  epochs: number;
  learningRate: number;
  batchSize: number;
  targets: string[];
}

export interface QuickTrainConfirmModalProps {
  /** Whether the dialog is open */
  open: boolean;
  /** Callback when dialog open state changes */
  onOpenChange: (open: boolean) => void;
  /** Dataset to train on */
  dataset: Dataset;
  /** Callback when user confirms training */
  onConfirm: (config: QuickTrainConfig) => void;
  /** Callback when user cancels */
  onCancel: () => void;
  /** Callback to open advanced wizard instead */
  onAdvanced?: () => void;
  /** Whether training is in progress */
  isLoading?: boolean;
  /** Current tenant ID */
  tenantId?: string;
  /** Selected dataset version ID for preflight validation */
  selectedVersionId?: string;
}

/** Default training configuration */
const DEFAULT_CONFIG: QuickTrainConfig = {
  adapterName: '',
  rank: 8,
  alpha: 16,
  epochs: 3,
  learningRate: 3e-4,
  batchSize: 4,
  targets: ['q_proj', 'v_proj'],
};

/** Get icon for check severity */
function getCheckIcon(check: PolicyCheck) {
  if (check.passed) {
    return <CheckCircle className="h-4 w-4 text-green-500" />;
  }
  switch (check.severity) {
    case 'error':
      return <XCircle className="h-4 w-4 text-red-500" />;
    case 'warning':
      return <AlertTriangle className="h-4 w-4 text-yellow-500" />;
    default:
      return <Info className="h-4 w-4 text-blue-500" />;
  }
}

/** Generate default adapter name from dataset */
function generateAdapterName(dataset: Dataset): string {
  const baseName = dataset.name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '')
    .slice(0, 30);
  return `${baseName}-adapter`;
}

export function QuickTrainConfirmModal({
  open,
  onOpenChange,
  dataset,
  onConfirm,
  onCancel,
  onAdvanced,
  isLoading = false,
  tenantId = 'default',
  selectedVersionId,
}: QuickTrainConfirmModalProps) {
  const descriptionId = useId();

  // Config state
  const [config, setConfig] = useState<QuickTrainConfig>(() => ({
    ...DEFAULT_CONFIG,
    adapterName: generateAdapterName(dataset),
  }));
  const [showAdvanced, setShowAdvanced] = useState(false);

  // Preflight checks
  const preflight = useTrainingPreflight(dataset, { enabled: open, tenantId }, selectedVersionId);

  // Validation
  const isNameValid = config.adapterName.length >= 3 && /^[a-z0-9][a-z0-9-]*[a-z0-9]$/.test(config.adapterName);
  const canSubmit = preflight.canProceed && isNameValid && !isLoading && !preflight.isLoading;

  // Update adapter name when dataset changes
  useMemo(() => {
    if (open) {
      setConfig((prev) => ({
        ...prev,
        adapterName: generateAdapterName(dataset),
      }));
    }
  }, [dataset.id, open]);

  const handleConfirm = useCallback(() => {
    if (canSubmit) {
      onConfirm(config);
    }
  }, [canSubmit, config, onConfirm]);

  const handleCancel = useCallback(() => {
    setShowAdvanced(false);
    onCancel();
  }, [onCancel]);

  // Categorize checks for display
  const { failedChecks, passedChecks } = useMemo(() => {
    const failed = preflight.allChecks.filter((c) => !c.passed || c.severity === 'warning');
    const passed = preflight.allChecks.filter((c) => c.passed && c.severity !== 'warning');
    return { failedChecks: failed, passedChecks: passed };
  }, [preflight.allChecks]);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="max-w-xl max-h-[85vh] overflow-hidden flex flex-col"
        aria-describedby={descriptionId}
        data-testid="quick-train-modal"
      >
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Play className="h-5 w-5 text-primary" />
            Start Training
          </DialogTitle>
          <DialogDescription id={descriptionId}>
            Train a new adapter from the selected dataset.
          </DialogDescription>
        </DialogHeader>

        <ScrollArea className="flex-1 -mx-6 px-6">
          <div className="space-y-4 py-2">
            {/* Dataset Summary */}
            <div className="rounded-lg border p-4 bg-muted/30">
              <div className="flex items-start gap-3">
                <Database className="h-5 w-5 text-primary mt-0.5" />
                <div className="flex-1 min-w-0">
                  <h4 className="font-medium truncate">{dataset.name}</h4>
                  <div className="flex flex-wrap gap-x-4 gap-y-1 text-sm text-muted-foreground mt-1">
                    <span>{dataset.file_count} files</span>
                    <span>{formatBytes(dataset.total_size_bytes)}</span>
                    {dataset.total_tokens > 0 && (
                      <span>{dataset.total_tokens.toLocaleString()} tokens</span>
                    )}
                  </div>
                  <div className="flex items-center gap-2 mt-2">
                    <Badge
                      variant={dataset.validation_status === 'valid' ? 'default' : 'secondary'}
                      className="text-xs"
                    >
                      {dataset.validation_status}
                    </Badge>
                    {dataset.trust_state && (
                      <Badge
                        variant={
                          dataset.trust_state === 'allowed'
                            ? 'outline'
                            : dataset.trust_state === 'allowed_with_warning'
                              ? 'secondary'
                              : 'destructive'
                        }
                        className="text-xs"
                      >
                        {dataset.trust_state}
                      </Badge>
                    )}
                  </div>
                </div>
              </div>
            </div>

            {/* Preflight Checks */}
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <h4 className="text-sm font-medium flex items-center gap-2">
                  <Shield className="h-4 w-4" />
                  Preflight Checks
                </h4>
                {preflight.isLoading && (
                  <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
                )}
              </div>

              {/* Failed/Warning checks */}
              {failedChecks.length > 0 && (
                <div className="space-y-2">
                  {failedChecks.map((check) => (
                    <Alert
                      key={check.policy_id}
                      variant={check.severity === 'error' ? 'destructive' : 'default'}
                      className={cn(
                        'py-2',
                        check.severity === 'warning' && 'border-yellow-200 bg-yellow-50 dark:border-yellow-900 dark:bg-yellow-950'
                      )}
                    >
                      <div className="flex items-start gap-2">
                        {getCheckIcon(check)}
                        <div className="flex-1 min-w-0">
                          <AlertTitle className="text-sm font-medium">
                            {check.policy_name}
                          </AlertTitle>
                          <AlertDescription className="text-xs mt-0.5">
                            {check.message}
                            {check.details && (
                              <span className="block mt-1 text-muted-foreground">
                                {check.details}
                              </span>
                            )}
                          </AlertDescription>
                        </div>
                      </div>
                    </Alert>
                  ))}
                </div>
              )}

              {/* Passed checks summary */}
              {passedChecks.length > 0 && failedChecks.length === 0 && (
                <Alert
                  className="py-2 border-green-200 bg-green-50 dark:border-green-900 dark:bg-green-950"
                  data-testid="quick-train-preflight-passed"
                >
                  <CheckCircle className="h-4 w-4 text-green-500" />
                  <AlertTitle className="text-sm">All checks passed</AlertTitle>
                  <AlertDescription className="text-xs">
                    {passedChecks.length} checks verified. Ready to start training.
                  </AlertDescription>
                </Alert>
              )}
            </div>

            {/* Quick Config */}
            <div className="space-y-3">
              <h4 className="text-sm font-medium">Configuration</h4>

              <div className="space-y-2">
                <Label htmlFor="adapter-name" className="text-xs">
                  Adapter Name
                </Label>
                <Input
                  id="adapter-name"
                  data-testid="quick-train-adapter-name"
                  value={config.adapterName}
                  onChange={(e) =>
                    setConfig((prev) => ({ ...prev, adapterName: e.target.value.toLowerCase() }))
                  }
                  placeholder="my-adapter"
                  className={cn(!isNameValid && config.adapterName.length > 0 && 'border-red-500')}
                />
                {!isNameValid && config.adapterName.length > 0 && (
                  <p className="text-xs text-red-500">
                    Name must be 3+ chars, lowercase alphanumeric with hyphens, no leading/trailing hyphens.
                  </p>
                )}
              </div>

              <div className="grid grid-cols-3 gap-3">
                <div className="space-y-1">
                  <Label htmlFor="rank" className="text-xs">
                    Rank
                  </Label>
                  <Input
                    id="rank"
                    data-testid="quick-train-rank"
                    type="number"
                    min={4}
                    max={64}
                    value={config.rank}
                    onChange={(e) =>
                      setConfig((prev) => ({ ...prev, rank: parseInt(e.target.value) || 8 }))
                    }
                  />
                </div>
                <div className="space-y-1">
                  <Label htmlFor="alpha" className="text-xs">
                    Alpha
                  </Label>
                  <Input
                    id="alpha"
                    data-testid="quick-train-alpha"
                    type="number"
                    min={1}
                    max={128}
                    value={config.alpha}
                    onChange={(e) =>
                      setConfig((prev) => ({ ...prev, alpha: parseInt(e.target.value) || 16 }))
                    }
                  />
                </div>
                <div className="space-y-1">
                  <Label htmlFor="epochs" className="text-xs">
                    Epochs
                  </Label>
                  <Input
                    id="epochs"
                    data-testid="quick-train-epochs"
                    type="number"
                    min={1}
                    max={20}
                    value={config.epochs}
                    onChange={(e) =>
                      setConfig((prev) => ({ ...prev, epochs: parseInt(e.target.value) || 3 }))
                    }
                  />
                </div>
              </div>
            </div>

            {/* Advanced Options */}
            <Collapsible open={showAdvanced} onOpenChange={setShowAdvanced}>
              <CollapsibleTrigger asChild>
                <Button variant="ghost" size="sm" className="w-full justify-start gap-2 text-muted-foreground">
                  {showAdvanced ? (
                    <ChevronDown className="h-4 w-4" />
                  ) : (
                    <ChevronRight className="h-4 w-4" />
                  )}
                  <Settings2 className="h-4 w-4" />
                  Advanced Options
                </Button>
              </CollapsibleTrigger>
              <CollapsibleContent className="pt-2 space-y-3">
                <div className="grid grid-cols-2 gap-3">
                  <div className="space-y-1">
                    <Label htmlFor="learning-rate" className="text-xs">
                      Learning Rate
                    </Label>
                    <Input
                      id="learning-rate"
                      type="number"
                      step="0.0001"
                      min={0.00001}
                      max={0.01}
                      value={config.learningRate}
                      onChange={(e) =>
                        setConfig((prev) => ({
                          ...prev,
                          learningRate: parseFloat(e.target.value) || 3e-4,
                        }))
                      }
                    />
                  </div>
                  <div className="space-y-1">
                    <Label htmlFor="batch-size" className="text-xs">
                      Batch Size
                    </Label>
                    <Input
                      id="batch-size"
                      type="number"
                      min={1}
                      max={32}
                      value={config.batchSize}
                      onChange={(e) =>
                        setConfig((prev) => ({
                          ...prev,
                          batchSize: parseInt(e.target.value) || 4,
                        }))
                      }
                    />
                  </div>
                </div>
                <p className="text-xs text-muted-foreground">
                  For more options, use the{' '}
                  <button
                    type="button"
                    className="text-primary hover:underline"
                    onClick={onAdvanced}
                  >
                    full training wizard
                  </button>
                  .
                </p>
              </CollapsibleContent>
            </Collapsible>
          </div>
        </ScrollArea>

        <DialogFooter className="flex-row justify-between sm:justify-between">
          <Button
            variant="ghost"
            size="sm"
            onClick={onAdvanced}
            disabled={isLoading}
            data-testid="quick-train-advanced-btn"
          >
            Advanced...
          </Button>
          <div className="flex gap-2">
            <Button
              variant="outline"
              onClick={handleCancel}
              disabled={isLoading}
              data-testid="quick-train-cancel"
            >
              Cancel
            </Button>
            <Button
              onClick={handleConfirm}
              disabled={!canSubmit}
              data-testid="quick-train-start"
            >
              {isLoading ? (
                <>
                  <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                  Starting...
                </>
              ) : (
                <>
                  <Play className="h-4 w-4 mr-2" />
                  Start Training
                </>
              )}
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

export default QuickTrainConfirmModal;
