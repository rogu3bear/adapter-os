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
import { BaseModelStatus } from '../api/types';
import apiClient from '../api/client';
import { toast } from 'react-hot-toast';

interface BaseModelStatusProps {
  selectedTenant: string;
  refreshInterval?: number;
}

export function BaseModelStatusComponent({ selectedTenant, refreshInterval = 5000 }: BaseModelStatusProps) {
  const [status, setStatus] = useState<BaseModelStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null);

  const fetchStatus = async () => {
    try {
      setError(null);
      const modelStatus = await apiClient.getBaseModelStatus(selectedTenant);
      setStatus(modelStatus);
      setLastUpdated(new Date());
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to fetch model status';
      setError(errorMsg);
      console.error('Failed to fetch base model status:', err);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchStatus();
    
    if (refreshInterval > 0) {
      const interval = setInterval(fetchStatus, refreshInterval);
      return () => clearInterval(interval);
    }
  }, [selectedTenant, refreshInterval]);

  const getStatusIcon = (status: string) => {
    switch (status) {
      case 'loaded':
        return <CheckCircle className="h-5 w-5 text-green-500" />;
      case 'loading':
        return <RefreshCw className="h-5 w-5 text-blue-500 animate-spin" />;
      case 'unloading':
        return <RefreshCw className="h-5 w-5 text-orange-500 animate-spin" />;
      case 'unloaded':
        return <XCircle className="h-5 w-5 text-gray-500" />;
      case 'error':
        return <AlertTriangle className="h-5 w-5 text-red-500" />;
      default:
        return <Info className="h-5 w-5 text-gray-500" />;
    }
  };

  const getStatusColor = (status: string) => {
    switch (status) {
      case 'loaded':
        return 'bg-green-50 border-green-200 text-green-800';
      case 'loading':
        return 'bg-blue-50 border-blue-200 text-blue-800';
      case 'unloading':
        return 'bg-orange-50 border-orange-200 text-orange-800';
      case 'unloaded':
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
    return new Date(timestamp).toLocaleString();
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
      <div className="bg-white rounded-lg border border-red-200 p-6">
        <div className="flex items-center space-x-3">
          <AlertTriangle className="h-5 w-5 text-red-500" />
          <div>
            <h3 className="text-lg font-medium text-red-900">Base Model Status</h3>
            <p className="text-sm text-red-600">{error}</p>
            <button
              onClick={fetchStatus}
              className="mt-2 text-sm text-red-600 hover:text-red-800 underline"
            >
              Retry
            </button>
          </div>
        </div>
      </div>
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
            <h3 className="text-lg font-medium text-gray-900">Base Model Status</h3>
            <p className="text-sm text-gray-500">
              {status.model_name} ({status.model_id})
            </p>
          </div>
        </div>
        <button
          onClick={fetchStatus}
          className="p-2 text-gray-400 hover:text-gray-600 transition-colors"
          title="Refresh status"
        >
          <RefreshCw className="h-4 w-4" />
        </button>
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
          <span className="text-sm font-medium text-gray-700">Memory:</span>
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
