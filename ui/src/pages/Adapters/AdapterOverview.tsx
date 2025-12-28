// AdapterOverview - Overview tab displaying adapter metadata, status, and metrics
// Shows comprehensive adapter information at a glance

import React from 'react';
import {
  Activity,
  Box,
  Calendar,
  Clock,
  Cpu,
  Database,
  FileCode,
  GitBranch,
  Hash,
  Layers,
  MemoryStick,
  Target,
  TrendingUp,
  User,
  Zap,
} from 'lucide-react';

import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Skeleton } from '@/components/ui/skeleton';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { HealthBadge, TrustBadge } from '@/components/shared/TrustHealthBadge';
import { AdapterDetailResponse, AdapterHealthResponse } from '@/api/adapter-types';
import type { TrustState } from '@/api/training-types';
import { getLifecycleVariant } from '@/utils/lifecycle';
import { LIFECYCLE_STATE_LABELS } from '@/constants/terminology';
import { formatBytes, formatRelativeTime } from '@/lib/formatters';
import { describeSubcode, pickPrimarySubcode } from '@/utils/health';

const normalizeTrustState = (state?: string): string => {
  switch ((state ?? 'unknown').toLowerCase()) {
    case 'warn':
      return 'allowed_with_warning';
    case 'blocked_regressed':
      return 'blocked';
    default:
      return state ?? 'unknown';
  }
};

interface AdapterOverviewProps {
  adapter: AdapterDetailResponse | null;
  health: AdapterHealthResponse | null;
  isLoading: boolean;
}

