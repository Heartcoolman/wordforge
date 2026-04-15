import '@testing-library/jest-dom/vitest';
import { cleanup } from '@solidjs/testing-library';
import { afterEach, beforeAll, vi } from 'vitest';
import { TEST_BASE_URL } from './helpers/constants';

function createStorageMock(): Storage {
  const store = new Map<string, string>();
  return {
    get length() {
      return store.size;
    },
    clear() {
      store.clear();
    },
    getItem(key: string) {
      return store.has(key) ? store.get(key)! : null;
    },
    key(index: number) {
      return Array.from(store.keys())[index] ?? null;
    },
    removeItem(key: string) {
      store.delete(key);
    },
    setItem(key: string, value: string) {
      store.set(key, String(value));
    },
  };
}

beforeAll(() => {
  const local = createStorageMock();
  const session = createStorageMock();

  Object.defineProperty(window, 'localStorage', {
    configurable: true,
    writable: true,
    value: local,
  });
  Object.defineProperty(window, 'sessionStorage', {
    configurable: true,
    writable: true,
    value: session,
  });
  Object.defineProperty(globalThis, 'localStorage', {
    configurable: true,
    writable: true,
    value: local,
  });
  Object.defineProperty(globalThis, 'sessionStorage', {
    configurable: true,
    writable: true,
    value: session,
  });

  // Set window.location for API client resolution
  Object.defineProperty(window, 'location', {
    writable: true,
    value: {
      ...window.location,
      origin: TEST_BASE_URL,
      href: TEST_BASE_URL,
      pathname: '/',
      search: '',
      hash: '',
    },
  });
});

afterEach(() => {
  cleanup();
  localStorage.clear();
});

// Mock navigator.sendBeacon (not available in some environments)
if (!navigator.sendBeacon) {
  Object.defineProperty(navigator, 'sendBeacon', {
    writable: true,
    value: vi.fn(() => true),
  });
}

// Mock matchMedia
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

if (!window.ResizeObserver) {
  class ResizeObserverMock {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  Object.defineProperty(window, 'ResizeObserver', {
    configurable: true,
    writable: true,
    value: ResizeObserverMock,
  });
  Object.defineProperty(globalThis, 'ResizeObserver', {
    configurable: true,
    writable: true,
    value: ResizeObserverMock,
  });
}
