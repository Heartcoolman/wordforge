import { describe, it, expect } from 'vitest';
import { storage } from '@/lib/storage';

describe('storage', () => {
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
});
