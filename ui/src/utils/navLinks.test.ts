import { describe, expect, it } from 'vitest';
import {
  buildAdapterDetailLink,
  buildAdapterHealthLink,
  buildAdminTenantsLink,
  buildDashboardLink,
  buildDatasetChatLink,
  buildDatasetDetailLink,
  buildDocumentsLink,
  buildGoldenLink,
  buildInferenceLink,
  buildMetricsLink,
  buildReplayCompareLink,
  buildReplayRunsLink,
  buildRepoDetailLink,
  buildRepoVersionLink,
  buildRoutingLink,
  buildSecurityAuditLink,
  buildSecurityPoliciesLink,
  buildTelemetryAlertsLink,
  buildTelemetryEventStreamLink,
  buildTelemetryExportsLink,
  buildTelemetryFiltersLink,
  buildTelemetryTraceLink,
  buildTelemetryViewerLink,
  buildTestingLink,
  buildTrainingJobChatLink,
  buildTrainingJobDetailLink,
} from './navLinks';

describe('navLinks helpers', () => {
  it('builds replay runs link with session id', () => {
    expect(buildReplayRunsLink('abc123')).toBe('/replay/abc123');
  });

  it('builds replay runs link without session id', () => {
    expect(buildReplayRunsLink()).toBe('/replay');
  });

  it('encodes session id', () => {
    expect(buildReplayRunsLink('id with space')).toBe('/replay/id%20with%20space');
  });

  it('builds compare and telemetry links', () => {
    expect(buildReplayCompareLink()).toBe('/replay/compare');
    expect(buildReplayCompareLink('session-123')).toBe('/replay/session-123/compare');
    expect(buildTelemetryEventStreamLink()).toBe('/telemetry');
    expect(buildTelemetryFiltersLink()).toBe('/telemetry/filters');
    expect(buildTelemetryAlertsLink()).toBe('/telemetry/alerts');
    expect(buildTelemetryExportsLink()).toBe('/telemetry/exports');
  });

  it('builds telemetry viewer + trace links', () => {
    expect(buildTelemetryViewerLink()).toBe('/telemetry/viewer');
    expect(buildTelemetryTraceLink('trace-123')).toBe('/telemetry/viewer/trace-123');
    expect(buildTelemetryTraceLink('trace-123', { sourceType: 'code_intelligence' })).toBe(
      '/telemetry/viewer/trace-123?source_type=code_intelligence'
    );
    expect(buildTelemetryViewerLink({ sourceType: 'code_intelligence' })).toBe('/telemetry/viewer?source_type=code_intelligence');
  });

  it('builds training, dataset, adapter, and repo links', () => {
    expect(buildTrainingJobDetailLink('job-1')).toBe('/training/jobs/job-1');
    expect(buildTrainingJobChatLink('job-1')).toBe('/training/jobs/job-1/chat');
    expect(buildDatasetDetailLink('ds-1')).toBe('/training/datasets/ds-1');
    expect(buildDatasetDetailLink('ds-1', { datasetVersionId: 'v1' })).toBe('/training/datasets/ds-1?datasetVersionId=v1');
    expect(buildDatasetChatLink('ds-1')).toBe('/training/datasets/ds-1/chat');
    expect(buildAdapterDetailLink('adapter-1')).toBe('/adapters/adapter-1');
    expect(buildAdapterHealthLink('adapter-1')).toBe('/adapters/adapter-1#adapter-health');
    expect(buildRepoDetailLink('repo-1')).toBe('/repos/repo-1');
    expect(buildRepoVersionLink('repo-1', 'version-1')).toBe('/repos/repo-1/versions/version-1');
    expect(buildRepoVersionLink('repo id', 'ver id')).toBe('/repos/repo%20id/versions/ver%20id');
  });

  it('builds simple navigation links', () => {
    expect(buildInferenceLink()).toBe('/inference');
    expect(buildDashboardLink()).toBe('/dashboard');
    expect(buildMetricsLink()).toBe('/metrics');
    expect(buildRoutingLink()).toBe('/routing');
    expect(buildTestingLink()).toBe('/testing');
    expect(buildGoldenLink()).toBe('/golden');
    expect(buildDocumentsLink()).toBe('/documents');
  });

  it('builds security and admin links', () => {
    expect(buildSecurityPoliciesLink()).toBe('/security/policies');
    expect(buildSecurityAuditLink()).toBe('/security/audit');
    expect(buildAdminTenantsLink()).toBe('/admin/tenants');
    expect(buildAdminTenantsLink({ action: 'create' })).toBe('/admin/tenants?action=create');
  });
});

