import '@testing-library/jest-dom/vitest';
import { cleanup } from '@solidjs/testing-library';
import { afterEach, beforeAll, vi } from 'vitest';
import { TEST_BASE_URL } from './helpers/constants';

beforeAll(() => {
  // Set window.location for API client resolution
  Object.defineProperty(window, 'location', {
    writable: true,
    value: {
      ...window.location,
      origin: TEST_BASE_URL,
      href: TEST_BASE_URL,
      pathname: '/',
      search: '',
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
