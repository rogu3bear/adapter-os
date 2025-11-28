/**
 * Testing utilities for SSE streaming components
 *
 * Provides mocks and helpers for testing components that use streaming endpoints.
 *
 * Usage:
 * ```typescript
 * import { mockMetricsStream, mockTrainingStream } from '../streaming/test-utils';
 *
 * test('displays metrics', () => {
 *   mockMetricsStream({
 *     cpu: { usage_percent: 50, cores: 4 },
 *     memory: { used_gb: 8, total_gb: 16, usage_percent: 50 },
 *     // ...
 *   });
 *
 *   const { getByText } = render(<MetricsComponent />);
 *   expect(getByText(/50%/)).toBeInTheDocument();
 * });
 * ```
 */

import { vi } from 'vitest';
import type {
  TrainingStreamEvent,
  DiscoveryStreamEvent,
  ContactStreamEvent,
  FileChangeStreamEvent,
  MetricsStreamEvent,
  TelemetryStreamEvent,
  AdapterStreamEvent,
} from '../api/streaming-types';

// ============================================================================
// Mock Factories
// ============================================================================

/**
 * Create a mock training progress event
 */
export function createMockTrainingProgressEvent(
  overrides: Partial<TrainingStreamEvent> = {}
): TrainingStreamEvent {
  return {
    job_id: 'training-job-123',
    dataset_id: 'dataset-456',
    status: 'running',
    progress_pct: 50,
    current_epoch: 5,
    total_epochs: 10,
    current_loss: 0.5234,
    learning_rate: 0.001,
    tokens_per_second: 150,
    timestamp: new Date().toISOString(),
    ...(overrides as any),
  };
}

/**
 * Create a mock system metrics event
 * Returns MetricsSnapshotEvent by default (matches backend SSE format)
 */
export function createMockMetricsEvent(
  overrides: Partial<MetricsStreamEvent> = {}
): MetricsStreamEvent {
  // Default to MetricsSnapshotEvent format (what backend sends)
  const defaultSnapshot: import('../api/streaming-types').MetricsSnapshotEvent = {
    timestamp_ms: Date.now(),
    latency: {
      p50_ms: 10.5,
      p95_ms: 25.3,
      p99_ms: 45.2,
    },
    throughput: {
      tokens_per_second: 150.5,
      inferences_per_second: 12.3,
    },
    system: {
      cpu_percent: 45,
      memory_percent: 53,
      disk_percent: 49,
    },
  };

  return {
    ...defaultSnapshot,
    ...(overrides as any),
  };
}

/**
 * Create a mock adapter state transition event
 */
export function createMockAdapterStateTransitionEvent(
  overrides: Partial<AdapterStreamEvent> = {}
): AdapterStreamEvent {
  return {
    adapter_id: 'tenant-a/engineering/code-review/r001',
    tenant_id: 'tenant-a',
    previous_state: 'cold',
    new_state: 'warm',
    trigger: 'activation',
    timestamp: new Date().toISOString(),
    ...(overrides as any),
  } as AdapterStreamEvent;
}

/**
 * Create a mock adapter discovery event
 */
export function createMockDiscoveryEvent(
  overrides: Partial<DiscoveryStreamEvent> = {}
): DiscoveryStreamEvent {
  return {
    adapter_id: 'adapter-123',
    name: 'Test Adapter',
    version: '1.0.0',
    tier: 'warm',
    rank: 16,
    tags: ['test', 'example'],
    relevance_score: 0.95,
    timestamp: new Date().toISOString(),
    ...(overrides as any),
  } as DiscoveryStreamEvent;
}

/**
 * Create a mock telemetry event
 */
export function createMockTelemetryEvent(
  overrides: Partial<TelemetryStreamEvent> = {}
): TelemetryStreamEvent {
  return {
    event_id: 'telemetry-123',
    event_type: 'adapter.loaded',
    correlation_id: 'corr-456',
    user_id: 'user-789',
    tenant_id: 'tenant-a',
    resource_type: 'adapter',
    resource_id: 'adapter-123',
    action: 'load',
    status: 'success',
    duration_ms: 1234,
    metadata: { memory_freed_mb: 512 },
    timestamp: new Date().toISOString(),
    ...(overrides as any),
  } as TelemetryStreamEvent;
}

/**
 * Create a mock contact event
 */
export function createMockContactEvent(
  overrides: Partial<ContactStreamEvent> = {}
): ContactStreamEvent {
  return {
    contact_id: 'contact-123',
    action: 'added',
    name: 'John Doe',
    email: 'john@example.com',
    role: 'user',
    timestamp: new Date().toISOString(),
    ...(overrides as any),
  } as ContactStreamEvent;
}

/**
 * Create a mock file change event
 */
