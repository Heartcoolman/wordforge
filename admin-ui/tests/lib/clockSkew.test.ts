import { describe, it, expect, beforeEach } from 'vitest';
import {
  nowSecs,
  updateSkewFromServerTime,
  ingestServerTimeFromResponse,
  getSkewSecs,
  _resetClockSkewForTests,
} from '@/lib/clockSkew';

beforeEach(() => {
  _resetClockSkewForTests();
});

describe('clockSkew', () => {
  describe('nowSecs', () => {
    it('returns client now when no skew set', () => {
      const before = Math.floor(Date.now() / 1000);
      const got = nowSecs();
      const after = Math.floor(Date.now() / 1000);
      expect(got).toBeGreaterThanOrEqual(before);
      expect(got).toBeLessThanOrEqual(after);
    });

    it('applies positive skew when server ahead', () => {
      const clientNow = Math.floor(Date.now() / 1000);
      updateSkewFromServerTime(clientNow + 1000);
      expect(nowSecs()).toBeGreaterThanOrEqual(clientNow + 999);
    });

    it('applies negative skew when server behind', () => {
      const clientNow = Math.floor(Date.now() / 1000);
      updateSkewFromServerTime(clientNow - 5000);
      expect(nowSecs()).toBeLessThanOrEqual(clientNow - 4999);
    });
  });

  describe('updateSkewFromServerTime', () => {
    it('ignores non-finite values', () => {
      updateSkewFromServerTime(Number.NaN);
      expect(getSkewSecs()).toBe(0);
      updateSkewFromServerTime(Number.POSITIVE_INFINITY);
      expect(getSkewSecs()).toBe(0);
    });

    it('ignores suspiciously small unix timestamps', () => {
      updateSkewFromServerTime(100); // pre-2024 timestamp = garbage
      expect(getSkewSecs()).toBe(0);
    });

    it('persists to localStorage', () => {
      const clientNow = Math.floor(Date.now() / 1000);
      updateSkewFromServerTime(clientNow + 60);
      const raw = localStorage.getItem('eng_clock_skew');
      expect(raw).not.toBeNull();
      const parsed = JSON.parse(raw!);
      expect(parsed.skew).toBeGreaterThanOrEqual(59);
      expect(parsed.skew).toBeLessThanOrEqual(61);
    });
  });

  describe('ingestServerTimeFromResponse', () => {
    function fakeResponse(headers: Record<string, string>): Response {
      return new Response(null, { headers });
    }

    it('reads X-Server-Time header and updates skew', () => {
      const clientNow = Math.floor(Date.now() / 1000);
      ingestServerTimeFromResponse(fakeResponse({ 'X-Server-Time': String(clientNow + 120) }));
      expect(getSkewSecs()).toBeGreaterThanOrEqual(119);
    });

    it('is case-insensitive on header name', () => {
      const clientNow = Math.floor(Date.now() / 1000);
      ingestServerTimeFromResponse(fakeResponse({ 'x-server-time': String(clientNow + 30) }));
      expect(getSkewSecs()).toBeGreaterThanOrEqual(29);
    });

    it('no-ops on missing header', () => {
      ingestServerTimeFromResponse(fakeResponse({}));
      expect(getSkewSecs()).toBe(0);
    });

    it('no-ops on non-numeric header', () => {
      ingestServerTimeFromResponse(fakeResponse({ 'X-Server-Time': 'banana' }));
      expect(getSkewSecs()).toBe(0);
    });
  });
});
