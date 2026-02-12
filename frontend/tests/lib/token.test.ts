import { describe, it, expect, beforeEach } from 'vitest';
import { createFakeJwt } from '../helpers/factories';

// tokenManager 是模块级单例，使用内存存储 access token。
// 每次测试前需要 clearTokens 来重置内存状态。
import { tokenManager } from '@/lib/token';

beforeEach(() => {
  tokenManager.clearTokens();
  tokenManager.clearAdminToken();
  localStorage.clear();
  sessionStorage.clear();
});

describe('tokenManager', () => {
  describe('getToken', () => {
    it('returns null when no token stored', () => {
      expect(tokenManager.getToken()).toBeNull();
    });

    it('returns token when valid (not expired)', () => {
      const token = createFakeJwt({ exp: Math.floor(Date.now() / 1000) + 3600 });
      tokenManager.setTokens(token);
      expect(tokenManager.getToken()).toBe(token);
    });

    it('returns null and clears when expired', () => {
      const token = createFakeJwt({ exp: Math.floor(Date.now() / 1000) - 100 });
      tokenManager.setTokens(token);
      expect(tokenManager.getToken()).toBeNull();
    });
  });

  describe('setTokens', () => {
    it('stores access token in memory', () => {
      const token = createFakeJwt({ exp: Math.floor(Date.now() / 1000) + 3600 });
      tokenManager.setTokens(token, 'refresh-1');
      expect(tokenManager.getToken()).toBe(token);
      // Access token 不应写入 localStorage
      expect(localStorage.getItem('eng_auth_token')).toBeNull();
    });
  });

  describe('clearTokens', () => {
    it('clears in-memory token and removes legacy storage keys', () => {
      const token = createFakeJwt({ exp: Math.floor(Date.now() / 1000) + 3600 });
      tokenManager.setTokens(token);
      // 添加 legacy storage 条目
      localStorage.setItem('eng_auth_token', 'legacy');
      localStorage.setItem('eng_refresh_token', 'legacy');
      localStorage.setItem('eng_user', '"u"');

      tokenManager.clearTokens();
      expect(tokenManager.getToken()).toBeNull();
      expect(localStorage.getItem('eng_auth_token')).toBeNull();
      expect(localStorage.getItem('eng_refresh_token')).toBeNull();
      expect(localStorage.getItem('eng_user')).toBeNull();
    });
  });

  describe('needsRefresh', () => {
    it('returns false when no token', () => {
      expect(tokenManager.needsRefresh()).toBe(false);
    });

    it('returns false when token fresh (exp far in future)', () => {
      const token = createFakeJwt({ exp: Math.floor(Date.now() / 1000) + 7200 });
      tokenManager.setTokens(token);
      expect(tokenManager.needsRefresh()).toBe(false);
    });

    it('returns true when token expires within 300s', () => {
      const token = createFakeJwt({ exp: Math.floor(Date.now() / 1000) + 100 });
      tokenManager.setTokens(token);
      expect(tokenManager.needsRefresh()).toBe(true);
    });
  });

  describe('isAuthenticated', () => {
    it('returns false when no token', () => {
      expect(tokenManager.isAuthenticated()).toBe(false);
    });

    it('returns true when valid token', () => {
      const token = createFakeJwt({ exp: Math.floor(Date.now() / 1000) + 3600 });
      tokenManager.setTokens(token);
      expect(tokenManager.isAuthenticated()).toBe(true);
    });
  });

  describe('admin token methods', () => {
    it('getAdminToken returns null when no token', () => {
      expect(tokenManager.getAdminToken()).toBeNull();
    });

    it('setAdminToken and getAdminToken roundtrip', () => {
      const token = createFakeJwt({ exp: Math.floor(Date.now() / 1000) + 3600 });
      tokenManager.setAdminToken(token);
      expect(tokenManager.getAdminToken()).toBe(token);
    });

    it('clearAdminToken removes admin token', () => {
      const token = createFakeJwt({ exp: Math.floor(Date.now() / 1000) + 3600 });
      tokenManager.setAdminToken(token);
      tokenManager.clearAdminToken();
      expect(tokenManager.getAdminToken()).toBeNull();
    });
  });
});
