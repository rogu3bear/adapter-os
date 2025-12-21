/**
 * Navigation link builders for common routes.
 * Keep these as the single canonical place to change URL formats.
 */

export const ROUTE_PATHS = {
  dashboard: '/dashboard',
  inference: '/inference',
  metrics: '/metrics',
  routing: '/routing',
  testing: '/testing',
  golden: '/golden',
  documents: '/documents',
  replay: {
    runs: '/replay',
    runsWithSession: '/replay/:sessionId',
    decisionTrace: '/replay/decision-trace',
    decisionTraceWithSession: '/replay/:sessionId/decision-trace',
    evidence: '/replay/evidence',
    evidenceWithSession: '/replay/:sessionId/evidence',
    compare: '/replay/compare',
    compareWithSession: '/replay/:sessionId/compare',
    export: '/replay/export',
    exportWithSession: '/replay/:sessionId/export',
  },
  telemetry: {
    eventStream: '/telemetry',
    viewer: '/telemetry/viewer',
    viewerTrace: '/telemetry/viewer/:traceId',
    alerts: '/telemetry/alerts',
    exports: '/telemetry/exports',
    filters: '/telemetry/filters',
    legacyTraces: '/telemetry/traces',
    legacyTrace: '/telemetry/traces/:traceId',
  },
  training: {
    overview: '/training',
    jobs: '/training/jobs',
    jobDetail: '/training/jobs/:jobId',
    jobChat: '/training/jobs/:jobId/chat',
    datasets: '/training/datasets',
    datasetDetail: '/training/datasets/:datasetId',
    datasetChat: '/training/datasets/:datasetId/chat',
    templates: '/training/templates',
    artifacts: '/training/artifacts',
    settings: '/training/settings',
  },
  adapters: {
    list: '/adapters',
    register: '/adapters/new',
    overview: '/adapters/:adapterId',
    activations: '/adapters/:adapterId/activations',
    usage: '/adapters/:adapterId/usage',
    lineage: '/adapters/:adapterId/lineage',
    manifest: '/adapters/:adapterId/manifest',
    policies: '/adapters/:adapterId/policies',
  },
  repos: {
    list: '/repos',
    detail: '/repos/:repoId',
    version: '/repos/:repoId/versions/:versionId',
  },
  security: {
    policies: '/security/policies',
    audit: '/security/audit',
  },
  admin: {
    root: '/admin',
    tenants: '/admin/tenants',
    stacks: '/admin/stacks',
    users: '/admin/users',
    settings: '/admin/settings',
  },
  system: {
    root: '/system',
    overview: '/system',
    memory: '/system/memory',
    nodes: '/system/nodes',
    workers: '/system/workers',
    metrics: '/system/metrics',
  },
  baseModels: '/base-models',
  routerConfig: '/router-config',
} as const;

export function buildReplayRunsLink(sessionId?: string): string {
  if (sessionId) {
    return buildPath(ROUTE_PATHS.replay.runsWithSession, { sessionId });
  }
  return ROUTE_PATHS.replay.runs;
}

export function buildReplayDecisionTraceLink(sessionId?: string): string {
  if (sessionId) {
    return buildPath(ROUTE_PATHS.replay.decisionTraceWithSession, { sessionId });
  }
  return ROUTE_PATHS.replay.decisionTrace;
}

export function buildReplayEvidenceLink(sessionId?: string): string {
  if (sessionId) {
    return buildPath(ROUTE_PATHS.replay.evidenceWithSession, { sessionId });
  }
  return ROUTE_PATHS.replay.evidence;
}

export function buildReplayCompareLink(sessionId?: string): string {
  if (sessionId) {
    return buildPath(ROUTE_PATHS.replay.compareWithSession, { sessionId });
  }
  return ROUTE_PATHS.replay.compare;
}

export function buildReplayExportLink(sessionId?: string): string {
  if (sessionId) {
    return buildPath(ROUTE_PATHS.replay.exportWithSession, { sessionId });
  }
  return ROUTE_PATHS.replay.export;
}

function hasNonEmptyValue(value?: string | null): value is string {
  return Boolean(value && value.trim().length > 0);
}

function buildQuery(params: Record<string, string | undefined>): string {
  const entries: string[] = [];
  for (const [key, value] of Object.entries(params)) {
    if (!hasNonEmptyValue(value)) continue;
    entries.push(`${encodeURIComponent(key)}=${encodeURIComponent(value.trim())}`);
  }
  return entries.length > 0 ? `?${entries.join('&')}` : '';
}

function buildPath(pathPattern: string, params: Record<string, string>): string {
  return pathPattern.replace(/:([A-Za-z0-9_]+)/g, (match, key: string) => {
    const value = params[key];
    if (value === undefined) {
      throw new Error(`Missing "${key}" param for path pattern "${pathPattern}"`);
    }
    return encodeURIComponent(value);
  });
}

export interface TelemetryLinkOptions {
  sourceType?: string;
}

export function buildTelemetryEventStreamLink(options: TelemetryLinkOptions = {}): string {
  return `${ROUTE_PATHS.telemetry.eventStream}${buildQuery({ source_type: options.sourceType })}`;
}

export function buildTelemetryViewerLink(options: TelemetryLinkOptions = {}): string {
  return `${ROUTE_PATHS.telemetry.viewer}${buildQuery({ source_type: options.sourceType })}`;
}

export function buildTelemetryTraceLink(traceId: string, options: TelemetryLinkOptions = {}): string {
  return `${buildPath(ROUTE_PATHS.telemetry.viewerTrace, { traceId })}${buildQuery({ source_type: options.sourceType })}`;
}

