import '@testing-library/jest-dom';
import { vi } from 'vitest';
import './matchers'; // Initialize custom matchers

// TextEncoder/TextDecoder polyfills
try {
  type GlobalWithTextCodecs = typeof globalThis & {
    TextEncoder?: typeof TextEncoder;
    TextDecoder?: typeof TextDecoder;
  };
  const g = globalThis as GlobalWithTextCodecs;

  if (!g.TextEncoder) {
    const { TextEncoder: NodeTextEncoder } = await import('util');
    g.TextEncoder = NodeTextEncoder as unknown as typeof TextEncoder;
  }
  if (!g.TextDecoder) {
    const { TextDecoder: NodeTextDecoder } = await import('util');
    g.TextDecoder = NodeTextDecoder as unknown as typeof TextDecoder;
  }
} catch {}

// Web Crypto API
async function ensureCrypto() {
  type GlobalWithCrypto = typeof globalThis & { crypto?: Crypto };
  const g = globalThis as GlobalWithCrypto;

  if (g.crypto?.subtle && typeof g.crypto.getRandomValues === 'function') return;
  try {
    const { webcrypto } = await import('node:crypto');
    g.crypto = webcrypto as unknown as Crypto;
  } catch {
    const nodeCrypto = await import('node:crypto');
    g.crypto = {
      getRandomValues: (arr: Uint8Array) => { arr.set(nodeCrypto.randomBytes(arr.length)); return arr; },
      subtle: {
        async digest(alg: AlgorithmIdentifier, data: BufferSource): Promise<ArrayBuffer> {
          const algo = (typeof alg === 'string' ? alg : alg.name).toLowerCase();
          const hash = nodeCrypto.createHash(algo);
          const buf = Buffer.from(data as ArrayBuffer);
          hash.update(buf);
          return hash.digest().buffer.slice(0) as ArrayBuffer;
        },
      } as SubtleCrypto,
    } as Crypto;
  }
}
await ensureCrypto();

// Storage shims
class MemoryStorage implements Storage {
  private store = new Map<string, string>();
  get length() { return this.store.size; }
  clear() { this.store.clear(); }
  getItem(key: string) { return this.store.has(key) ? this.store.get(key)! : null; }
  key(index: number) { return Array.from(this.store.keys())[index] ?? null; }
  removeItem(key: string) { this.store.delete(key); }
  setItem(key: string, value: string) { this.store.set(key, String(value)); }
}

// Check if localStorage works, otherwise replace with MemoryStorage
type GlobalWithStorage = typeof globalThis & {
  localStorage?: Storage;
  sessionStorage?: Storage;
};
const gStorage = globalThis as GlobalWithStorage;

try {
  if (!gStorage.localStorage) {
    gStorage.localStorage = new MemoryStorage();
  } else {
    // Test if it's accessible (jsdom may throw SecurityError)
    gStorage.localStorage.getItem('__test__');
  }
} catch {
  gStorage.localStorage = new MemoryStorage();
}

try {
  if (!gStorage.sessionStorage) {
    gStorage.sessionStorage = new MemoryStorage();
  } else {
    gStorage.sessionStorage.getItem('__test__');
  }
} catch {
  gStorage.sessionStorage = new MemoryStorage();
}

// ResizeObserver stub
type GlobalWithResizeObserver = typeof globalThis & {
  ResizeObserver?: typeof ResizeObserver;
};
const gResizeObserver = globalThis as GlobalWithResizeObserver;

if (!gResizeObserver.ResizeObserver) {
  gResizeObserver.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
}

// matchMedia stub
if (!window.matchMedia) {
  Object.defineProperty(window, 'matchMedia', {
    writable: true,
    value: vi.fn().mockImplementation((query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
    })),
  });
}

// scrollIntoView stub
type ElementWithScrollIntoView = Element & {
  scrollIntoView?: (arg?: boolean | ScrollIntoViewOptions) => void;
};
const elementProto = Element.prototype as ElementWithScrollIntoView;

if (!elementProto.scrollIntoView) {
  elementProto.scrollIntoView = vi.fn();
}

// Default EventSource stub
type GlobalWithEventSource = typeof globalThis & {
  EventSource?: typeof EventSource;
};
const gEventSource = globalThis as GlobalWithEventSource;

if (!gEventSource.EventSource) {
  class EventSourceStub {
    url: string;
    readyState = 1;
    onerror: ((this: EventSource, ev: Event) => unknown) | null = null;
    constructor(url: string) { this.url = url; }
    addEventListener(_type: string, _listener: EventListenerOrEventListenerObject) {}
    close() { this.readyState = 2; }
  }
  vi.stubGlobal('EventSource', EventSourceStub as unknown as typeof EventSource);
}

// import.meta.env defaults
try {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- test setup needs to modify import.meta.env
  const meta = import.meta as any;
  const env = meta.env ?? {};
  Object.assign(env, { DEV: true, VITE_API_URL: '/api', VITE_SSE_URL: undefined });
  meta.env = env;
} catch {}

// Safe default api client mock for providers/components
// Uses Proxy to return a mock function for any method access
const createApiMock = () => {
  const methodCache: Record<string, ReturnType<typeof vi.fn>> = {};

  // Pre-defined mocks with specific behavior
  const predefinedMocks: Record<string, ReturnType<typeof vi.fn>> = {
    getToken: vi.fn(() => null),
    setToken: vi.fn(),
    getCurrentUser: vi.fn().mockResolvedValue({ user_id: 'u-test', email: 'test@example.com', role: 'viewer' }),
    login: vi.fn(),
    logout: vi.fn(),
    listTenants: vi.fn().mockResolvedValue([]),
    getSystemMetrics: vi.fn().mockResolvedValue(null),
    subscribeToMetrics: vi.fn(() => () => {}),
    getTelemetryEvents: vi.fn().mockResolvedValue([]),
    getRecentActivityEvents: vi.fn().mockResolvedValue([]),
    listActivityEvents: vi.fn().mockResolvedValue([]),
    subscribeToActivity: vi.fn(() => () => {}),
    listAlerts: vi.fn().mockResolvedValue([]),
    subscribeToAlerts: vi.fn(() => () => {}),
    getStatus: vi.fn().mockResolvedValue({ status: 'healthy', services: {} }),
  };

  return new Proxy({} as Record<string, unknown>, {
    get(_target, prop: string) {
      if (prop in predefinedMocks) {
        return predefinedMocks[prop];
      }
      // Create a mock function on first access and cache it
      if (!(prop in methodCache)) {
        methodCache[prop] = vi.fn().mockResolvedValue(undefined);
      }
      return methodCache[prop];
    },
  });
};

const defaultApiMock = createApiMock();

vi.mock('@/api/client', () => ({ __esModule: true, ApiClient: vi.fn(() => createApiMock()) }));
vi.mock('@/api/services', () => ({ __esModule: true, default: defaultApiMock, apiClient: defaultApiMock }));
// Note: allow test files to mock '../api/client' themselves
