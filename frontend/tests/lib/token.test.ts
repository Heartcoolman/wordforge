import { describe, it, expect } from 'vitest';
import { tokenManager } from '@/lib/token';
import { createFakeJwt } from '../helpers/factories';

describe('tokenManager', () => {
  describe('getToken', () => {
    it('returns null when no token stored', () => {
      expect(tokenManager.getToken()).toBeNull();
    });

    it('returns token when valid (not expired)', () => {
      const token = createFakeJwt({ exp: Math.floor(Date.now() / 1000) + 3600 });
      localStorage.setItem('eng_auth_token', token);
      expect(tokenManager.getToken()).toBe(token);
    });

    it('returns null and clears when expired', () => {
      const token = createFakeJwt({ exp: Math.floor(Date.now() / 1000) - 100 });
      localStorage.setItem('eng_auth_token', token);
      expect(tokenManager.getToken()).toBeNull();
      expect(localStorage.getItem('eng_auth_token')).toBeNull();
    });
  });

  describe('getRefreshToken', () => {
    it('returns null when no token', () => {
      expect(tokenManager.getRefreshToken()).toBeNull();
    });

    it('returns stored token', () => {
      localStorage.setItem('eng_refresh_token', 'rt-123');
      expect(tokenManager.getRefreshToken()).toBe('rt-123');
    });
  });

  describe('setTokens', () => {
    it('stores both tokens', () => {
      tokenManager.setTokens('access-1', 'refresh-1');
      expect(localStorage.getItem('eng_auth_token')).toBe('access-1');
      expect(localStorage.getItem('eng_refresh_token')).toBe('refresh-1');
    });
  });

  describe('clearTokens', () => {
    it('removes all auth keys', () => {
      localStorage.setItem('eng_auth_token', 'a');
      localStorage.setItem('eng_refresh_token', 'b');
      localStorage.setItem('eng_user', '"u"');
      tokenManager.clearTokens();
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
      localStorage.setItem('eng_auth_token', token);
      expect(tokenManager.needsRefresh()).toBe(false);
    });

    it('returns true when token expires within 300s', () => {
      const token = createFakeJwt({ exp: Math.floor(Date.now() / 1000) + 100 });
      localStorage.setItem('eng_auth_token', token);
      expect(tokenManager.needsRefresh()).toBe(true);
    });
  });

  describe('isAuthenticated', () => {
    it('returns false when no token', () => {
      expect(tokenManager.isAuthenticated()).toBe(false);
    });

    it('returns true when valid token', () => {
      const token = createFakeJwt({ exp: Math.floor(Date.now() / 1000) + 3600 });
      localStorage.setItem('eng_auth_token', token);
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
