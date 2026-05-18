import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { storage, STORAGE_KEYS } from '@/lib/storage';

describe('storage', () => {
  beforeEach(() => {
    localStorage.clear();
    sessionStorage.clear();
  });

  it('get returns fallback when key not found', () => {
    expect(storage.get('nonexistent', 42)).toBe(42);
  });

  it('get parses JSON value correctly', () => {
    localStorage.setItem('eng_data', JSON.stringify({ a: 1 }));
    expect(storage.get('data', null)).toEqual({ a: 1 });
  });

  it('get returns fallback on invalid JSON', () => {
    localStorage.setItem('eng_bad', '{broken');
    expect(storage.get('bad', 'default')).toBe('default');
  });

  it('set stores JSON value with prefix', () => {
    storage.set('key1', { x: 10 });
    expect(localStorage.getItem('eng_key1')).toBe(JSON.stringify({ x: 10 }));
  });

  it('remove deletes key with prefix', () => {
    localStorage.setItem('eng_rm', 'val');
    storage.remove('rm');
    expect(localStorage.getItem('eng_rm')).toBeNull();
  });

  it('getString returns fallback when key not found', () => {
    expect(storage.getString('missing')).toBe('');
  });

  it('getString returns stored string value', () => {
    localStorage.setItem('eng_str', 'hello');
    expect(storage.getString('str')).toBe('hello');
  });

  it('setString stores string value with prefix', () => {
    storage.setString('s1', 'world');
    expect(localStorage.getItem('eng_s1')).toBe('world');
  });

  it('get returns fallback when parsed type does not match fallback type', () => {
    localStorage.setItem('eng_typed', JSON.stringify('a-string'));
    expect(storage.get('typed', 42)).toBe(42);
  });

  it('get returns parsed value when fallback is null (skips type check)', () => {
    localStorage.setItem('eng_anytype', JSON.stringify([1, 2, 3]));
    expect(storage.get<number[]>('anytype', null as any)).toEqual([1, 2, 3]);
  });

  it('admin token writes to sessionStorage primary and removes legacy localStorage entry', () => {
    localStorage.setItem('eng_' + STORAGE_KEYS.ADMIN_TOKEN, 'old-legacy');
    storage.setString(STORAGE_KEYS.ADMIN_TOKEN, 'new-tok');
    expect(sessionStorage.getItem('eng_' + STORAGE_KEYS.ADMIN_TOKEN)).toBe('new-tok');
    expect(localStorage.getItem('eng_' + STORAGE_KEYS.ADMIN_TOKEN)).toBeNull();
  });

  it('admin token read falls back to legacy localStorage and migrates value to sessionStorage', () => {
    localStorage.setItem('eng_' + STORAGE_KEYS.ADMIN_TOKEN, 'legacy-tok');
    const value = storage.getString(STORAGE_KEYS.ADMIN_TOKEN);
    expect(value).toBe('legacy-tok');
    // 迁移：legacy 应被删除，session 应有
    expect(sessionStorage.getItem('eng_' + STORAGE_KEYS.ADMIN_TOKEN)).toBe('legacy-tok');
    expect(localStorage.getItem('eng_' + STORAGE_KEYS.ADMIN_TOKEN)).toBeNull();
  });

  it('admin token remove clears both primary and legacy stores', () => {
    sessionStorage.setItem('eng_' + STORAGE_KEYS.ADMIN_TOKEN, 'cur');
    localStorage.setItem('eng_' + STORAGE_KEYS.ADMIN_TOKEN, 'leg');
    storage.remove(STORAGE_KEYS.ADMIN_TOKEN);
    expect(sessionStorage.getItem('eng_' + STORAGE_KEYS.ADMIN_TOKEN)).toBeNull();
    expect(localStorage.getItem('eng_' + STORAGE_KEYS.ADMIN_TOKEN)).toBeNull();
  });

  it('admin token write swallows error if sessionStorage migration setItem throws', () => {
    localStorage.setItem('eng_' + STORAGE_KEYS.ADMIN_TOKEN, 'legacy');
    const origSet = sessionStorage.setItem;
    sessionStorage.setItem = ((..._args: unknown[]) => {
      throw new Error('session-full');
    }) as typeof sessionStorage.setItem;
    try {
      // 读时尝试迁移失败但仍应返回 legacy 值
      expect(storage.getString(STORAGE_KEYS.ADMIN_TOKEN)).toBe('legacy');
    } finally {
      sessionStorage.setItem = origSet;
    }
  });

  it('write swallows errors when localStorage.setItem throws', () => {
    const origSet = localStorage.setItem;
    localStorage.setItem = ((..._args: unknown[]) => {
      throw new Error('quota');
    }) as typeof localStorage.setItem;
    try {
      expect(() => storage.set('throwy', { a: 1 })).not.toThrow();
    } finally {
      localStorage.setItem = origSet;
    }
  });

  it('remove swallows errors when storage throws', () => {
    const origRemove = localStorage.removeItem;
    localStorage.removeItem = ((..._args: unknown[]) => {
      throw new Error('boom');
    }) as typeof localStorage.removeItem;
    try {
      expect(() => storage.remove('any')).not.toThrow();
    } finally {
      localStorage.removeItem = origRemove;
    }
  });

  it('admin token remove swallows errors when sessionStorage throws', () => {
    const origRemove = sessionStorage.removeItem;
    sessionStorage.removeItem = ((..._args: unknown[]) => {
      throw new Error('boom');
    }) as typeof sessionStorage.removeItem;
    try {
      expect(() => storage.remove(STORAGE_KEYS.ADMIN_TOKEN)).not.toThrow();
    } finally {
      sessionStorage.removeItem = origRemove;
    }
  });

  it('admin token getString returns fallback when neither session nor legacy localStorage has value', () => {
    expect(storage.getString(STORAGE_KEYS.ADMIN_TOKEN)).toBe('');
  });

  it('readRaw returns null on outer try/catch when primary.getItem throws', () => {
    const origGet = localStorage.getItem;
    localStorage.getItem = ((..._args: unknown[]) => {
      throw new Error('boom');
    }) as typeof localStorage.getItem;
    try {
      expect(storage.get('broken', 'fallback')).toBe('fallback');
      expect(storage.getString('broken')).toBe('');
    } finally {
      localStorage.getItem = origGet;
    }
  });
});
