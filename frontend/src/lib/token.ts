import { storage, STORAGE_KEYS } from './storage';

/** Decode JWT payload without verification */
function decodeJwtPayload(token: string): Record<string, unknown> | null {
  try {
    const parts = token.split('.');
    if (parts.length !== 3) return null;
    const payload = atob(parts[1].replace(/-/g, '+').replace(/_/g, '/'));
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

export const tokenManager = {
  /** Get stored access token */
  getToken(): string | null {
    const token = storage.getString(STORAGE_KEYS.AUTH_TOKEN);
    if (!token) return null;
    if (isTokenExpired(token)) {
      this.clearTokens();
      return null;
    }
    return token;
  },

  /** Get stored refresh token */
  getRefreshToken(): string | null {
    return storage.getString(STORAGE_KEYS.REFRESH_TOKEN) || null;
  },

  /** Store tokens after login/register */
  setTokens(accessToken: string, refreshToken: string): void {
    storage.setString(STORAGE_KEYS.AUTH_TOKEN, accessToken);
    storage.setString(STORAGE_KEYS.REFRESH_TOKEN, refreshToken);
  },

  /** Clear all auth tokens */
  clearTokens(): void {
    storage.remove(STORAGE_KEYS.AUTH_TOKEN);
    storage.remove(STORAGE_KEYS.REFRESH_TOKEN);
    storage.remove(STORAGE_KEYS.USER);
  },

  /** Check if token needs refresh (expires within 5 minutes) */
  needsRefresh(): boolean {
    const token = storage.getString(STORAGE_KEYS.AUTH_TOKEN);
    if (!token) return false;
    return isTokenExpired(token, 300);
  },

  /** Check if user has a valid (non-expired) token */
  isAuthenticated(): boolean {
    return this.getToken() !== null;
  },

  // Admin token management
  getAdminToken(): string | null {
    const token = storage.getString(STORAGE_KEYS.ADMIN_TOKEN);
    if (!token) return null;
    if (isTokenExpired(token)) {
      storage.remove(STORAGE_KEYS.ADMIN_TOKEN);
      return null;
    }
    return token;
  },

  setAdminToken(token: string): void {
    storage.setString(STORAGE_KEYS.ADMIN_TOKEN, token);
  },

  clearAdminToken(): void {
    storage.remove(STORAGE_KEYS.ADMIN_TOKEN);
  },
};
