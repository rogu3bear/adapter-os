import React from 'react';
import { Label } from '@/components/ui/label';
import { Badge } from '@/components/ui/badge';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { AlertTriangle, HelpCircle, Loader2 } from 'lucide-react';
import { BackendName, HardwareCapabilities } from '@/api/types';
import { BackendOption } from './types';
import { BACKEND_LABELS } from './constants';

export interface BackendSelectorProps {
  /** Available backend options */
  backendOptions: BackendOption[];
  /** Currently selected backend */
  selectedBackend: BackendName;
  /** Last backend actually used */
  lastBackendUsed: string | null;
  /** Hardware capabilities */
  hardwareCapabilities: HardwareCapabilities | null;
  /** Loading state */
  isLoading: boolean;
  /** Error message */
  error: string | null;
  /** Warning message (fallback notification) */
  warning: string | null;
  /** Callback when backend is selected */
  onSelect: (backend: BackendName) => void;
  /** Whether selector is disabled */
  disabled?: boolean;
}

/**
 * Backend selector component with availability indicators and fallback warnings.
 */
export function BackendSelector({
  backendOptions,
  selectedBackend,
  lastBackendUsed,
  hardwareCapabilities,
  isLoading,
  error,
  warning,
  onSelect,
  disabled = false,
}: BackendSelectorProps) {
  const activeBackend = (lastBackendUsed || selectedBackend || 'auto') as BackendName;
  const activeBackendOption = backendOptions.find((opt) => opt.name === activeBackend);
  const activeBackendLabel = `${BACKEND_LABELS[activeBackend] || activeBackend}${activeBackendOption?.hardwareHint ? ` (${activeBackendOption.hardwareHint})` : ''}${activeBackendOption?.status ? ` · ${activeBackendOption.status}` : ''}`;

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <Label className="flex items-center gap-1">
          Backend
          <GlossaryTooltip termId="inference-backend">
            <span className="cursor-help text-muted-foreground hover:text-foreground">
              <HelpCircle className="h-3 w-3" />
            </span>
          </GlossaryTooltip>
        </Label>
        <div className="flex items-center gap-2">
          {isLoading && (
            <Loader2
              className="h-4 w-4 animate-spin text-muted-foreground"
              aria-label="Loading backend status"
              data-testid="loading-state"
            />
          )}
          <Badge variant="secondary" className="text-xs gap-1" data-cy="active-backend-tag">
            {activeBackendLabel || 'Auto (router)'}
          </Badge>
        </div>
      </div>

      <Select
        value={selectedBackend || 'auto'}
        onValueChange={(value) => onSelect(value as BackendName)}
        disabled={isLoading || disabled}
      >
        <SelectTrigger data-cy="backend-selector">
          <SelectValue placeholder="Select backend" />
        </SelectTrigger>
        <SelectContent>
          {backendOptions.map((option) => (
            <SelectItem
              key={option.name}
              value={option.name}
              data-cy={`backend-option-${option.name}`}
              disabled={!option.available && option.name !== 'auto'}
            >
              <div className="flex items-center gap-2">
                <span>{BACKEND_LABELS[option.name] || option.name}</span>
                <Badge
                  variant={option.available ? 'default' : 'secondary'}
                  className="text-[10px]"
                >
                  {option.available ? 'available' : 'fallback to auto'}
                </Badge>
                {option.mode && (
                  <Badge variant="outline" className="text-[10px]">
                    {option.mode}
                  </Badge>
                )}
              </div>
              {option.hardwareHint && (
                <div className="text-[11px] text-muted-foreground ml-6">
                  {option.hardwareHint}
                </div>
              )}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>

      {warning && (
        <Alert variant="destructive" data-cy="backend-fallback-alert">
          <AlertTriangle className="h-4 w-4" />
          <AlertDescription>{warning}</AlertDescription>
        </Alert>
      )}

      {error && (
        <Alert variant="default">
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}

      {hardwareCapabilities && (
        <p className="text-[11px] text-muted-foreground">
          Hardware: {hardwareCapabilities.ane_available ? 'ANE' : 'No ANE'} ·{' '}
          {hardwareCapabilities.gpu_available
            ? hardwareCapabilities.gpu_type || 'GPU'
            : 'No GPU'}{' '}
          · {hardwareCapabilities.cpu_model || 'CPU'}
        </p>
      )}
    </div>
  );
}