export function buildTelemetryAlertsLink(options: TelemetryLinkOptions = {}): string {
  return `${ROUTE_PATHS.telemetry.alerts}${buildQuery({ source_type: options.sourceType })}`;
}

export function buildTelemetryExportsLink(options: TelemetryLinkOptions = {}): string {
  return `${ROUTE_PATHS.telemetry.exports}${buildQuery({ source_type: options.sourceType })}`;
}

export function buildTelemetryFiltersLink(options: TelemetryLinkOptions = {}): string {
  return `${ROUTE_PATHS.telemetry.filters}${buildQuery({ source_type: options.sourceType })}`;
}

export function buildTrainingJobDetailLink(jobId: string): string {
  return buildPath(ROUTE_PATHS.training.jobDetail, { jobId });
}

export function buildTrainingJobChatLink(jobId: string): string {
  return buildPath(ROUTE_PATHS.training.jobChat, { jobId });
}

export interface DatasetDetailLinkOptions {
  datasetVersionId?: string;
}

export function buildDatasetDetailLink(datasetId: string, options: DatasetDetailLinkOptions = {}): string {
  return `${buildPath(ROUTE_PATHS.training.datasetDetail, { datasetId })}${buildQuery({
    datasetVersionId: options.datasetVersionId,
  })}`;
}

export function buildDatasetChatLink(datasetId: string): string {
  return buildPath(ROUTE_PATHS.training.datasetChat, { datasetId });
}

export function buildAdapterDetailLink(adapterId: string): string {
  return buildPath(ROUTE_PATHS.adapters.overview, { adapterId });
}

export function buildAdapterHealthLink(adapterId: string): string {
  return `${buildAdapterDetailLink(adapterId)}#adapter-health`;
}

export function buildRepoDetailLink(repoId: string): string {
  return buildPath(ROUTE_PATHS.repos.detail, { repoId });
}

export function buildRepoVersionLink(repoId: string, versionId: string): string {
  return buildPath(ROUTE_PATHS.repos.version, { repoId, versionId });
}

// Simple list/overview links
export function buildTrainingOverviewLink(): string {
  return ROUTE_PATHS.training.overview;
}

export function buildTrainingJobsLink(options: { adapterId?: string; datasetId?: string } = {}): string {
  return `${ROUTE_PATHS.training.jobs}${buildQuery({ adapterId: options.adapterId, datasetId: options.datasetId })}`;
}

export function buildTrainingDatasetsLink(): string {
  return ROUTE_PATHS.training.datasets;
}

export function buildAdaptersListLink(options: { action?: string } = {}): string {
  return `${ROUTE_PATHS.adapters.list}${buildQuery({ action: options.action })}`;
}

export function buildAdaptersRegisterLink(): string {
  return ROUTE_PATHS.adapters.register;
}

export function buildChatLink(options: { sessionId?: string; stackId?: string } = {}): string {
  return `/chat${buildQuery({ session: options.sessionId, stack: options.stackId })}`;
}

export function buildReplayLink(hash?: string): string {
  return hash ? `${ROUTE_PATHS.replay.runs}#${hash}` : ROUTE_PATHS.replay.runs;
}

export function buildTelemetryLink(): string {
  return ROUTE_PATHS.telemetry.eventStream;
}

export function buildReposListLink(): string {
  return ROUTE_PATHS.repos.list;
}

// Simple builders (no params)
export function buildInferenceLink(): string {
  return ROUTE_PATHS.inference;
}

export function buildDashboardLink(): string {
  return ROUTE_PATHS.dashboard;
}

export function buildMetricsLink(): string {
  return ROUTE_PATHS.metrics;
}

export function buildRoutingLink(): string {
  return ROUTE_PATHS.routing;
}

export function buildTestingLink(): string {
  return ROUTE_PATHS.testing;
}

export function buildGoldenLink(): string {
  return ROUTE_PATHS.golden;
}

export function buildDocumentsLink(): string {
  return ROUTE_PATHS.documents;
}

// With optional params
export function buildSecurityPoliciesLink(): string {
  return ROUTE_PATHS.security.policies;
}

export function buildSecurityAuditLink(): string {
  return ROUTE_PATHS.security.audit;
}

export function buildAdminTenantsLink(options: { action?: string } = {}): string {
  return `${ROUTE_PATHS.admin.tenants}${buildQuery({ action: options.action })}`;
}

export function buildAdminStacksLink(): string {
  return ROUTE_PATHS.admin.stacks;
}

export function buildAdminSettingsLink(): string {
  return ROUTE_PATHS.admin.settings;
}

export function buildSystemLink(): string {
  return ROUTE_PATHS.system.overview;
}

export function buildSystemOverviewLink(): string {
  return ROUTE_PATHS.system.root;
}

export function buildSystemNodesLink(): string {
  return ROUTE_PATHS.system.nodes;
}

export function buildSystemWorkersLink(): string {
  return ROUTE_PATHS.system.workers;
}

export function buildSystemMemoryLink(): string {
  return ROUTE_PATHS.system.memory;
}

export function buildSystemMetricsLink(): string {
  return ROUTE_PATHS.system.metrics;
}

export function buildAdminLink(): string {
  return ROUTE_PATHS.admin.root;
}

export function buildBaseModelsLink(): string {
  return ROUTE_PATHS.baseModels;
}

export function buildRouterConfigLink(): string {
  return ROUTE_PATHS.routerConfig;
}
