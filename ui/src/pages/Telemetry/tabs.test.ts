import { describe, expect, it } from 'vitest';
import { resolveTelemetryTab, telemetryTabToPath } from '@/pages/Telemetry/tabs';

describe('Telemetry tab mapping', () => {
  it('resolves tabs from routes and hashes', () => {
    expect(resolveTelemetryTab('/telemetry/viewer', '')).toBe('viewer');
    expect(resolveTelemetryTab('/telemetry', '#alerts')).toBe('alerts');
    expect(resolveTelemetryTab('/telemetry', '#exports')).toBe('exports');
    expect(resolveTelemetryTab('/telemetry', '#filters')).toBe('filters');
    expect(resolveTelemetryTab('/telemetry', '')).toBe('event-stream');
  });

  it('builds paths from tab enums', () => {
    expect(telemetryTabToPath('event-stream')).toBe('/telemetry');
    expect(telemetryTabToPath('viewer')).toBe('/telemetry/viewer');
    expect(telemetryTabToPath('alerts')).toBe('/telemetry#alerts');
    expect(telemetryTabToPath('exports')).toBe('/telemetry#exports');
    expect(telemetryTabToPath('filters')).toBe('/telemetry#filters');
  });
});


