import { createSignal, createRoot } from 'solid-js';
import type { User } from '@/types/user';
import { storage, STORAGE_KEYS } from '@/lib/storage';
import { tokenManager } from '@/lib/token';
import { authApi } from '@/api/auth';
import { resetUnauthorized } from '@/api/client';

function createAuthStore() {
  // Optimistic load from localStorage
  const cachedUser = storage.get<User | null>(STORAGE_KEYS.USER, null);
  const hasToken = tokenManager.isAuthenticated();

  const [user, setUser] = createSignal<User | null>(hasToken ? cachedUser : null);
  const [loading, setLoading] = createSignal(true);
  const [initialized, setInitialized] = createSignal(false);

  const isAuthenticated = () => user() !== null;

  /** Initialize auth state - call once on app startup */
  async function init() {
    if (initialized()) return;
    setInitialized(true);

    if (!tokenManager.isAuthenticated()) {
      setLoading(false);
      return;
    }

    // Silently verify token
    try {
      if (tokenManager.needsRefresh()) {
        const refreshToken = tokenManager.getRefreshToken();
        if (refreshToken) {
          const res = await authApi.refresh();
          tokenManager.setTokens(res.accessToken, res.refreshToken);
          setUser(res.user);
          storage.set(STORAGE_KEYS.USER, res.user);
        } else {
          throw new Error('No refresh token');
        }
      } else {
        // Verify current token by fetching user profile
        const { usersApi } = await import('@/api/users');
        const profile = await usersApi.getMe();
        setUser(profile);
        storage.set(STORAGE_KEYS.USER, profile);
      }
    } catch {
      // Token invalid, clear everything
      tokenManager.clearTokens();
      setUser(null);
    } finally {
      setLoading(false);
    }
  }

  async function login(email: string, password: string) {
    const res = await authApi.login({ email, password });
    tokenManager.setTokens(res.accessToken, res.refreshToken);
    setUser(res.user);
    storage.set(STORAGE_KEYS.USER, res.user);
    resetUnauthorized();
    return res.user;
  }

  async function register(email: string, username: string, password: string) {
    const res = await authApi.register({ email, username, password });
    tokenManager.setTokens(res.accessToken, res.refreshToken);
    setUser(res.user);
    storage.set(STORAGE_KEYS.USER, res.user);
    resetUnauthorized();
    return res.user;
  }

  async function logout() {
    try {
      await authApi.logout();
    } catch {
      // Ignore logout API errors
    } finally {
      tokenManager.clearTokens();
      setUser(null);
      // Clear learning data to prevent leaking to next user
      storage.remove(STORAGE_KEYS.LEARNING_MODE);
      storage.remove(STORAGE_KEYS.LEARNING_QUEUE);
      storage.remove(STORAGE_KEYS.LEARNING_SESSION_ID);
    }
  }

  function updateUser(updated: User) {
    setUser(updated);
    storage.set(STORAGE_KEYS.USER, updated);
  }

  return {
    user,
    loading,
    isAuthenticated,
    initialized,
    init,
    login,
    register,
    logout,
    updateUser,
  };
}

export const authStore = createRoot(createAuthStore);
