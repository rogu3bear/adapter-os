import React from 'react';
import { useQuery } from '@tanstack/react-query';
import { Alert, AlertDescription, AlertTitle } from './ui/alert';
import { Button } from './ui/button';
import { AlertTriangle, CheckCircle2, XCircle, ExternalLink } from 'lucide-react';
import apiClient from '@/api/client';
import { formatDistanceToNow } from 'date-fns';
import type { DeterminismStatusResponse, AdapterQuarantineStatusResponse } from '@/api/types';

export function AdminBanner() {
  const { data: determinismStatus } = useQuery<DeterminismStatusResponse>({
    queryKey: ['determinism-status'],
    queryFn: () => apiClient.getDeterminismStatus(),
    refetchInterval: 30000, // Refresh every 30 seconds
  });

  const { data: quarantineStatus } = useQuery<AdapterQuarantineStatusResponse>({
    queryKey: ['quarantine-status'],
    queryFn: () => apiClient.getDiagnosticsQuarantineStatus(),
    refetchInterval: 30000, // Refresh every 30 seconds
  });

  const hasDeterminismIssue = determinismStatus?.result === 'fail';
  const hasQuarantineIssue = (quarantineStatus?.quarantined_count ?? 0) > 0;

  if (!hasDeterminismIssue && !hasQuarantineIssue) {
    return null; // Don't show banner if everything is OK
  }

  return (
    <div className="space-y-2 mb-4">
      {hasDeterminismIssue && (
        <Alert variant="destructive">
          <XCircle className="h-4 w-4" />
          <AlertTitle>Determinism Check Failed</AlertTitle>
          <AlertDescription className="flex items-center justify-between">
            <span>
              Last run: {determinismStatus?.last_run 
                ? formatDistanceToNow(new Date(determinismStatus.last_run), { addSuffix: true })
                : 'Never'}
              {determinismStatus?.divergences !== undefined && determinismStatus.divergences > 0 && (
                <span className="ml-2">
                  ({determinismStatus.divergences} divergence{determinismStatus.divergences !== 1 ? 's' : ''})
                </span>
              )}
            </span>
            <Button
              variant="outline"
              size="sm"
              onClick={() => {
                // Navigate to diagnostics page or run check
                window.location.href = '/admin?tab=diagnostics';
              }}
            >
              View Details
            </Button>
          </AlertDescription>
        </Alert>
      )}

      {hasQuarantineIssue && (
        <Alert variant={quarantineStatus?.in_active_stacks ? 'destructive' : 'default'}>
          <AlertTriangle className="h-4 w-4" />
          <AlertTitle>
            Quarantined Adapters Present: {quarantineStatus?.quarantined_count ?? 0}
          </AlertTitle>
          <AlertDescription className="flex items-center justify-between">
            <span>
              {quarantineStatus?.in_active_stacks && (
                <span className="font-semibold text-red-600 mr-2">
                  WARNING: Some quarantined adapters are in active stacks!
                </span>
              )}
              {quarantineStatus?.quarantined_count === 1 
                ? '1 adapter is quarantined'
                : `${quarantineStatus?.quarantined_count} adapters are quarantined`}
            </span>
            <Button
              variant="outline"
              size="sm"
              onClick={() => {
                window.location.href = '/admin?tab=quarantine';
              }}
            >
              View Details
              <ExternalLink className="ml-1 h-3 w-3" />
            </Button>
          </AlertDescription>
        </Alert>
      )}
    </div>
  );
}

