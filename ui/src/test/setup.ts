import '@testing-library/jest-dom';
import { vi } from 'vitest';
import './matchers'; // Initialize custom matchers

// TextEncoder/TextDecoder polyfills
try {
  if (!(globalThis as any).TextEncoder) {
    const { TextEncoder } = await import('util');
    ;(globalThis as any).TextEncoder = TextEncoder as any;
  }
  if (!(globalThis as any).TextDecoder) {
    const { TextDecoder } = await import('util');
    ;(globalThis as any).TextDecoder = TextDecoder as any;
  }
} catch {}

// Web Crypto API
async function ensureCrypto() {
  const g = globalThis as any;
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
          const buf = Buffer.from(data as ArrayBuffer as any);
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
try {
  if (!(globalThis as any).localStorage) {
    (globalThis as any).localStorage = new MemoryStorage();
  } else {
    // Test if it's accessible (jsdom may throw SecurityError)
    (globalThis as any).localStorage.getItem('__test__');
  }
} catch {
  (globalThis as any).localStorage = new MemoryStorage();
}

try {
  if (!(globalThis as any).sessionStorage) {
    (globalThis as any).sessionStorage = new MemoryStorage();
  } else {
    (globalThis as any).sessionStorage.getItem('__test__');
  }
} catch {
  (globalThis as any).sessionStorage = new MemoryStorage();
}

// ResizeObserver stub
if (!(globalThis as any).ResizeObserver) {
  (globalThis as any).ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as any;
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
if (!(Element.prototype as any).scrollIntoView) {
  (Element.prototype as any).scrollIntoView = vi.fn();
}

// Default EventSource stub
if (!(globalThis as any).EventSource) {
  class EventSourceStub {
    url: string; readyState = 1; onerror: ((this: EventSource, ev: Event) => any) | null = null;
    constructor(url: string) { this.url = url; }
    addEventListener(_type: string, _listener: EventListenerOrEventListenerObject) {}
    close() { this.readyState = 2; }
  }
  vi.stubGlobal('EventSource', EventSourceStub as any);
}

// import.meta.env defaults
try {
  const env = (import.meta as any).env ?? {};
  Object.assign(env, { DEV: true, VITE_API_URL: '/api', VITE_SSE_URL: undefined });
  (import.meta as any).env = env;
} catch {}

// Safe default api client mock for providers/components
const defaultApiMock = {
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

vi.mock('@/api/client', () => ({ __esModule: true, default: defaultApiMock, apiClient: defaultApiMock }));
// Note: allow test files to mock '../api/client' themselves
