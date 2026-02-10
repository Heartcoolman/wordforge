import { describe, it, expect } from 'vitest';
import { cn } from '@/utils/cn';

describe('cn', () => {
  it('returns empty string for no args', () => {
    expect(cn()).toBe('');
  });

  it('merges multiple class strings', () => {
    expect(cn('px-2', 'py-1', 'bg-red-500')).toBe('px-2 py-1 bg-red-500');
  });

  it('handles conditional classes (false, null, undefined)', () => {
    expect(cn('base', false && 'hidden', null, undefined)).toBe('base');
  });

  it('resolves Tailwind conflicts', () => {
    expect(cn('p-2', 'p-4')).toBe('p-4');
  });

  it('handles array inputs', () => {
    expect(cn(['px-2', 'py-1'], 'bg-red-500')).toBe('px-2 py-1 bg-red-500');
  });
});