export function createMockFileChangeEvent(
  overrides: Partial<FileChangeStreamEvent> = {}
): FileChangeStreamEvent {
  return {
    path: '/data/models/adapter.bin',
    change_type: 'modified',
    is_directory: false,
    size_bytes: 1024000,
    mime_type: 'application/octet-stream',
    timestamp: new Date().toISOString(),
    ...(overrides as any),
  } as FileChangeStreamEvent;
}

// ============================================================================
// Hook Mocks
// ============================================================================

/**
 * Mock useMetricsStream hook
 */
export function mockMetricsStream(
  data: Partial<MetricsStreamEvent> = {},
  options: { error?: string | null; connected?: boolean } = {}
) {
  const mockData = createMockMetricsEvent(data);

  const useMetricsStreamMock = vi.fn(() => ({
    data: mockData,
    error: options.error ?? null,
    connected: options.connected ?? true,
    reconnect: vi.fn(),
    lastUpdated: 'timestamp' in mockData ? mockData.timestamp : undefined,
  }));

  vi.mock('../hooks/useStreamingEndpoints', () => ({
    useMetricsStream: useMetricsStreamMock,
  }));

  return useMetricsStreamMock;
}

/**
 * Mock useTrainingStream hook
 */
export function mockTrainingStream(
  data: Partial<TrainingStreamEvent> | null = null,
  options: { error?: string | null; connected?: boolean } = {}
) {
  const mockData = data ? createMockTrainingProgressEvent(data) : null;

  const useTrainingStreamMock = jest.fn(() => ({
    data: mockData,
    error: options.error ?? null,
    connected: options.connected ?? (data !== null),
    reconnect: vi.fn(),
    lastUpdated: mockData?.timestamp,
  }));

  vi.mock('../hooks/useStreamingEndpoints', () => ({
    useTrainingStream: useTrainingStreamMock,
  }));

  return useTrainingStreamMock;
}

/**
 * Mock useAdaptersStream hook
 */
export function mockAdaptersStream(
  data: Partial<AdapterStreamEvent> | null = null,
  options: { error?: string | null; connected?: boolean } = {}
) {
  const mockData = data ? createMockAdapterStateTransitionEvent(data) : null;

  const useAdaptersStreamMock = jest.fn(() => ({
    data: mockData,
    error: options.error ?? null,
    connected: options.connected ?? (data !== null),
    reconnect: vi.fn(),
    lastUpdated: mockData?.timestamp,
  }));

  vi.mock('../hooks/useStreamingEndpoints', () => ({
    useAdaptersStream: useAdaptersStreamMock,
  }));

  return useAdaptersStreamMock;
}

/**
 * Mock all streaming hooks at once
 */
export function mockAllStreams(defaults: { error?: string; connected?: boolean } = {}) {
  const mocks = {
    useMetricsStream: jest.fn(() => ({
      data: createMockMetricsEvent(),
      error: defaults.error ?? null,
      connected: defaults.connected ?? true,
      reconnect: vi.fn(),
    })),
    useTrainingStream: jest.fn(() => ({
      data: createMockTrainingProgressEvent(),
      error: defaults.error ?? null,
      connected: defaults.connected ?? true,
      reconnect: vi.fn(),
    })),
    useAdaptersStream: jest.fn(() => ({
      data: createMockAdapterStateTransitionEvent(),
      error: defaults.error ?? null,
      connected: defaults.connected ?? true,
      reconnect: vi.fn(),
    })),
    useDiscoveryStream: jest.fn(() => ({
      data: createMockDiscoveryEvent(),
      error: defaults.error ?? null,
      connected: defaults.connected ?? true,
      reconnect: vi.fn(),
    })),
    useContactsStream: jest.fn(() => ({
      data: createMockContactEvent(),
      error: defaults.error ?? null,
      connected: defaults.connected ?? true,
      reconnect: vi.fn(),
    })),
    useFileChangesStream: jest.fn(() => ({
      data: createMockFileChangeEvent(),
      error: defaults.error ?? null,
      connected: defaults.connected ?? true,
      reconnect: vi.fn(),
    })),
    useTelemetryStream: jest.fn(() => ({
      data: createMockTelemetryEvent(),
      error: defaults.error ?? null,
      connected: defaults.connected ?? true,
      reconnect: vi.fn(),
    })),
  };

  jest.mock('../hooks/useStreamingEndpoints', () => mocks);

  return mocks;
}

// ============================================================================
// Service Mocks
// ============================================================================

/**
 * Mock StreamingService
 */
