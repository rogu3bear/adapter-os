import React, { useState, useMemo } from 'react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../ui/dialog';
import { Button } from '../ui/button';
import { Switch } from '../ui/switch';
import { Label } from '../ui/label';
import { Settings, RefreshCw, Loader2 } from 'lucide-react';
import type { DashboardWidgetConfig } from '../../api/types';

// Widget metadata for display
const WIDGET_METADATA: Record<string, { label: string; description: string }> = {
  'service-status': {
    label: 'Service Status',
    description: 'Monitor essential services and their health'
  },
  'multi-model-status': {
    label: 'Model Status',
    description: 'View loaded models and their status'
  },
  'system-health': {
    label: 'System Health',
    description: 'Overall system health metrics'
  },
  'active-alerts': {
    label: 'Active Alerts',
    description: 'Current system alerts and warnings'
  },
  'compliance-score': {
    label: 'Compliance Score',
    description: 'Policy compliance and violations'
  },
  'reporting-summary': {
    label: 'Reporting Summary',
    description: 'Summary of reports and audits'
  },
  'base-model': {
    label: 'Base Model',
    description: 'Base model information and status'
  },
  'ml-pipeline': {
    label: 'ML Pipeline',
    description: 'Training pipeline progress'
  },
  'adapter-status': {
    label: 'Adapter Status',
    description: 'Lifecycle state and memory usage'
  },
  'next-steps': {
    label: 'Next Steps',
    description: 'Recommended actions'
  },
  'activity-feed': {
    label: 'Activity Feed',
    description: 'Recent activity and events'
  },
};

interface DashboardSettingsProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  availableWidgetIds: string[]; // Widget IDs available for the current role
  currentConfig: DashboardWidgetConfig[];
  onUpdateVisibility: (widgetId: string, enabled: boolean) => Promise<void>;
  onReset: () => Promise<void>;
  isUpdating?: boolean;
}

export function DashboardSettings({
  open,
  onOpenChange,
  availableWidgetIds,
  currentConfig,
  onUpdateVisibility,
  onReset,
  isUpdating = false,
}: DashboardSettingsProps) {
  const [localUpdates, setLocalUpdates] = useState<Record<string, boolean>>({});
  const [isResetting, setIsResetting] = useState(false);

  // Merge current config with local updates
  const widgetStates = useMemo(() => {
    const states: Record<string, boolean> = {};

    // Default all available widgets to enabled
    availableWidgetIds.forEach(id => {
      states[id] = true;
    });

    // Apply current config from backend
    currentConfig.forEach(config => {
      states[config.widget_id] = config.enabled;
    });

    // Apply local updates (not yet saved)
    Object.entries(localUpdates).forEach(([id, enabled]) => {
      states[id] = enabled;
    });

    return states;
  }, [availableWidgetIds, currentConfig, localUpdates]);

  const handleToggle = async (widgetId: string, enabled: boolean) => {
    // Update local state immediately for responsive UI
    setLocalUpdates(prev => ({ ...prev, [widgetId]: enabled }));

    // Update backend
    try {
      await onUpdateVisibility(widgetId, enabled);
      // Clear local update once backend confirms
      setLocalUpdates(prev => {
        const updated = { ...prev };
        delete updated[widgetId];
        return updated;
      });
    } catch (err) {
      // On error, revert local update
      setLocalUpdates(prev => {
        const updated = { ...prev };
        delete updated[widgetId];
        return updated;
      });
    }
  };

  const handleReset = async () => {
    setIsResetting(true);
    try {
      await onReset();
      setLocalUpdates({});
      onOpenChange(false);
    } catch (err) {
      // Error handling is done by parent
    } finally {
      setIsResetting(false);
    }
  };

  const hasChanges = Object.keys(localUpdates).length > 0;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[500px]">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Settings className="h-5 w-5" />
            Customize Dashboard
          </DialogTitle>
          <DialogDescription>
            Show or hide widgets on your dashboard. Changes are saved automatically.
          </DialogDescription>
        </DialogHeader>

        <div className="max-h-[400px] overflow-y-auto py-4">
          <div className="space-y-4">
            {availableWidgetIds.map((widgetId) => {
              const metadata = WIDGET_METADATA[widgetId] || {
                label: widgetId,
                description: 'Widget',
              };
              const isEnabled = widgetStates[widgetId] ?? true;

              return (
                <div
                  key={widgetId}
                  className="flex items-start justify-between space-x-4 p-3 rounded-lg border"
                >
                  <div className="flex-1">
                    <Label
                      htmlFor={`widget-${widgetId}`}
                      className="text-sm font-medium"
                    >
                      {metadata.label}
                    </Label>
                    <p className="text-xs text-muted-foreground mt-0.5">
                      {metadata.description}
                    </p>
                  </div>
                  <Switch
                    id={`widget-${widgetId}`}
                    checked={isEnabled}
                    onCheckedChange={(checked) => handleToggle(widgetId, checked)}
                    disabled={isUpdating}
                  />
                </div>
              );
            })}
          </div>
        </div>

        <DialogFooter className="flex items-center justify-between">
          <Button
            variant="outline"
            size="sm"
            onClick={handleReset}
            disabled={isUpdating || isResetting}
          >
            {isResetting ? (
              <>
                <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                Resetting...
              </>
            ) : (
              <>
                <RefreshCw className="h-4 w-4 mr-2" />
                Reset to Defaults
              </>
            )}
          </Button>
          <div className="flex gap-2">
            {hasChanges && (
              <span className="text-xs text-muted-foreground self-center">
                Saving...
              </span>
            )}
            <Button variant="default" onClick={() => onOpenChange(false)}>
              Done
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
