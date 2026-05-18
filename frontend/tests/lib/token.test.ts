import { describe, it, expect, beforeEach, vi } from 'vitest';
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
      tokenManager.setTokens(token);
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

    it('getAdminToken rehydrates from sessionStorage when memory empty', () => {
      const token = createFakeJwt({ exp: Math.floor(Date.now() / 1000) + 3600 });
      // 直接写入 sessionStorage 模拟刷新后场景；先清掉内存态
      tokenManager.clearAdminToken();
      sessionStorage.setItem('eng_admin_token', token);
      expect(tokenManager.getAdminToken()).toBe(token);
    });

    it('getAdminToken clears expired token and returns null', () => {
      const expired = createFakeJwt({ exp: Math.floor(Date.now() / 1000) - 100 });
      tokenManager.setAdminToken(expired);
      expect(tokenManager.getAdminToken()).toBeNull();
      expect(sessionStorage.getItem('eng_admin_token')).toBeNull();
    });

    it('isAdminTokenExpiringSoon returns false when no admin token', () => {
      expect(tokenManager.isAdminTokenExpiringSoon()).toBe(false);
    });

    it('isAdminTokenExpiringSoon returns true near expiry', () => {
      const token = createFakeJwt({ exp: Math.floor(Date.now() / 1000) + 60 });
      tokenManager.setAdminToken(token);
      expect(tokenManager.isAdminTokenExpiringSoon()).toBe(true);
    });

    it('isAdminTokenExpiringSoon returns false when token far from expiry', () => {
      const token = createFakeJwt({ exp: Math.floor(Date.now() / 1000) + 7200 });
      tokenManager.setAdminToken(token);
      expect(tokenManager.isAdminTokenExpiringSoon()).toBe(false);
    });
  });

  describe('refreshAccessToken', () => {
    it('returns true and stores token on successful refresh', async () => {
      const newToken = createFakeJwt({ exp: Math.floor(Date.now() / 1000) + 3600 });
      vi.doMock('@/api/auth', () => ({
        authApi: { refresh: vi.fn().mockResolvedValue({ accessToken: newToken }) },
      }));
      vi.resetModules();
      const { tokenManager: fresh } = await import('@/lib/token');
      fresh.clearTokens();
      const ok = await fresh.refreshAccessToken();
      expect(ok).toBe(true);
      expect(fresh.getToken()).toBe(newToken);
      vi.doUnmock('@/api/auth');
      vi.resetModules();
    });

    it('returns false and clears tokens on refresh failure', async () => {
      vi.doMock('@/api/auth', () => ({
        authApi: { refresh: vi.fn().mockRejectedValue(new Error('boom')) },
      }));
      vi.resetModules();
      const { tokenManager: fresh } = await import('@/lib/token');
      fresh.clearTokens();
      const ok = await fresh.refreshAccessToken();
      expect(ok).toBe(false);
      expect(fresh.getToken()).toBeNull();
      vi.doUnmock('@/api/auth');
      vi.resetModules();
    });

    it('coalesces concurrent refresh calls (single-flight)', async () => {
      const refreshSpy = vi.fn().mockImplementation(() =>
        new Promise((resolve) => setTimeout(() => resolve({ accessToken: createFakeJwt({ exp: Math.floor(Date.now() / 1000) + 3600 }) }), 10)));
      vi.doMock('@/api/auth', () => ({ authApi: { refresh: refreshSpy } }));
      vi.resetModules();
      const { tokenManager: fresh } = await import('@/lib/token');
      fresh.clearTokens();
      const [a, b] = await Promise.all([fresh.refreshAccessToken(), fresh.refreshAccessToken()]);
      expect(a).toBe(true);
      expect(b).toBe(true);
      expect(refreshSpy).toHaveBeenCalledTimes(1);
      vi.doUnmock('@/api/auth');
      vi.resetModules();
    });
  });

  describe('JWT payload parsing edge cases', () => {
    it('treats malformed token (less than 3 parts) as expired', () => {
      tokenManager.setTokens('not-a-jwt');
      expect(tokenManager.getToken()).toBeNull();
    });

    it('treats token with non-JSON payload as expired', () => {
      tokenManager.setTokens('aaa.bbb.ccc');
      expect(tokenManager.getToken()).toBeNull();
    });
  });
});
