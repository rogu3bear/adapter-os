import React from 'react';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor, within, fireEvent } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { ActivityFeedWidget } from '@/components/dashboard/ActivityFeedWidget';
import { logger } from '@/utils/logger';
import apiClient from '@/api/client';

vi.mock('@/api/client', () => {
  const mock = {
    getRecentActivityEvents: vi.fn(),
    listActivityEvents: vi.fn().mockResolvedValue([]),
    subscribeToActivity: vi.fn(() => () => {}),
    getToken: vi.fn(() => null),
    setToken: vi.fn(),
    getCurrentUser: vi.fn().mockResolvedValue({ user_id: 'u1', email: 'user@test.dev', role: 'viewer' }),
    listTenants: vi.fn().mockResolvedValue([]),
    login: vi.fn(),
    logout: vi.fn(),
  };
  return {
    __esModule: true,
    default: mock,
    apiClient: mock,
  };
});

const apiClientMock = apiClient as any;

vi.mock('@/providers/FeatureProviders', () => ({
  useTenant: () => ({ selectedTenant: 'tenant-1' }),
}));
const localStorageMock = {
  getItem: vi.fn().mockReturnValue(null),
  setItem: vi.fn(),
  removeItem: vi.fn(),
  clear: vi.fn(),
};

function renderWidget() {
  return render(
    <MemoryRouter>
      <ActivityFeedWidget />
    </MemoryRouter>
  );
}

const sampleEvents = [
  { id: 'e1', timestamp: new Date(Date.now() - 1000).toISOString(), event_type: 'policy_update', level: 'info', message: 'Policy updated', component: 'PolicyService' },
  { id: 'e2', timestamp: new Date(Date.now() - 5000).toISOString(), event_type: 'security_violation', level: 'error', message: 'Access denied', component: 'AuthGateway' },
  { id: 'e3', timestamp: new Date(Date.now() - 2000).toISOString(), event_type: 'build_complete', level: 'warning', message: 'Build completed', component: 'Planner' },
];

beforeEach(() => {
  vi.clearAllMocks();
  Object.defineProperty(global, 'localStorage', {
    value: localStorageMock,
    configurable: true,
    writable: true,
  });
  Object.defineProperty(window, 'matchMedia', {
    value: vi.fn().mockImplementation((query) => ({
      matches: false,
      media: query,
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
    })),
    configurable: true,
  });
  if (typeof global.EventSource === 'undefined') {
    class EventSourceMock {
      constructor(public url: string) {}
      addEventListener() {}
      close() {}
    }
    (global as any).EventSource = EventSourceMock;
  }
  apiClientMock.getRecentActivityEvents.mockResolvedValue([]);
  apiClientMock.listActivityEvents.mockResolvedValue([]);
});

afterEach(() => {
  vi.unstubAllGlobals();
  localStorageMock.getItem.mockReturnValue(null);
  localStorageMock.setItem.mockClear();
  localStorageMock.removeItem.mockClear();
  localStorageMock.clear.mockClear();
});

