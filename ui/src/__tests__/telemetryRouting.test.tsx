import React from 'react';
import { describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter, Route, Routes, useLocation } from 'react-router-dom';

vi.mock('@/components/LegacyRedirectNotice', () => ({
  __esModule: true,
  default: ({ to, label }: { to: string; label?: string }) => (
    <div data-testid="legacy-redirect" data-to={to} data-label={label ?? ''} />
  ),
}));

vi.mock('@/providers/CoreProviders', () => ({
  useAuth: () => ({
    user: { tenant_id: 'tenant-1' },
  }),
  useResize: () => undefined,
}));

vi.mock('@/providers/FeatureProviders', () => ({
  useTenant: () => ({
    selectedTenant: undefined,
  }),
}));

vi.mock('@/components/Telemetry', () => ({
  Telemetry: () => <div data-testid="telemetry" />,
}));

vi.mock('@/components/telemetry/TelemetryViewer', () => ({
  TelemetryViewer: () => <div data-testid="telemetry-viewer" />,
}));

vi.mock('@/components/trace/TraceSummaryPanel', () => ({
  TraceSummaryPanel: () => <div data-testid="trace-summary" />,
}));

vi.mock('@/components/trace/TraceTokenTable', () => ({
  TraceTokenTable: () => <div data-testid="trace-token-table" />,
}));

vi.mock('@/components/GoldenRuns', () => ({
  GoldenRuns: () => <div data-testid="golden-runs" />,
}));

vi.mock('@/components/RoutingInspector', () => ({
  RoutingInspector: () => <div data-testid="routing-inspector" />,
}));

vi.mock('@/hooks/observability/useTrace', () => ({
  useTrace: () => ({
    data: null,
    isLoading: false,
    isFetching: false,
    isError: false,
    error: null,
  }),
}));

import { routes } from '@/config/routes';
import { useTelemetryTabRouter } from '@/hooks/navigation/useTabRouter';
import { ROUTE_PATHS } from '@/utils/navLinks';

function LocationDisplay() {
  const location = useLocation();
  return <div data-testid="location">{`${location.pathname}${location.search}`}</div>;
}

function SuspendedRouteComponent({ component }: { component: unknown }) {
  const Component = component as React.ComponentType;
  return (
    <React.Suspense fallback={<div data-testid="route-loading" />}>
      <Component />
    </React.Suspense>
  );
}

const TELEMETRY_ROUTE_PATHS = [
  ROUTE_PATHS.telemetry.eventStream,
  ROUTE_PATHS.telemetry.viewer,
  ROUTE_PATHS.telemetry.viewerTrace,
  ROUTE_PATHS.telemetry.alerts,
  ROUTE_PATHS.telemetry.exports,
  ROUTE_PATHS.telemetry.filters,
] as const;

function TelemetryRoutesHarness() {
  return (
    <>
      <Routes>
        {TELEMETRY_ROUTE_PATHS.map((path) => {
          const route = routes.find((r) => r.path === path);
          if (!route) {
            throw new Error(`Missing telemetry route config for ${path}`);
          }
          return <Route key={path} path={path} element={<SuspendedRouteComponent component={route.component} />} />;
        })}
      </Routes>
      <LocationDisplay />
    </>
  );
}

function TelemetryTabHarness() {
  const { activeTab, setActiveTab, getTabPath } = useTelemetryTabRouter();
  const location = useLocation();
  return (
    <div>
      <div data-testid="active-tab">{activeTab}</div>
      <div data-testid="location">{`${location.pathname}${location.search}`}</div>
      <div data-testid="path-viewer">{getTabPath('viewer')}</div>
      <div data-testid="path-default">{getTabPath('event-stream')}</div>
      <button type="button" onClick={() => setActiveTab('viewer')}>to-viewer</button>
      <button type="button" onClick={() => setActiveTab('event-stream')}>to-default</button>
    </div>
  );
}

