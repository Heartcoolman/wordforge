import { createSignal, createRoot } from 'solid-js';
import type { User } from '@/types/user';
import { storage, STORAGE_KEYS } from '@/lib/storage';
import { tokenManager } from '@/lib/token';
import { authApi } from '@/api/auth';
import { resetUnauthorized } from '@/api/client';

function logAuthWarning(message: string, err: unknown): void {
  if (import.meta.env.DEV) {
    console.warn(message, err);
    return;
  }

  if (err instanceof Error) {
    console.warn(message, err.message);
    return;
  }

  console.warn(message);
}

function createAuthStore() {
  // Optimistic load from storage
  const cachedUser = storage.get<User | null>(STORAGE_KEYS.USER, null);
  const hasToken = tokenManager.isAuthenticated();

  const [user, setUser] = createSignal<User | null>(hasToken ? cachedUser : null);
  const [loading, setLoading] = createSignal(true);
  let initCalled = false;

  const isAuthenticated = () => user() !== null;

  /** 存储到 localStorage 时排除敏感字段，仅保留必要信息 */
  function safeUserForStorage(u: User) {
    return { id: u.id, username: u.username };
  }

  /** Initialize auth state - call once on app startup */
  async function init() {
    if (initCalled) return;
    initCalled = true;

    // Access token 存储在内存中，页面刷新后丢失。
    // 先检查内存中的 token，如果没有则尝试通过 HttpOnly cookie 中的 refresh token 恢复。
    if (!tokenManager.isAuthenticated()) {
      // 尝试通过 refresh token 恢复会话
      const refreshed = await tokenManager.refreshAccessToken();
      if (!refreshed) {
        setLoading(false);
        return;
      }
    }

    // 通过获取用户信息验证 token 有效性
    try {
      const { usersApi } = await import('@/api/users');
      const profile = await usersApi.getMe();
      setUser(profile);
      storage.set(STORAGE_KEYS.USER, safeUserForStorage(profile));
    } catch (err: unknown) {
      // Distinguish network errors from auth errors
      const isNetworkError =
        (err instanceof TypeError && /fetch/i.test(err.message)) ||
        (err instanceof Error && 'status' in err && ((err as { status: number }).status === 0 || (err as { status: number }).status >= 500));

      if (isNetworkError) {
        // Network/server error: keep current cached state, don't log out
        logAuthWarning('[auth] 网络错误，保持当前状态', err);
      } else {
        // Auth error (401/403/invalid token): clear everything
        tokenManager.clearTokens();
        setUser(null);
      }
    } finally {
      setLoading(false);
    }
  }

  async function login(email: string, password: string) {
    const res = await authApi.login({ email, password });
    tokenManager.setTokens(res.accessToken, res.refreshToken);
    setUser(res.user);
    storage.set(STORAGE_KEYS.USER, safeUserForStorage(res.user));
    resetUnauthorized();
    return res.user;
  }

  async function register(email: string, username: string, password: string) {
    const res = await authApi.register({ email, username, password });
    tokenManager.setTokens(res.accessToken, res.refreshToken);
    setUser(res.user);
    storage.set(STORAGE_KEYS.USER, safeUserForStorage(res.user));
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
      // Clear all user-specific data to prevent leaking to next user
      storage.remove(STORAGE_KEYS.USER);
      storage.remove(STORAGE_KEYS.LEARNING_MODE);
      storage.remove(STORAGE_KEYS.LEARNING_QUEUE);
      storage.remove(STORAGE_KEYS.LEARNING_SESSION_ID);
      storage.remove(STORAGE_KEYS.FATIGUE_ENABLED);
    }
  }

  function updateUser(updated: User) {
    setUser(updated);
    storage.set(STORAGE_KEYS.USER, safeUserForStorage(updated));
  }

  return {
    user,
    loading,
    isAuthenticated,
    get initialized() { return initCalled; },
    init,
    login,
    register,
    logout,
    updateUser,
  };
}

export const authStore = createRoot(createAuthStore);
