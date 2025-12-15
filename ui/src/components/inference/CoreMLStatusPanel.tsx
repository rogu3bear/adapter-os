import React from 'react';
import { Label } from '@/components/ui/label';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { AlertTriangle, Download, HelpCircle, History, Loader2 } from 'lucide-react';
import { UseCoreMLManagementReturn } from '@/hooks/inference/useCoreMLManagement';

export interface CoreMLStatusPanelProps {
  /** CoreML management hook return */
  coreml: UseCoreMLManagementReturn;
  /** Selected adapter ID */
  adapterId: string;
  /** Whether CoreML backend is available */
  coremlBackendAvailable: boolean;
  /** Whether the panel is disabled */
  disabled?: boolean;
}

/**
 * CoreML status panel with export and verification controls.
 */
export function CoreMLStatusPanel({
  coreml,
  adapterId,
  coremlBackendAvailable,
  disabled = false,
}: CoreMLStatusPanelProps) {
  const {
    isLoading,
    actionInProgress,
    isUiEnabled,
    isSupported,
    actionsAvailable,
    exportStatus,
    verificationStatus,
    hasMismatch,
    expectedHash,
    actualHash,
    triggerExport,
    triggerVerification,
  } = coreml;

  const hasAdapter = adapterId && adapterId !== 'none';
  const showUnavailableBadge = isUiEnabled && !coremlBackendAvailable;

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <Label className="flex items-center gap-1">
          CoreML package
          <GlossaryTooltip termId="coreml">
            <span className="cursor-help text-muted-foreground hover:text-foreground">
              <HelpCircle className="h-3 w-3" />
            </span>
          </GlossaryTooltip>
        </Label>
        {isLoading && (
          <Loader2
            className="h-3 w-3 animate-spin text-muted-foreground"
            aria-label="Loading CoreML status"
          />
        )}
      </div>

      {!isUiEnabled ? (
        <p className="text-[11px] text-muted-foreground" data-cy="coreml-disabled-note">
          CoreML export and verification are not yet supported in this build.
        </p>
      ) : (
        <>
          {/* Status Badges */}
          <div
            className="flex flex-wrap items-center gap-2"
            data-cy="coreml-status-panel"
            role="status"
            aria-label="CoreML package status"
          >
            <Badge
              variant={exportStatus.variant}
              className="text-[11px]"
              data-cy="coreml-export-badge"
              aria-label={`Export status: ${exportStatus.label}`}
            >
              Export: {exportStatus.label}
            </Badge>
            <Badge
              variant={verificationStatus.variant}
              className="text-[11px]"
              data-cy="coreml-verification-badge"
              aria-label={`Verification status: ${verificationStatus.label}`}
            >
              Verification: {verificationStatus.label}
            </Badge>
            {hasMismatch && (
              <Badge variant="destructive" className="text-[11px]" data-cy="coreml-mismatch-badge">
                Verification mismatch
              </Badge>
            )}
            {showUnavailableBadge && (
              <Badge variant="secondary" className="text-[11px]" data-cy="coreml-unavailable-badge">
                CoreML fallback · unavailable
              </Badge>
            )}
            {!isSupported && (
              <Badge variant="outline" className="text-[11px]" data-cy="coreml-unsupported-badge">
                CoreML actions unsupported by server
              </Badge>
            )}
          </div>

          {/* Mismatch Alert */}
          {hasMismatch && (
            <Alert variant="destructive" data-cy="coreml-mismatch-alert">
              <AlertTriangle className="h-4 w-4" />
              <AlertDescription>
                Verification reported a CoreML package hash mismatch. Re-run verification after
                refreshing the package or check registry integrity.
              </AlertDescription>
            </Alert>
          )}

          {/* Hash Information */}
          {(expectedHash || actualHash) && (
            <p className="text-[11px] text-muted-foreground" data-cy="coreml-hash-info">
              {expectedHash ? `Expected: ${expectedHash}` : 'Expected hash unavailable'}
              {actualHash ? ` · Actual: ${actualHash}` : ''}
            </p>
          )}

          {/* Help Text */}
          <p className="text-[11px] text-muted-foreground">
            {!hasAdapter
              ? 'Select an adapter to view CoreML export and verification status.'
              : coremlBackendAvailable
                ? 'CoreML is preferred. If blocked by policy or hardware, the UI will show the fallback backend.'
                : 'CoreML is unavailable; inference will fall back automatically.'}
          </p>

          {/* Action Buttons */}
          <div className="flex flex-wrap gap-2">
            <Button
              size="sm"
              variant="outline"
              data-cy="coreml-export-trigger"
              onClick={triggerExport}
              disabled={!actionsAvailable || disabled}
              className="h-8"
            >
              {actionInProgress === 'export' ? (
                <Loader2 className="h-4 w-4 animate-spin mr-2" />
              ) : (
                <Download className="h-4 w-4 mr-2" />
              )}
              Request CoreML export
            </Button>
            <Button
              size="sm"
              variant="outline"
              data-cy="coreml-verify-trigger"
              onClick={triggerVerification}
              disabled={!actionsAvailable || disabled}
              className="h-8"
            >
              {actionInProgress === 'verify' ? (
                <Loader2 className="h-4 w-4 animate-spin mr-2" />
              ) : (
                <History className="h-4 w-4 mr-2" />
              )}
              Re-run verification
            </Button>
          </div>
        </>
      )}
    </div>
  );
}