function parseRedirectTarget(to: string) {
  return new URL(to, 'https://example.com');
}

describe('Telemetry routing', () => {
  it('redirects legacy tab=traces to /telemetry/viewer', async () => {
    render(
      <MemoryRouter initialEntries={['/telemetry?tab=traces']}>
        <TelemetryRoutesHarness />
      </MemoryRouter>,
    );

    await waitFor(() => {
      expect(screen.getByTestId('location')).toHaveTextContent('/telemetry/viewer');
    });
  });

  it('canonicalizes legacy query params and drops unrelated params', async () => {
    render(
      <MemoryRouter initialEntries={['/telemetry?tab=viewer&trace_id=req-555&source_type=code_intelligence&foo=bar']}>
        <TelemetryRoutesHarness />
      </MemoryRouter>,
    );

    await waitFor(() => {
      const loc = screen.getByTestId('location').textContent ?? '';
      expect(loc).toContain('/telemetry/viewer/req-555');
      expect(loc).toContain('source_type=code_intelligence');
      expect(loc).not.toContain('foo=bar');
      expect(loc).not.toContain('tab=');
      expect(loc).not.toContain('trace_id=');
      expect(loc).not.toContain('traceId=');
      expect(loc).not.toContain('requestId=');
    });
  });

  it('canonicalizes legacy tab=alerts to /telemetry/alerts', async () => {
    render(
      <MemoryRouter initialEntries={['/telemetry?tab=alerts&foo=bar']}>
        <TelemetryRoutesHarness />
      </MemoryRouter>,
    );

    await waitFor(() => {
      const loc = screen.getByTestId('location').textContent ?? '';
      expect(loc).toContain('/telemetry/alerts');
      expect(loc).not.toContain('foo=bar');
      expect(loc).not.toContain('tab=');
    });
  });

  it('canonicalizes legacy requestId to /telemetry/viewer/:traceId', async () => {
    render(
      <MemoryRouter initialEntries={['/telemetry?requestId=req-777']}>
        <TelemetryRoutesHarness />
      </MemoryRouter>,
    );

    await waitFor(() => {
      const loc = screen.getByTestId('location').textContent ?? '';
      expect(loc).toContain('/telemetry/viewer/req-777');
      expect(loc).not.toContain('requestId=');
    });
  });

  it('resolves telemetry tab state from canonical paths', async () => {
    const user = userEvent.setup();

    render(
      <MemoryRouter initialEntries={['/telemetry/viewer']}>
        <Routes>
          <Route path="*" element={<TelemetryTabHarness />} />
        </Routes>
      </MemoryRouter>,
    );

    expect(screen.getByTestId('active-tab')).toHaveTextContent('viewer');
    await user.click(screen.getByRole('button', { name: 'to-default' }));

    expect(screen.getByTestId('active-tab')).toHaveTextContent('event-stream');
    expect(screen.getByTestId('location')).toHaveTextContent('/telemetry');
    expect(screen.getByTestId('path-default')).toHaveTextContent('/telemetry');
  });

  it('treats /telemetry/viewer/:traceId as the trace tab', () => {
    render(
      <MemoryRouter initialEntries={['/telemetry/viewer/req-123']}>
        <Routes>
          <Route path="*" element={<TelemetryTabHarness />} />
        </Routes>
      </MemoryRouter>,
    );

    expect(screen.getByTestId('active-tab')).toHaveTextContent('viewer-trace');
  });

  it('selects Telemetry tab triggers from canonical routes', async () => {
    render(
      <MemoryRouter initialEntries={['/telemetry/viewer/req-123']}>
        <TelemetryRoutesHarness />
      </MemoryRouter>,
    );

    const traceTab = await screen.findByRole('tab', { name: 'Trace' });
    expect(traceTab).toHaveAttribute('data-state', 'active');
    expect(traceTab).toHaveAttribute('href', '/telemetry/viewer/req-123');

    const viewerTab = screen.getByRole('tab', { name: 'Viewer' });
    expect(viewerTab).toHaveAttribute('href', '/telemetry/viewer');
    expect(viewerTab).not.toHaveAttribute('data-state', 'active');

    const streamTab = screen.getByRole('tab', { name: 'Event Stream' });
    expect(streamTab).toHaveAttribute('href', '/telemetry');
  });

  it('navigates between telemetry tabs using canonical paths', async () => {
    const user = userEvent.setup();

    render(
      <MemoryRouter initialEntries={['/telemetry']}>
        <TelemetryRoutesHarness />
      </MemoryRouter>,
    );

    const viewerTab = await screen.findByRole('tab', { name: 'Viewer' });
    await user.click(viewerTab);

    await waitFor(() => {
      expect(screen.getByTestId('location')).toHaveTextContent('/telemetry/viewer');
    });
  });

  it('redirects legacy trace routes and canonicalizes trace id params', () => {
    const traceRoute = routes.find(r => r.path === '/telemetry/traces/:traceId');
    expect(traceRoute).toBeDefined();

    const Component = traceRoute!.component as React.ComponentType;

    render(
      <MemoryRouter initialEntries={['/telemetry/traces/req-123?tab=events&trace_id=legacy&traceId=legacy2&foo=bar']}>
        <Routes>
          <Route path="/telemetry/traces/:traceId" element={<Component />} />
        </Routes>
      </MemoryRouter>,
    );

    const to = screen.getByTestId('legacy-redirect').getAttribute('data-to');
    expect(to).toBeTruthy();

    const url = parseRedirectTarget(to!);
    expect(url.pathname).toBe('/telemetry/viewer/req-123');
    expect(url.searchParams.has('foo')).toBe(false);
    expect(url.searchParams.has('trace_id')).toBe(false);
    expect(url.searchParams.has('traceId')).toBe(false);
  });

  it('canonicalizes legacy traceId search params on /telemetry', async () => {
    render(
      <MemoryRouter initialEntries={['/telemetry?tab=viewer&trace_id=req-555']}>
        <TelemetryRoutesHarness />
      </MemoryRouter>,
    );

    await waitFor(() => {
      const loc = screen.getByTestId('location').textContent ?? '';
      expect(loc).toContain('/telemetry/viewer/req-555');
      expect(loc).not.toContain('trace_id=');
      expect(loc).not.toContain('traceId=');
    });
  });

  it('renders a single top-level heading for Telemetry route component', async () => {
    const route = routes.find(r => r.path === '/telemetry');
    expect(route).toBeDefined();

    render(
      <MemoryRouter initialEntries={['/telemetry']}>
        <Routes>
          <Route path="/telemetry" element={<SuspendedRouteComponent component={route!.component} />} />
        </Routes>
      </MemoryRouter>,
    );

    await waitFor(() => {
      expect(screen.getAllByRole('heading', { level: 1 })).toHaveLength(1);
    });
  });

  it('renders a single top-level heading for Golden route component', async () => {
    const route = routes.find(r => r.path === '/golden');
    expect(route).toBeDefined();

    render(
      <MemoryRouter initialEntries={['/golden']}>
        <Routes>
          <Route path="/golden" element={<SuspendedRouteComponent component={route!.component} />} />
        </Routes>
      </MemoryRouter>,
    );

    await waitFor(() => {
      expect(screen.getAllByRole('heading', { level: 1 })).toHaveLength(1);
    });
  });

  it('renders a single top-level heading for Routing route component', async () => {
    const route = routes.find(r => r.path === '/routing');
    expect(route).toBeDefined();

    render(
      <MemoryRouter initialEntries={['/routing']}>
        <Routes>
          <Route path="/routing" element={<SuspendedRouteComponent component={route!.component} />} />
        </Routes>
      </MemoryRouter>,
    );

    await waitFor(() => {
      expect(screen.getAllByRole('heading', { level: 1 })).toHaveLength(1);
    });
  });
});
