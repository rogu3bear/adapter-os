import { describe, expect, it } from 'vitest';
import { buildReplayRunsLink, buildReplayCompareLink, buildTelemetryFiltersLink } from './navLinks';

describe('navLinks helpers', () => {
  it('builds replay runs link with session id', () => {
    expect(buildReplayRunsLink('abc123')).toBe('/replay?sessionId=abc123#runs');
  });

  it('builds replay runs link without session id', () => {
    expect(buildReplayRunsLink()).toBe('/replay#runs');
  });

  it('encodes session id', () => {
    expect(buildReplayRunsLink('id with space')).toBe('/replay?sessionId=id%20with%20space#runs');
  });

  it('builds compare and telemetry links', () => {
    expect(buildReplayCompareLink()).toBe('/replay#compare');
    expect(buildTelemetryFiltersLink()).toBe('/telemetry#filters');
  });
});