export default function AdapterOverview({ adapter, health, isLoading }: AdapterOverviewProps) {
  if (isLoading && !adapter) {
    return <OverviewSkeleton />;
  }

  if (!adapter) {
    return (
      <div className="text-center py-12">
        <p className="text-muted-foreground">No adapter data available</p>
      </div>
    );
  }

  const adapterData = adapter.adapter;
  const metrics = adapter.metrics;
  const manifest = adapter.manifest;
  const adapterTrustState =
    adapterData?.adapter_trust_state ||
    (adapter as unknown as { adapter_trust_state?: string }).adapter_trust_state ||
    'unknown';
  const normalizedTrust = normalizeTrustState(adapterTrustState);
  const datasetTrustEntries: Array<{ id: string; trust_state: TrustState }> =
    (adapterData?.dataset_version_trust ?? []).length > 0
      ? (adapterData?.dataset_version_trust ?? []).map((snapshot) => ({
          id: snapshot.dataset_version_id,
          trust_state: (snapshot.trust_at_training_time ?? 'unknown') as TrustState,
        }))
      : (health?.datasets ?? []).map((ds) => ({
          id: ds.dataset_version_id,
          trust_state: (ds.trust_state as TrustState) ?? 'unknown',
        }));
  const adapterTrustVariant =
    normalizedTrust === 'blocked'
      ? 'destructive'
      : normalizedTrust === 'allowed_with_warning'
        ? 'secondary'
        : 'outline';

  const tenantId = adapter.tenant_id || adapterData?.tenant_id;
  const runtimeState =
    adapter.runtime_state ||
    adapter.current_state ||
    adapterData?.runtime_state ||
    adapterData?.current_state ||
    'unknown';
  const lifecycleState = adapter.lifecycle_state || adapterData?.lifecycle_state || 'active';
  const signatureValid =
    adapter.signature_valid ??
    adapterData?.signature_valid ??
    (adapter.content_hash_b3 || adapterData?.content_hash_b3 ? true : false);
  const primarySubcode = pickPrimarySubcode(health);
  const primarySubcodeDetail = primarySubcode ? describeSubcode(primarySubcode) : null;

  // Format timestamp helper
  const formatTime = (timestamp: string | undefined): string => {
    if (!timestamp) return 'Never';
    try {
      return formatRelativeTime(timestamp);
    } catch {
      return timestamp;
    }
  };

  return (
    <div className="space-y-6">
      {/* Top Row - Key Metrics */}
      <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
        <MetricCard
          icon={<Activity className="h-4 w-4" />}
          label="Inference Count"
          value={metrics?.inference_count?.toLocaleString() ?? '0'}
          helpText="Total number of inference requests processed by this adapter"
        />
        <MetricCard
          icon={<Zap className="h-4 w-4" />}
          label="Avg Latency"
          value={metrics?.avg_latency_ms ? `${metrics.avg_latency_ms.toFixed(1)} ms` : 'N/A'}
          helpText="Average response time for inference requests"
        />
        <MetricCard
          icon={<Database className="h-4 w-4" />}
          label="Total Tokens"
          value={metrics?.total_tokens?.toLocaleString() ?? '0'}
          helpText="Total tokens processed across all requests"
        />
        <MetricCard
          icon={<Target className="h-4 w-4" />}
          label="Error Rate"
          value={metrics?.error_count ? `${((metrics.error_count / (metrics.inference_count || 1)) * 100).toFixed(2)}%` : '0%'}
          helpText="Percentage of requests that resulted in errors"
        />
      </div>

      {/* Main Content Grid */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Basic Information */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Box className="h-5 w-5" />
              Basic Information
            </CardTitle>
            <CardDescription>Core adapter metadata and configuration</CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <InfoRow
              icon={<FileCode className="h-4 w-4" />}
              label="Name"
              value={adapterData?.name || adapterData?.adapter_name || 'Unknown'}
            />
            <InfoRow
              icon={<User className="h-4 w-4" />}
              label="Workspace"
              value={tenantId || adapter.tenant_namespace || 'N/A'}
            />
            <InfoRow
              icon={<Hash className="h-4 w-4" />}
              label="Hash (B3)"
              value={adapterData?.hash_b3 || adapter.hash_b3 || 'N/A'}
              truncate
            />
            <InfoRow
              icon={<Layers className="h-4 w-4" />}
              label="Tier"
              value={
                <Badge variant="outline">
                  Tier {adapterData?.tier ?? adapter.tier ?? 1}
                </Badge>
              }
            />
            <InfoRow
              icon={<TrendingUp className="h-4 w-4" />}
              label="Rank"
              value={adapterData?.rank ?? adapter.rank ?? 16}
            />
            <InfoRow
              icon={<Target className="h-4 w-4" />}
              label="Alpha"
              value={manifest?.alpha ?? adapter.alpha ?? 32}
            />
            <InfoRow
              icon={<Cpu className="h-4 w-4" />}
              label="Category"
              value={
                <Badge variant="secondary">
                  {adapterData?.category || adapter.category || 'code'}
                </Badge>
              }
            />
            <InfoRow
              icon={<GitBranch className="h-4 w-4" />}
              label="Scope"
              value={adapterData?.scope || adapter.scope || 'global'}
            />
            <InfoRow
              icon={<Cpu className="h-4 w-4" />}
              label="Base Model"
              value={adapter.base_model_id || adapterData?.base_model_id || manifest?.base_model || 'Unknown'}
            />
          </CardContent>
        </Card>

        {/* Lifecycle & State */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Activity className="h-5 w-5" />
              Lifecycle & State
            </CardTitle>
            <CardDescription>Current state and lifecycle information</CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <InfoRow
              icon={<Zap className="h-4 w-4" />}
              label="Current State"
              value={
                <Badge>
                  {LIFECYCLE_STATE_LABELS[runtimeState] || runtimeState}
                </Badge>
              }
            />
            <InfoRow
              icon={<Activity className="h-4 w-4" />}
              label="Lifecycle State"
              value={
                <Badge variant={getLifecycleVariant(lifecycleState)}>
                  {lifecycleState}
                </Badge>
              }
            />
            <InfoRow
              icon={<MemoryStick className="h-4 w-4" />}
              label="Memory Usage"
              value={formatBytes(adapterData?.memory_bytes || adapter.memory_bytes || 0)}
            />
            <InfoRow
              icon={<TrendingUp className="h-4 w-4" />}
              label="Activation Count"
              value={adapterData?.activation_count ?? adapter.activation_count ?? 0}
            />
            <InfoRow
              icon={<Clock className="h-4 w-4" />}
              label="Last Activated"
              value={formatTime(adapterData?.last_activated || adapter.last_activated)}
            />
            <InfoRow
              icon={<Calendar className="h-4 w-4" />}
              label="Created"
              value={formatTime(adapterData?.created_at)}
            />
            <InfoRow
              icon={<Calendar className="h-4 w-4" />}
              label="Updated"
              value={formatTime(adapterData?.updated_at)}
            />
          </CardContent>
        </Card>

        {/* Semantic Naming */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <User className="h-5 w-5" />
              Semantic Naming
              <GlossaryTooltip brief="Semantic naming follows the pattern: workspace/domain/purpose/revision" />
            </CardTitle>
            <CardDescription>Naming taxonomy and versioning</CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <InfoRow
              label="Workspace Namespace"
              value={adapterData?.tenant_namespace || adapter.tenant_namespace || 'N/A'}
            />
            <InfoRow
              label="Domain"
              value={adapterData?.domain || adapter.domain || 'N/A'}
            />
            <InfoRow
              label="Purpose"
              value={adapterData?.purpose || adapter.purpose || 'N/A'}
            />
            <InfoRow
              label="Revision"
              value={
                <Badge variant="outline">
                  {adapterData?.revision || adapter.revision || 'r001'}
                </Badge>
              }
            />
            <InfoRow
              label="Version"
              value={adapterData?.version || 'N/A'}
            />
            <InfoRow
              label="Framework"
              value={adapterData?.framework || adapter.framework || 'N/A'}
            />
            <InfoRow
              label="Signature / Compliance"
              value={
                <Badge variant={signatureValid ? 'default' : 'destructive'}>
                  {signatureValid ? 'Valid' : 'Missing'}
                </Badge>
              }
            />
            {adapter.content_hash_b3 && (
              <InfoRow
                label="Content Hash"
                value={adapter.content_hash_b3 || adapterData?.content_hash_b3}
                truncate
              />
            )}
          </CardContent>
        </Card>

        {/* Health Status */}
        <Card id="adapter-health">
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Activity className="h-5 w-5" />
              Health Status
              <GlossaryTooltip brief="Real-time health checks for the adapter" />
            </CardTitle>
            <CardDescription>Current health checks and status</CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            {health ? (
              <>
                <div className="flex items-center justify-between">
                  <span className="text-sm font-medium">Overall Health</span>
                  <div className="flex items-center gap-2">
                    <HealthBadge state={health.health} />
                    {primarySubcodeDetail ? (
                      <span className="text-sm font-medium">{primarySubcodeDetail.label}</span>
                    ) : (
                      <span className="text-sm text-muted-foreground">{health.health}</span>
                    )}
                    {health.subcodes?.length ? (
                      <a
                        href="#health-details"
                        className="text-xs text-primary underline-offset-4 hover:underline"
                      >
                        Health details
                      </a>
                    ) : null}
                  </div>
                </div>
                {health.drift_summary && (
                  <div className="text-xs text-muted-foreground">
                    Drift: {health.drift_summary.current.toFixed(4)}
                    {health.drift_summary.hard_threshold !== undefined &&
                      ` (hard ≥ ${health.drift_summary.hard_threshold})`}
                  </div>
                )}
                <div className="space-y-2 mt-4" id="health-details">
                  <h4 className="text-sm font-medium">Health details</h4>
                  {health.subcodes?.length ? (
                    health.subcodes.map((code, idx) => {
                      const detail = describeSubcode(code);
                      return (
                        <div
                          key={`${code.code}-${idx}`}
                          className="flex items-center justify-between py-1 border-b last:border-0"
                        >
                          <div className="flex flex-col">
                            <span className="text-sm font-medium">{detail.label}</span>
                            <span className="text-xs text-muted-foreground">
                              {code.domain} — {detail.detail}
                            </span>
                          </div>
                          <Badge variant="outline">{code.code}</Badge>
                        </div>
                      );
                    })
                  ) : (
                    <p className="text-sm text-muted-foreground">No health signals reported</p>
                  )}
                  {health.storage && (
                    <div className="mt-3 space-y-1">
                      <div className="flex items-center justify-between">
                        <span className="text-sm font-medium">Storage/Reconciler</span>
                        <Badge variant={health.storage.reconciler_status === 'corrupt' ? 'destructive' : 'default'}>
                          {health.storage.reconciler_status}
                        </Badge>
                      </div>
                      {health.storage.last_checked_at && (
                        <div className="text-xs text-muted-foreground">
                          Last checked: {formatTime(health.storage.last_checked_at)}
                        </div>
                      )}
                    </div>
                  )}
                </div>
              </>
            ) : (
              <p className="text-sm text-muted-foreground">Health data unavailable</p>
            )}
          </CardContent>
        </Card>

        {/* Debug Snapshot */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Cpu className="h-5 w-5" />
              Debug Snapshot
            </CardTitle>
            <CardDescription>Adapter-version focused diagnostics</CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <InfoRow
              label="Adapter Version ID"
              value={adapterData?.adapter_id || adapterData?.id || 'N/A'}
            />
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <div className="space-y-2">
                <h4 className="text-sm font-medium">Backend & CoreML</h4>
                <p className="text-sm text-muted-foreground">
                  Backend: {health?.backend?.backend || 'unknown'}
                </p>
                <p className="text-sm text-muted-foreground">
                  CoreML: {health?.backend?.coreml_used ? 'used' : 'not used'} {health?.backend?.coreml_device_type && `(${health.backend.coreml_device_type})`}
                </p>
              </div>
              <div className="space-y-2">
                <h4 className="text-sm font-medium">Drift</h4>
                <p className="text-sm text-muted-foreground">
                  {health?.drift_summary
                    ? `current ${health.drift_summary.current.toFixed(4)}`
                    : 'No drift signal reported'}
                </p>
              </div>
            </div>
            <div className="space-y-2">
              <h4 className="text-sm font-medium">Adapter Trust</h4>
              <div className="flex items-center gap-2">
                <Badge variant={adapterTrustVariant}>{normalizedTrust}</Badge>
                <span className="text-xs text-muted-foreground">
                  Adapter trust is the worst of its datasets: blocked &gt; warn &gt; unknown &gt; allowed.
                </span>
              </div>
            </div>
            <div className="space-y-2">
              <h4 className="text-sm font-medium">Datasets & Trust</h4>
              {datasetTrustEntries.length ? (
                datasetTrustEntries.map(ds => (
                  <div key={ds.id} className="flex items-center justify-between py-1 border-b last:border-0">
                    <span className="text-sm">{ds.id}</span>
                    <TrustBadge state={ds.trust_state} size="sm" />
                  </div>
                ))
              ) : (
                <p className="text-sm text-muted-foreground">No dataset links recorded</p>
              )}
            </div>
            {health?.storage && (
              <div className="space-y-2">
                <h4 className="text-sm font-medium">Storage/Reconciler</h4>
                <div className="flex items-center gap-2">
                  <Badge variant={health.storage.reconciler_status === 'corrupt' ? 'destructive' : 'default'}>
                    {health.storage.reconciler_status}
                  </Badge>
                  {health.storage.last_checked_at && (
                    <span className="text-xs text-muted-foreground">
                      Last checked {formatTime(health.storage.last_checked_at)}
                    </span>
                  )}
                </div>
                {health.storage.issues?.length ? (
                  <div className="space-y-1 text-sm text-muted-foreground">
                    {health.storage.issues.map((issue, idx) => {
                      const detail = describeSubcode(issue);
                      return (
                        <div key={`${issue.code}-${idx}`} className="flex items-center justify-between">
                          <div className="flex flex-col">
                            <span>{detail.label}</span>
                            <span className="text-xs text-muted-foreground">
                              {issue.domain} — {detail.detail}
                            </span>
                          </div>
                          <Badge variant="outline">{issue.code}</Badge>
                        </div>
                      );
                    })}
                  </div>
                ) : (
                  <p className="text-sm text-muted-foreground">No reconciler issues detected</p>
                )}
              </div>
            )}
          </CardContent>
        </Card>
      </div>

      {/* Model Configuration */}
      {manifest && (
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Cpu className="h-5 w-5" />
              Model Configuration
            </CardTitle>
            <CardDescription>Base model and training configuration from manifest</CardDescription>
          </CardHeader>
          <CardContent>
            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
              <InfoRow
                label="Base Model"
                value={manifest.base_model || 'Unknown'}
              />
              <InfoRow
                label="Rank"
                value={manifest.rank}
              />
              <InfoRow
                label="Alpha"
                value={manifest.alpha}
              />
              <InfoRow
                label="Target Modules"
                value={manifest.target_modules?.join(', ') || 'N/A'}
              />
              <InfoRow
                label="Quantization"
                value={
                  manifest.quantization
                    ? `${manifest.quantization} (backend-fixed)`
                    : 'None (backend-fixed)'
                }
              />
              <InfoRow
                label="Data Type"
                value={manifest.dtype || 'float16'}
              />
              <InfoRow
                label="Manifest Version"
                value={manifest.version}
              />
              <InfoRow
                label="Created"
                value={formatTime(manifest.created_at)}
              />
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}

// Helper component for metric cards
interface MetricCardProps {
  icon: React.ReactNode;
  label: string;
  value: string | number;
  helpText?: string;
}

function MetricCard({ icon, label, value, helpText }: MetricCardProps) {
  return (
    <Card>
      <CardContent className="pt-6">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2 text-muted-foreground">
            {icon}
            <span className="text-sm">{label}</span>
            {helpText && <GlossaryTooltip brief={helpText} />}
          </div>
        </div>
        <div className="text-2xl font-bold mt-2">{value}</div>
      </CardContent>
    </Card>
  );
}

// Helper component for info rows
interface InfoRowProps {
  icon?: React.ReactNode;
  label: string;
  value: React.ReactNode;
  truncate?: boolean;
}

function InfoRow({ icon, label, value, truncate }: InfoRowProps) {
  return (
    <div className="flex items-center justify-between py-1">
      <div className="flex items-center gap-2 text-muted-foreground">
        {icon}
        <span className="text-sm">{label}</span>
      </div>
      <div className={`text-sm font-medium ${truncate ? 'max-w-[calc(var(--base-unit)*50)] truncate' : ''}`}>
        {value}
      </div>
    </div>
  );
}

// Skeleton for loading state
function OverviewSkeleton() {
  return (
    <div className="space-y-6">
      <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
        {[...Array(4)].map((_, i) => (
          <Card key={i}>
            <CardContent className="pt-6">
              <Skeleton className="h-4 w-24 mb-2" />
              <Skeleton className="h-8 w-16" />
            </CardContent>
          </Card>
        ))}
      </div>
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {[...Array(4)].map((_, i) => (
          <Card key={i}>
            <CardHeader>
              <Skeleton className="h-6 w-32" />
              <Skeleton className="h-4 w-48" />
            </CardHeader>
            <CardContent>
              <div className="space-y-3">
                {[...Array(6)].map((_, j) => (
                  <div key={j} className="flex justify-between">
                    <Skeleton className="h-4 w-24" />
                    <Skeleton className="h-4 w-32" />
                  </div>
                ))}
              </div>
            </CardContent>
          </Card>
        ))}
      </div>
    </div>
  );
}