describe('ActivityFeedWidget integration', () => {
  it('renders events from API', async () => {
    apiClientMock.getRecentActivityEvents.mockResolvedValue(sampleEvents);

    renderWidget();

    expect(await screen.findByText(/Activity Feed/)).toBeInTheDocument();
    expect(await screen.findByText('Policy updated')).toBeInTheDocument();
    expect(await screen.findByText('Build completed')).toBeInTheDocument();
    expect(await screen.findByText('Access denied')).toBeInTheDocument();
  });

  it('SSE subscription updates feed with newest events first', async () => {
    apiClientMock.getRecentActivityEvents.mockResolvedValue([]);
    const now = Date.now();
    const EventSourceMock = vi.fn(() => ({
      addEventListener: vi.fn((type: string, listener: any) => {
        if (type === 'activity') {
          setTimeout(() => listener({
            data: JSON.stringify({ id: 's1', timestamp: new Date(now - 100).toISOString(), event_type: 'node_recovery', level: 'info', message: 'Node recovered' }),
          }), 0);
          setTimeout(() => listener({
            data: JSON.stringify({ id: 's2', timestamp: new Date(now + 100).toISOString(), event_type: 'security_violation', level: 'error', message: 'Unauthorized access' }),
          }), 5);
        }
      }),
      close: vi.fn(),
    }));
    vi.stubGlobal('EventSource', EventSourceMock as any);

    renderWidget();

    await waitFor(() => {
      expect(screen.getByText('Node recovered')).toBeInTheDocument();
      expect(screen.getByText('Unauthorized access')).toBeInTheDocument();
    });

    const unauthorizedRow = screen.getByRole('button', { name: /Unauthorized access/ });
    const recoveredRow = screen.getByRole('button', { name: /Node recovered/ });
    const rel = unauthorizedRow.compareDocumentPosition(recoveredRow);
    expect(rel & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it('SSE disconnect triggers polling fallback', async () => {
    apiClientMock.getRecentActivityEvents.mockResolvedValue([]);
    const intervalSpy = vi.spyOn(global, 'setInterval');

    vi.stubGlobal('EventSource', vi.fn(() => ({
      addEventListener: vi.fn((type: string, listener: any) => {
        if (type === 'error') {
          setTimeout(() => listener({ type: 'error' }), 0);
        }
      }),
      close: vi.fn(),
    })) as any);

    renderWidget();

    await waitFor(() => {
      expect(intervalSpy).toHaveBeenCalledWith(expect.any(Function), 5000);
    });
  });

  it('SSE auth error is logged and handled', async () => {
    apiClientMock.getRecentActivityEvents.mockResolvedValue([]);
    const errorSpy = vi.spyOn(logger, 'error');

    vi.stubGlobal('EventSource', vi.fn(() => ({
      addEventListener: vi.fn((type: string, listener: any) => {
        if (type === 'error') {
          setTimeout(() => listener({ type: 'error', status: 401 }), 0);
        }
      }),
      close: vi.fn(),
    })) as any);

    renderWidget();

    await waitFor(() => {
      expect(errorSpy).toHaveBeenCalled();
      const calls = (errorSpy as any).mock.calls as any[];
      expect(calls.some((c) => String(c[0]).includes('Activity SSE unauthorized'))).toBe(true);
    });
  });

  it('filters events by type', async () => {
    apiClientMock.getRecentActivityEvents.mockResolvedValue(sampleEvents);

    renderWidget();
    await screen.findByText('Policy updated');

    const typeTrigger = screen.getAllByRole('combobox')[0];
    fireEvent.click(typeTrigger);
    const policyItem = await screen.findByText('Policy');
    fireEvent.click(policyItem);

    await waitFor(() => {
      expect(screen.getByText('Policy updated')).toBeInTheDocument();
      expect(screen.queryByText('Build completed')).not.toBeInTheDocument();
      expect(screen.queryByText('Access denied')).not.toBeInTheDocument();
    });
  });

  it('filters events by severity', async () => {
    apiClientMock.getRecentActivityEvents.mockResolvedValue(sampleEvents);

    renderWidget();
    await screen.findByText('Policy updated');

    const severityTrigger = screen.getAllByRole('combobox')[1];
    fireEvent.click(severityTrigger);
    const errItem = await screen.findByText('Error');
    fireEvent.click(errItem);

    await waitFor(() => {
      expect(screen.getByText('Access denied')).toBeInTheDocument();
      expect(screen.queryByText('Policy updated')).not.toBeInTheDocument();
      expect(screen.queryByText('Build completed')).not.toBeInTheDocument();
    });
  });

  it('shows empty state', async () => {
    apiClientMock.getRecentActivityEvents.mockResolvedValue([]);

    renderWidget();

    expect(await screen.findByText('No recent activity')).toBeInTheDocument();
  });

  it('shows error state and logs error', async () => {
    const errorSpy = vi.spyOn(logger, 'error');
    apiClientMock.getRecentActivityEvents.mockRejectedValue(new Error('Network error'));

    renderWidget();

    expect(await screen.findByText(/Failed to load activity/)).toBeInTheDocument();
    expect(errorSpy).toHaveBeenCalled();
  });
});