export function mockStreamingService() {
  const mockService = {
    subscribeToMetrics: jest.fn(() => ({
      unsubscribe: jest.fn(),
      reconnect: vi.fn(),
      isConnected: jest.fn(() => true),
    })),
    subscribeToTraining: jest.fn(() => ({
      unsubscribe: jest.fn(),
      reconnect: vi.fn(),
      isConnected: jest.fn(() => true),
    })),
    subscribeToAdapters: jest.fn(() => ({
      unsubscribe: jest.fn(),
      reconnect: vi.fn(),
      isConnected: jest.fn(() => true),
    })),
    subscribeToDiscovery: jest.fn(() => ({
      unsubscribe: jest.fn(),
      reconnect: vi.fn(),
      isConnected: jest.fn(() => true),
    })),
    subscribeToContacts: jest.fn(() => ({
      unsubscribe: jest.fn(),
      reconnect: vi.fn(),
      isConnected: jest.fn(() => true),
    })),
    subscribeToFileChanges: jest.fn(() => ({
      unsubscribe: jest.fn(),
      reconnect: vi.fn(),
      isConnected: jest.fn(() => true),
    })),
    subscribeTelemetry: jest.fn(() => ({
      unsubscribe: jest.fn(),
      reconnect: vi.fn(),
      isConnected: jest.fn(() => true),
    })),
    getActiveSubscriptions: jest.fn(() => []),
    getSubscriptionCount: jest.fn(() => 0),
    unsubscribeAll: jest.fn(),
  };

  jest.mock('../services/StreamingService', () => ({
    streamingService: mockService,
    default: mockService,
  }));

  return mockService;
}

// ============================================================================
// EventSource Mock
// ============================================================================

/**
 * Mock EventSource for testing
 */
export class MockEventSource {
  public url: string;
  public readyState: number = EventSource.CONNECTING;
  public onopen: ((event: Event) => void) | null = null;
  public onmessage: ((event: MessageEvent) => void) | null = null;
  public onerror: ((event: Event) => void) | null = null;

  private listeners: Map<string, Set<(event: MessageEvent) => void>> = new Map();

  constructor(url: string) {
    this.url = url;
    // Simulate connection
    setTimeout(() => {
      this.readyState = EventSource.OPEN;
      if (this.onopen) {
        this.onopen(new Event('open'));
      }
    }, 0);
  }

  addEventListener(eventType: string, listener: (event: MessageEvent) => void) {
    if (!this.listeners.has(eventType)) {
      this.listeners.set(eventType, new Set());
    }
    this.listeners.get(eventType)!.add(listener);
  }

  removeEventListener(eventType: string, listener: (event: MessageEvent) => void) {
    this.listeners.get(eventType)?.delete(listener);
  }

  close() {
    this.readyState = EventSource.CLOSED;
  }

  /**
   * Emit an event for testing
   */
  emitEvent(eventType: string, data: any) {
    const event = new MessageEvent(eventType, {
      data: JSON.stringify(data),
    });

    if (eventType === 'message' && this.onmessage) {
      this.onmessage(event);
    } else {
      this.listeners.get(eventType)?.forEach((listener) => {
        listener(event);
      });
    }
  }

  /**
   * Simulate error
   */
  emitError(error?: string) {
    const event = new MessageEvent('error', { data: error });
    if (this.onerror) {
      this.onerror(event);
    }
  }
}

// ============================================================================
// Test Setup Helpers
// ============================================================================

/**
 * Setup EventSource mock for tests
 */
export function setupEventSourceMock() {
  const originalEventSource = global.EventSource;

  (global.EventSource as any) = MockEventSource;

  return {
    restore: () => {
      (global.EventSource as any) = originalEventSource;
    },
  };
}

/**
 * Create a test wrapper for streaming components
 */
export function createStreamingTestWrapper(props: any = {}) {
  return {
    defaultProps: {
      ...props,
    },
    mockSetup: mockAllStreams,
  };
}

// ============================================================================
// Assertion Helpers
// ============================================================================

/**
 * Assert that a hook was called with expected stream config
 */
export function assertStreamHookCalled(mock: jest.Mock, config?: Record<string, any>) {
  expect(mock).toHaveBeenCalled();
  if (config) {
    expect(mock).toHaveBeenCalledWith(expect.objectContaining(config));
  }
}

/**
 * Assert stream is connected
 */
export function assertStreamConnected(result: any) {
  expect(result.connected).toBe(true);
  expect(result.error).toBeNull();
}

/**
 * Assert stream is disconnected
 */
export function assertStreamDisconnected(result: any) {
  expect(result.connected).toBe(false);
}

/**
 * Assert stream has data
 */
export function assertStreamHasData(result: any) {
  expect(result.data).not.toBeNull();
  expect(result.data).toBeDefined();
}

export default {
  // Factories
  createMockTrainingProgressEvent,
  createMockMetricsEvent,
  createMockAdapterStateTransitionEvent,
  createMockDiscoveryEvent,
  createMockTelemetryEvent,
  createMockContactEvent,
  createMockFileChangeEvent,

  // Mock hooks
  mockMetricsStream,
  mockTrainingStream,
  mockAdaptersStream,
  mockAllStreams,

  // Mock service
  mockStreamingService,

  // EventSource
  MockEventSource,
  setupEventSourceMock,

  // Helpers
  createStreamingTestWrapper,
  assertStreamHookCalled,
  assertStreamConnected,
  assertStreamDisconnected,
  assertStreamHasData,
};
