import React, { useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Label } from '@/components/ui/label';
import { Badge } from '@/components/ui/badge';
import { Skeleton } from '@/components/ui/skeleton';
import { AlertTriangle, CheckCircle, Clock } from 'lucide-react';
import { apiClient } from '@/api/services';
import type { DatasetVersionSummary, TrustState } from '@/api/training-types';
import { formatTimestamp } from '@/lib/formatters';

interface DatasetVersionPickerProps {
  datasetId: string;
  selectedVersionId?: string;
  onVersionSelect: (versionId: string) => void;
  disabled?: boolean;
}

function getTrustStateIcon(state?: TrustState) {
  switch (state) {
    case 'allowed':
      return <CheckCircle className="h-3 w-3 text-green-500" />;
    case 'allowed_with_warning':
      return <AlertTriangle className="h-3 w-3 text-yellow-500" />;
    case 'blocked':
      return <AlertTriangle className="h-3 w-3 text-red-500" />;
    case 'needs_approval':
      return <Clock className="h-3 w-3 text-orange-500" />;
    default:
      return null;
  }
}

function getTrustStateBadge(state?: TrustState) {
  switch (state) {
    case 'allowed':
      return <Badge variant="outline" className="text-xs">Trusted</Badge>;
    case 'allowed_with_warning':
      return <Badge variant="secondary" className="text-xs">Warning</Badge>;
    case 'blocked':
      return <Badge variant="destructive" className="text-xs">Blocked</Badge>;
    case 'needs_approval':
      return <Badge variant="outline" className="text-xs">Pending</Badge>;
    default:
      return <Badge variant="outline" className="text-xs">Unknown</Badge>;
  }
}

export function DatasetVersionPicker({
  datasetId,
  selectedVersionId,
  onVersionSelect,
  disabled = false,
}: DatasetVersionPickerProps) {
  const {
    data: versionsData,
    isLoading,
    error,
  } = useQuery({
    queryKey: ['dataset-versions', datasetId],
    queryFn: () => apiClient.listDatasetVersions(datasetId),
    enabled: !!datasetId,
    staleTime: 30000,
  });

  const versions = useMemo(() => versionsData?.versions ?? [], [versionsData?.versions]);

  // Auto-select first version if none selected and versions available
  React.useEffect(() => {
    if (!selectedVersionId && versions.length > 0) {
      onVersionSelect(versions[0].dataset_version_id);
    }
  }, [selectedVersionId, versions, onVersionSelect]);

  if (!datasetId) {
    return null;
  }

  if (isLoading) {
    return (
      <div className="space-y-2">
        <Label>Dataset Version</Label>
        <Skeleton className="h-10 w-full" />
      </div>
    );
  }

  if (error) {
    return (
      <div className="space-y-2">
        <Label>Dataset Version</Label>
        <div className="text-sm text-destructive">
          Failed to load versions
        </div>
      </div>
    );
  }

  if (versions.length === 0) {
    return (
      <div className="space-y-2">
        <Label>Dataset Version</Label>
        <div className="text-sm text-muted-foreground">
          No versions available for this dataset
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-2">
      <Label htmlFor="dataset-version">Dataset Version</Label>
      <Select
        value={selectedVersionId}
        onValueChange={onVersionSelect}
        disabled={disabled || versions.length <= 1}
      >
        <SelectTrigger id="dataset-version">
          <SelectValue placeholder="Select version" />
        </SelectTrigger>
        <SelectContent>
          {versions.map((version) => (
            <SelectItem
              key={version.dataset_version_id}
              value={version.dataset_version_id}
            >
              <div className="flex items-center gap-2">
                {getTrustStateIcon(version.trust_state)}
                <span>
                  v{version.version_number}
                  {version.version_label && ` - ${version.version_label}`}
                </span>
                <span className="text-xs text-muted-foreground">
                  ({formatTimestamp(version.created_at, 'short')})
                </span>
              </div>
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      {selectedVersionId && (
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          {(() => {
            const selected = versions.find(v => v.dataset_version_id === selectedVersionId);
            if (!selected) return null;
            return (
              <>
                {getTrustStateBadge(selected.trust_state)}
                {selected.hash_b3 && (
                  <span className="font-mono">
                    {selected.hash_b3.substring(0, 12)}...
                  </span>
                )}
              </>
            );
          })()}
        </div>
      )}
    </div>
  );
}
