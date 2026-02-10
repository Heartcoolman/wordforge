import { describe, it, expect } from 'vitest';
import { QueryClient } from '@tanstack/solid-query';
import { queryClient } from '@/lib/queryClient';

describe('queryClient', () => {
  it('exports a QueryClient instance', () => {
    expect(queryClient).toBeInstanceOf(QueryClient);
  });

  it('default staleTime is 2 minutes', () => {
    const opts = queryClient.getDefaultOptions();
    expect(opts.queries?.staleTime).toBe(120_000);
  });

  it('default gcTime is 10 minutes', () => {
    const opts = queryClient.getDefaultOptions();
    expect(opts.queries?.gcTime).toBe(600_000);
  });

  it('default retry for queries is 1', () => {
    const opts = queryClient.getDefaultOptions();
    expect(opts.queries?.retry).toBe(1);
  });

  it('default retry for mutations is 0', () => {
    const opts = queryClient.getDefaultOptions();
    expect(opts.mutations?.retry).toBe(0);
  });

  it('refetchOnWindowFocus is false', () => {
    const opts = queryClient.getDefaultOptions();
    expect(opts.queries?.refetchOnWindowFocus).toBe(false);
  });
});
