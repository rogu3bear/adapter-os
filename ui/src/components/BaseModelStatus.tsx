
// 【ui/src/components/BaseModelStatus.tsx§46-52】 - Replace manual polling with standardized hook
import React, { useState, useEffect } from 'react';
import {
  CheckCircle,
  XCircle,
  Clock,
  AlertTriangle,
  Cpu,
  HardDrive,
  RefreshCw,
  Info
} from 'lucide-react';
import { BaseModelStatus } from '@/api/types';
import apiClient from '@/api/client';

import { toast } from 'sonner';
import { logger, toError } from '@/utils/logger';
import { usePolling } from '@/hooks/realtime/usePolling';
import { LastUpdated } from './ui/last-updated';
import { ErrorRecovery } from './ui/error-recovery';
import { GlossaryTooltip } from './ui/glossary-tooltip';
import { formatTimestamp as formatTimestampUtil, formatRelativeTime } from '@/utils/format';

interface BaseModelStatusProps {
  selectedTenant: string;
}

export function BaseModelStatusComponent({ selectedTenant }: BaseModelStatusProps) {
  const fetchModelStatus = async () => {
    const modelStatus = await apiClient.getBaseModelStatus(selectedTenant);
    return modelStatus;
  };

  const {
    data: status,
    isLoading: loading,
    lastUpdated,
    error,
    refetch: refreshStatus
  } = usePolling(
    fetchModelStatus,
    'normal',
    {
      showLoadingIndicator: true,
      onError: (err) => {
        logger.error('Failed to fetch base model status', {
          component: 'BaseModelStatus',
          operation: 'polling',
          tenantId: selectedTenant,
        }, err);
      }
    }
  );

  const getStatusIcon = (status: string) => {
    switch (status) {
      case 'ready':
      case 'loaded': // legacy
        return <CheckCircle className="h-5 w-5 text-green-500" />;
      case 'loading':
        return <RefreshCw className="h-5 w-5 text-blue-500 animate-spin" />;
      case 'unloading':
        return <RefreshCw className="h-5 w-5 text-orange-500 animate-spin" />;
      case 'no-model':
      case 'unloaded': // legacy
        return <XCircle className="h-5 w-5 text-gray-500" />;
      case 'error':
        return <AlertTriangle className="h-5 w-5 text-red-500" />;
      default:
        return <Info className="h-5 w-5 text-gray-500" />;
    }
  };

  const getStatusColor = (status: string) => {
    switch (status) {
      case 'ready':
      case 'loaded': // legacy
        return 'bg-green-50 border-green-200 text-green-800';
      case 'loading':
        return 'bg-blue-50 border-blue-200 text-blue-800';
      case 'unloading':
        return 'bg-orange-50 border-orange-200 text-orange-800';
      case 'no-model':
      case 'unloaded': // legacy
        return 'bg-gray-50 border-gray-200 text-gray-800';
      case 'error':
        return 'bg-red-50 border-red-200 text-red-800';
      default:
        return 'bg-gray-50 border-gray-200 text-gray-800';
    }
  };

  const formatMemoryUsage = (mb?: number) => {
    if (!mb) return 'N/A';
    if (mb >= 1024) {
      return `${(mb / 1024).toFixed(1)} GB`;
    }
    return `${mb} MB`;
  };

  const formatTimestamp = (timestamp?: string) => {
    if (!timestamp) return 'N/A';
    return formatTimestampUtil(timestamp, 'long');
  };

  const getTimeSinceLoaded = (loadedAt?: string) => {
    if (!loadedAt) return null;
    const loaded = new Date(loadedAt);
    const now = new Date();
    const diffMs = now.getTime() - loaded.getTime();
    const diffMins = Math.floor(diffMs / 60000);
    const diffHours = Math.floor(diffMins / 60);
    const diffDays = Math.floor(diffHours / 24);

    if (diffDays > 0) return `${diffDays}d ${diffHours % 24}h`;
    if (diffHours > 0) return `${diffHours}h ${diffMins % 60}m`;
    return `${diffMins}m`;
  };

  if (loading) {
    return (
      <div className="bg-white rounded-lg border border-gray-200 p-6">
        <div className="flex items-center space-x-3">
          <RefreshCw className="h-5 w-5 text-gray-400 animate-spin" />
          <div>
            <h3 className="text-lg font-medium text-gray-900">Base Model Status</h3>
            <p className="text-sm text-gray-500">Loading...</p>
          </div>
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <ErrorRecovery
        error={error.message}
        onRetry={() => refreshStatus()}
      />
    );
  }

  if (!status) {
    return (
      <div className="bg-white rounded-lg border border-gray-200 p-6">
        <div className="flex items-center space-x-3">
          <Info className="h-5 w-5 text-gray-400" />
          <div>
            <h3 className="text-lg font-medium text-gray-900">Base Model Status</h3>
            <p className="text-sm text-gray-500">No model status available</p>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="bg-white rounded-lg border border-gray-200 p-6">
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center space-x-3">
          {getStatusIcon(status.status)}
          <div>
            <h3 className="text-lg font-medium text-gray-900">
              Base Model Status
              <GlossaryTooltip termId="base-model-status" />
            </h3>
            <p className="text-sm text-gray-500">
              {status.model_name} ({status.model_id})
              <GlossaryTooltip termId="base-model-name" />
              {status.model_path && (
                <span className="ml-2 text-xs text-muted-foreground" title={status.model_path}>
                  📁 {status.model_path.split('/').pop()}
                </span>
              )}
            </p>

            {lastUpdated && <LastUpdated timestamp={lastUpdated} className="mt-1" />}

          </div>
        </div>
      </div>

      <div className="space-y-4">
        {/* Status Badge */}
        <div className="flex items-center space-x-2">
          <span className="text-sm font-medium text-gray-700">Status:</span>
          <span className={`inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium border ${getStatusColor(status.status)}`}>
            {status.status.charAt(0).toUpperCase() + status.status.slice(1)}
          </span>
        </div>

        {/* Memory Usage */}
        <div className="flex items-center space-x-2">
          <HardDrive className="h-4 w-4 text-gray-400" />
          <span className="text-sm font-medium text-gray-700">
            Memory:
            <GlossaryTooltip termId="base-model-memory" />
          </span>
          <span className="text-sm text-gray-600">{formatMemoryUsage(status.memory_usage_mb)}</span>
        </div>

        {/* Loaded Time */}
        {status.loaded_at && (
          <div className="flex items-center space-x-2">
            <Clock className="h-4 w-4 text-gray-400" />
            <span className="text-sm font-medium text-gray-700">Loaded:</span>
            <span className="text-sm text-gray-600">
              {formatTimestamp(status.loaded_at)}
              {getTimeSinceLoaded(status.loaded_at) && (
                <span className="text-gray-500 ml-1">
                  ({getTimeSinceLoaded(status.loaded_at)} ago)
                </span>
              )}
            </span>
          </div>
        )}

        {/* Error Message */}
        {status.error_message && (
          <div className="flex items-start space-x-2">
            <AlertTriangle className="h-4 w-4 text-red-400 mt-0.5" />
            <div>
              <span className="text-sm font-medium text-red-700">Error:</span>
              <p className="text-sm text-red-600 mt-1">{status.error_message}</p>
            </div>
          </div>
        )}

        {/* Last Updated */}
        {lastUpdated && (
          <div className="flex items-center space-x-2 pt-2 border-t border-gray-100">
            <Info className="h-4 w-4 text-gray-400" />
            <span className="text-xs text-gray-500">
              Last updated: {lastUpdated.toLocaleTimeString()}
            </span>
          </div>
        )}
      </div>
    </div>
  );
}
