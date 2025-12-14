// Behavior Export Wizard
//
// Multi-step dialog for exporting behavior training data with synthetic data generation.

import React, { useState } from 'react';
import { Download, Loader2 } from 'lucide-react';
import { useExportBehaviorData } from '@/hooks/training';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Checkbox } from '@/components/ui/checkbox';
import { Label } from '@/components/ui/label';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import type { BehaviorExportRequest } from '@/api/adapter-types';
import { logger, toError } from '@/utils/logger';

interface BehaviorExportWizardProps {
  isOpen: boolean;
  onClose: () => void;
  defaultTenantId?: string;
}

const CATEGORY_OPTIONS = [
  { id: 'promotion', label: 'Promotion', description: 'cold→warm→hot transitions' },
  { id: 'demotion', label: 'Demotion', description: 'hot→warm→cold transitions' },
  { id: 'eviction', label: 'Eviction', description: 'Memory pressure evictions' },
  { id: 'pinning', label: 'Pinning', description: 'Pin/unpin operations' },
  { id: 'recovery', label: 'Recovery', description: 'Heartbeat recovery' },
  { id: 'ttl_enforcement', label: 'TTL Enforcement', description: 'Expiration handling' },
];

export function BehaviorExportWizard({ isOpen, onClose, defaultTenantId }: BehaviorExportWizardProps) {
  const [step, setStep] = useState(1);
  const [config, setConfig] = useState<BehaviorExportRequest>({
    categories: [],
    tenant_id: defaultTenantId,
  });

  const exportMutation = useExportBehaviorData();

  const handleCategoryToggle = (categoryId: string, checked: boolean) => {
    setConfig((prev) => ({
      ...prev,
      categories: checked
        ? [...(prev.categories || []), categoryId]
        : (prev.categories || []).filter((c) => c !== categoryId),
    }));
  };

  const handleExport = async () => {
    try {
      await exportMutation.mutateAsync(config);
      onClose();
    } catch (error) {
      logger.error('Behavior export failed during execution', {
        component: 'BehaviorExportWizard',
        operation: 'handleExport',
        errorType: 'export_execution_failure',
        details: 'Failed to execute behavior export with provided configuration',
        exportConfig: config
      }, toError(error));
    }
  };

  const renderStep = () => {
    switch (step) {
      case 1:
        return (
          <div className="space-y-4">
            <CardHeader>
              <CardTitle>Step 1: Select Categories</CardTitle>
              <CardDescription>
                Choose which types of behavior events to include in the export
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              {CATEGORY_OPTIONS.map((category) => (
                <div key={category.id} className="flex items-start space-x-3 p-3 border rounded">
                  <Checkbox
                    id={category.id}
                    checked={(config.categories || []).includes(category.id)}
                    onCheckedChange={(checked) =>
                      handleCategoryToggle(category.id, checked as boolean)
                    }
                  />
                  <div className="flex-1">
                    <Label htmlFor={category.id} className="font-medium">
                      {category.label}
                    </Label>
                    <p className="text-sm text-muted-foreground">{category.description}</p>
                  </div>
                </div>
              ))}
              <div className="pt-2">
                <Button
                  onClick={() => setConfig((prev) => ({ ...prev, categories: CATEGORY_OPTIONS.map(c => c.id) }))}
                  variant="outline"
                  size="sm"
                >
                  Select All
                </Button>
              </div>
            </CardContent>
          </div>
        );

      case 2:
        return (
          <div className="space-y-4">
            <CardHeader>
              <CardTitle>Step 2: Date Range (Optional)</CardTitle>
              <CardDescription>
                Filter events by date range. Leave empty for all events.
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="space-y-2">
                <Label htmlFor="since">From Date</Label>
                <Input
                  id="since"
                  type="date"
                  value={config.since || ''}
                  onChange={(e) => setConfig((prev) => ({ ...prev, since: e.target.value }))}
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="until">To Date</Label>
                <Input
                  id="until"
                  type="date"
                  value={config.until || ''}
                  onChange={(e) => setConfig((prev) => ({ ...prev, until: e.target.value }))}
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="adapter_id">Adapter ID (Optional)</Label>
                <Input
                  id="adapter_id"
                  placeholder="Filter by specific adapter..."
                  value={config.adapter_id || ''}
                  onChange={(e) => setConfig((prev) => ({ ...prev, adapter_id: e.target.value }))}
                />
              </div>
            </CardContent>
          </div>
        );

      case 3:
        return (
          <div className="space-y-4">
            <CardHeader>
              <CardTitle>Step 3: Synthetic Data (Optional)</CardTitle>
              <CardDescription>
                Generate synthetic examples to supplement real events
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="space-y-2">
                <Label htmlFor="synthetic_count">Number of Synthetic Examples</Label>
                <Input
                  id="synthetic_count"
                  type="number"
                  min="0"
                  placeholder="0"
                  value={config.synthetic_count || ''}
                  onChange={(e) =>
                    setConfig((prev) => ({
                      ...prev,
                      synthetic_count: e.target.value ? parseInt(e.target.value) : undefined,
                    }))
                  }
                />
                <p className="text-xs text-muted-foreground">
                  Synthetic examples are generated using HKDF-seeded deterministic randomness
                </p>
              </div>
              <div className="space-y-2">
                <Label htmlFor="min_per_category">Minimum Examples Per Category</Label>
                <Input
                  id="min_per_category"
                  type="number"
                  min="0"
                  placeholder="0"
                  value={config.min_per_category || ''}
                  onChange={(e) =>
                    setConfig((prev) => ({
                      ...prev,
                      min_per_category: e.target.value ? parseInt(e.target.value) : undefined,
                    }))
                  }
                />
              </div>
            </CardContent>
          </div>
        );

      case 4:
        return (
          <div className="space-y-4">
            <CardHeader>
              <CardTitle>Step 4: Review & Export</CardTitle>
              <CardDescription>Review your export configuration</CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="space-y-2">
                <div>
                  <span className="font-semibold">Categories:</span>{' '}
                  {config.categories && config.categories.length > 0
                    ? config.categories.join(', ')
                    : 'All'}
                </div>
                {config.since && (
                  <div>
                    <span className="font-semibold">From:</span> {config.since}
                  </div>
                )}
                {config.until && (
                  <div>
                    <span className="font-semibold">To:</span> {config.until}
                  </div>
                )}
                {config.adapter_id && (
                  <div>
                    <span className="font-semibold">Adapter ID:</span> {config.adapter_id}
                  </div>
                )}
                {config.synthetic_count && config.synthetic_count > 0 && (
                  <div>
                    <span className="font-semibold">Synthetic Examples:</span> {config.synthetic_count}
                  </div>
                )}
                {config.min_per_category && config.min_per_category > 0 && (
                  <div>
                    <span className="font-semibold">Min Per Category:</span> {config.min_per_category}
                  </div>
                )}
              </div>
              <div className="pt-4">
                <Button
                  onClick={handleExport}
                  disabled={exportMutation.isPending}
                  className="w-full"
                >
                  {exportMutation.isPending ? (
                    <>
                      <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                      Generating Export...
                    </>
                  ) : (
                    <>
                      <Download className="mr-2 h-4 w-4" />
                      Export to JSONL
                    </>
                  )}
                </Button>
              </div>
            </CardContent>
          </div>
        );

      default:
        return null;
    }
  };

  return (
    <Dialog open={isOpen} onOpenChange={onClose}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>Export Behavior Training Data</DialogTitle>
          <DialogDescription>
            Generate a JSONL dataset from adapter lifecycle events
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          {/* Progress indicator */}
          <div className="flex items-center justify-between">
            {[1, 2, 3, 4].map((s) => (
              <div
                key={s}
                className={`flex items-center ${s === step ? 'text-primary' : 'text-muted-foreground'}`}
              >
                <div
                  className={`w-8 h-8 rounded-full flex items-center justify-center border-2 ${
                    s === step ? 'border-primary bg-primary text-primary-foreground' : 'border-muted'
                  }`}
                >
                  {s}
                </div>
                {s < 4 && <div className="w-12 h-0.5 bg-muted mx-2" />}
              </div>
            ))}
          </div>

          {/* Step content */}
          <Card>{renderStep()}</Card>

          {/* Navigation */}
          <div className="flex justify-between">
            <Button variant="outline" onClick={() => setStep((s) => Math.max(1, s - 1))} disabled={step === 1}>
              Previous
            </Button>
            {step < 4 && (
              <Button onClick={() => setStep((s) => Math.min(4, s + 1))}>
                Next
              </Button>
            )}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

