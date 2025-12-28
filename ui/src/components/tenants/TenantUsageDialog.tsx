import React from 'react';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { Progress } from '@/components/ui/progress';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { Tenant as ApiTenant, TenantUsageResponse } from '@/api/types';
import { formatPercent, formatCount } from '@/utils';

export interface TenantUsageDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  tenant: ApiTenant | null;
  usageData: TenantUsageResponse | null;
}

export function TenantUsageDialog({
  open,
  onOpenChange,
  tenant,
  usageData,
}: TenantUsageDialogProps) {
  const handleExportCsv = () => {
    if (!usageData || !tenant) return;

    const rows = [
      ['cpu_usage_pct', (usageData.cpu_usage_pct ?? 0).toFixed(1)],
      ['gpu_usage_pct', (usageData.gpu_usage_pct ?? 0).toFixed(1)],
      ['memory_used_gb', (usageData.memory_used_gb ?? 0).toFixed(2)],
      ['memory_total_gb', (usageData.memory_total_gb ?? 0).toFixed(2)],
      ['inference_count_24h', (usageData.inference_count_24h ?? 0).toString()],
      ['active_adapters_count', (usageData.active_adapters_count ?? 0).toString()],
    ];
    const csv = 'key,value\n' + rows.map((r) => r.join(',')).join('\n');
    const blob = new Blob([csv], { type: 'text/csv' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `workspace-usage-${tenant.id}.csv`;
    a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Workspace Usage - {tenant?.name}</DialogTitle>
        </DialogHeader>
        {usageData && (
          <div className="space-y-4">
            <div>
              <Label>CPU Usage</Label>
              <Progress value={usageData.cpu_usage_pct ?? 0} className="mt-2" />
              <p className="text-sm text-muted-foreground mt-1">
                {formatPercent(usageData.cpu_usage_pct)}
              </p>
            </div>
            <div>
              <Label>GPU Usage</Label>
              <Progress value={usageData.gpu_usage_pct ?? 0} className="mt-2" />
              <p className="text-sm text-muted-foreground mt-1">
                {formatPercent(usageData.gpu_usage_pct)}
              </p>
            </div>
            <div>
              <Label>Memory Usage</Label>
              <p className="text-sm">
                {(usageData.memory_used_gb ?? 0).toFixed(2)} GB /{' '}
                {(usageData.memory_total_gb ?? 0).toFixed(2)} GB
              </p>
              <Progress
                value={
                  usageData.memory_total_gb
                    ? ((usageData.memory_used_gb ?? 0) / usageData.memory_total_gb) * 100
                    : 0
                }
                className="mt-2"
              />
            </div>
            <div>
              <Label>Inference Count (24h)</Label>
              <p className="text-lg font-medium">
                {formatCount(usageData.inference_count_24h)}
              </p>
            </div>
            <div>
              <Label>Active Adapters</Label>
              <p>{formatCount(usageData.active_adapters_count)}</p>
            </div>
          </div>
        )}
        <DialogFooter>
          {usageData && (
            <GlossaryTooltip termId="export-usage-csv">
              <Button variant="outline" onClick={handleExportCsv}>
                Export CSV
              </Button>
            </GlossaryTooltip>
          )}
          <Button onClick={() => onOpenChange(false)}>Close</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
