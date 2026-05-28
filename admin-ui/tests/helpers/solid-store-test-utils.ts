import { vi } from 'vitest';

/**
 * Reset modules and dynamically import a store module to get a fresh instance.
 * SolidJS stores use `createRoot` at module level, so we need `vi.resetModules()`
 * to break the singleton cache between tests.
 */
export async function freshImport<T>(modulePath: string): Promise<T> {
  vi.resetModules();
  return await import(modulePath) as T;
}
