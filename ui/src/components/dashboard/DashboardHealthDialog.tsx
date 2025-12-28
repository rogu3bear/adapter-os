/**
 * Dashboard Health Dialog Component
 *
 * Modal dialog displaying detailed system health information.
 */

import React, { memo } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Progress } from '@/components/ui/progress';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog';
import { FormGrid } from '@/components/ui/grid';

/**
 * Props for the DashboardHealthDialog component
 */
export interface DashboardHealthDialogProps {
  /** Whether the dialog is open */
  open: boolean;
  /** Callback to close the dialog */
  onClose: () => void;
  /** CPU usage percentage */
  cpuUsage: number;
  /** Memory usage percentage */
  memoryUsage: number;
  /** Number of active nodes */
  nodeCount: number;
  /** Number of active adapters */
  adapterCount: number;
  /** Tokens processed per second */
  tokensPerSecond: number;
  /** 95th percentile latency in milliseconds */
  latencyP95: number;
}

/**
 * System health details dialog.
 *
 * Shows detailed CPU, memory, and system metrics in a modal.
 */
export const DashboardHealthDialog = memo(function DashboardHealthDialog({
  open,
  onClose,
  cpuUsage,
  memoryUsage,
  nodeCount,
  adapterCount,
  tokensPerSecond,
  latencyP95,
}: DashboardHealthDialogProps) {
  return (
    <Dialog open={open} onOpenChange={(isOpen) => !isOpen && onClose()}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>System Health Details</DialogTitle>
        </DialogHeader>
        <div className="space-y-4">
          <FormGrid>
            {/* CPU Usage Card */}
            <Card>
              <CardHeader className="pb-2">
                <CardTitle className="text-sm">CPU Usage</CardTitle>
              </CardHeader>
              <CardContent>
                <div className="text-2xl font-bold">{cpuUsage.toFixed(0)}%</div>
                <Progress value={cpuUsage} className="mt-2" />
              </CardContent>
            </Card>

            {/* Memory Usage Card */}
            <Card>
              <CardHeader className="pb-2">
                <CardTitle className="text-sm">Memory Usage</CardTitle>
              </CardHeader>
              <CardContent>
                <div className="text-2xl font-bold">{memoryUsage.toFixed(0)}%</div>
                <Progress value={memoryUsage} className="mt-2" />
              </CardContent>
            </Card>
          </FormGrid>

          {/* System Metrics */}
          <div className="space-y-2">
            <div className="flex justify-between text-sm">
              <span>Active Nodes:</span>
              <span className="font-medium">{nodeCount}</span>
            </div>
            <div className="flex justify-between text-sm">
              <span>Active Adapters:</span>
              <span className="font-medium">{adapterCount}</span>
            </div>
            <div className="flex justify-between text-sm">
              <span>Tokens/Second:</span>
              <span className="font-medium">{tokensPerSecond.toFixed(0)}</span>
            </div>
            <div className="flex justify-between text-sm">
              <span>Latency (p95):</span>
              <span className="font-medium">{latencyP95.toFixed(0)}ms</span>
            </div>
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            Close
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
});
