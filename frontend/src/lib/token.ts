import { storage, STORAGE_KEYS } from './storage';
import { TOKEN_REFRESH_BUFFER_SECS } from './constants';

/** Decode JWT payload without verification */
function decodeJwtPayload(token: string): Record<string, unknown> | null {
  try {
    const parts = token.split('.');
    if (parts.length !== 3) return null;
    const base64 = parts[1].replace(/-/g, '+').replace(/_/g, '/');
    const binary = atob(base64);
    const bytes = Uint8Array.from(binary, (c) => c.charCodeAt(0));
    const payload = new TextDecoder().decode(bytes);
    return JSON.parse(payload);
  } catch {
    return null;
  }
}

/** Check if a JWT is expired (with optional buffer in seconds) */
function isTokenExpired(token: string, bufferSec = 0): boolean {
  const payload = decodeJwtPayload(token);
  if (!payload || typeof payload.exp !== 'number') return true;
  return Date.now() / 1000 >= payload.exp - bufferSec;
}

let refreshPromise: Promise<boolean> | null = null;

// Access token 仅存储在内存中，不写入 sessionStorage/localStorage，
// 页面刷新后通过 HttpOnly cookie 中的 refresh token 重新获取。
let inMemoryAccessToken: string | null = null;
let inMemoryAdminToken: string | null = null;

export const tokenManager = {
  /** Get access token from memory */
  getToken(): string | null {
    if (!inMemoryAccessToken) return null;
    if (isTokenExpired(inMemoryAccessToken)) {
      this.clearTokens();
      return null;
    }
    return inMemoryAccessToken;
  },

  /** Store access token in memory after login/register/refresh */
  setTokens(accessToken: string): void {
    inMemoryAccessToken = accessToken;
    // Refresh token is managed by HttpOnly cookie, never persisted in JS storage.
    // Clean up any legacy storage entries.
    storage.remove(STORAGE_KEYS.AUTH_TOKEN);
    storage.remove(STORAGE_KEYS.REFRESH_TOKEN);
  },

  /** Clear all auth tokens */
  clearTokens(): void {
    inMemoryAccessToken = null;
    storage.remove(STORAGE_KEYS.AUTH_TOKEN);
    storage.remove(STORAGE_KEYS.REFRESH_TOKEN);
    storage.remove(STORAGE_KEYS.USER);
  },

  /** Refresh access token via HttpOnly cookie (single-flight) */
  async refreshAccessToken(): Promise<boolean> {
    if (refreshPromise) {
      return refreshPromise;
    }

    refreshPromise = (async () => {
      try {
        const { authApi } = await import('@/api/auth');
        const res = await authApi.refresh();
        tokenManager.setTokens(res.accessToken);
        return true;
      } catch {
        tokenManager.clearTokens();
        return false;
      } finally {
        refreshPromise = null;
      }
    })();

    return refreshPromise;
  },

  /** Check if token needs refresh (expires within 5 minutes) */
  needsRefresh(): boolean {
    if (!inMemoryAccessToken) return false;
    return isTokenExpired(inMemoryAccessToken, TOKEN_REFRESH_BUFFER_SECS);
  },

  /** Check if user has a valid (non-expired) token */
  isAuthenticated(): boolean {
    return this.getToken() !== null;
  },

  getAdminToken(): string | null {
    if (!inMemoryAdminToken) return null;
    if (isTokenExpired(inMemoryAdminToken)) {
      inMemoryAdminToken = null;
      return null;
    }
    return inMemoryAdminToken;
  },

  setAdminToken(token: string): void {
    inMemoryAdminToken = token;
  },

  clearAdminToken(): void {
    inMemoryAdminToken = null;
    storage.remove(STORAGE_KEYS.ADMIN_TOKEN);
  },

  isAdminTokenExpiringSoon(): boolean {
    if (!inMemoryAdminToken) return false;
    return isTokenExpired(inMemoryAdminToken, TOKEN_REFRESH_BUFFER_SECS);
  },
};
