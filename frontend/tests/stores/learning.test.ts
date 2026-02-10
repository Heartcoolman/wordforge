import { describe, it, expect, vi, beforeEach } from 'vitest';
import { STORAGE_KEYS } from '@/lib/storage';

const PREFIX = 'eng_';

async function getFreshStore() {
  vi.resetModules();
  const mod = await import('@/stores/learning');
  return mod.learningStore;
}

describe('learningStore', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('mode defaults to word-to-meaning', async () => {
    const store = await getFreshStore();
    expect(store.mode()).toBe('word-to-meaning');
  });

  it('setMode changes mode and persists to localStorage', async () => {
    const store = await getFreshStore();
    store.setMode('meaning-to-word');
    expect(store.mode()).toBe('meaning-to-word');
    expect(localStorage.getItem(PREFIX + STORAGE_KEYS.LEARNING_MODE)).toBe(
      JSON.stringify('meaning-to-word'),
    );
  });

  it('toggleMode switches between modes', async () => {
    const store = await getFreshStore();
    expect(store.mode()).toBe('word-to-meaning');
    store.toggleMode();
    expect(store.mode()).toBe('meaning-to-word');
    store.toggleMode();
    expect(store.mode()).toBe('word-to-meaning');
  });

  it('startSession sets sessionId and persists', async () => {
    const store = await getFreshStore();
    store.startSession('sess-123');
    expect(store.sessionId()).toBe('sess-123');
    expect(localStorage.getItem(PREFIX + STORAGE_KEYS.LEARNING_SESSION_ID)).toBe('sess-123');
  });

  it('clearSession clears sessionId and removes learning queue', async () => {
    const store = await getFreshStore();
    store.startSession('sess-abc');
    localStorage.setItem(PREFIX + STORAGE_KEYS.LEARNING_QUEUE, JSON.stringify([1, 2, 3]));
    store.clearSession();
    expect(store.sessionId()).toBeNull();
    expect(localStorage.getItem(PREFIX + STORAGE_KEYS.LEARNING_SESSION_ID)).toBeNull();
    expect(localStorage.getItem(PREFIX + STORAGE_KEYS.LEARNING_QUEUE)).toBeNull();
  });

  it('mode persists across fresh imports', async () => {
    const store = await getFreshStore();
    store.setMode('meaning-to-word');
    const store2 = await getFreshStore();
    expect(store2.mode()).toBe('meaning-to-word');
  });

  it('sessionId defaults to null', async () => {
    const store = await getFreshStore();
    expect(store.sessionId()).toBeNull();
  });
});
