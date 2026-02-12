import { describe, it, expect, vi, beforeEach, beforeAll, afterAll, afterEach } from 'vitest';
import { http, HttpResponse } from 'msw';
import { server } from '../helpers/msw-server';
import { createFakeUser, createFakeJwt } from '../helpers/factories';
import { STORAGE_KEYS } from '@/lib/storage';

const PREFIX = 'eng_';

beforeAll(() => server.listen({ onUnhandledRequest: 'error' }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

async function getFreshStore() {
  vi.resetModules();
  const mod = await import('@/stores/auth');
  return mod.authStore;
}

describe('authStore', () => {
  beforeEach(() => {
    localStorage.clear();
    sessionStorage.clear();
  });

  it('user defaults to null when no token', async () => {
    const store = await getFreshStore();
    expect(store.user()).toBeNull();
  });

  it('loading defaults to true', async () => {
    const store = await getFreshStore();
    expect(store.loading()).toBe(true);
  });

  it('isAuthenticated returns false by default', async () => {
    const store = await getFreshStore();
    expect(store.isAuthenticated()).toBe(false);
  });

  it('init sets loading to false when no token and refresh fails', async () => {
    // refreshAccessToken 会尝试 /api/auth/refresh，让它失败
    server.use(
      http.post('/api/auth/refresh', () =>
        HttpResponse.json({ success: false, code: 'NO_TOKEN', message: 'No refresh token' }, { status: 401 }),
      ),
    );

    const store = await getFreshStore();
    await store.init();
    expect(store.loading()).toBe(false);
    expect(store.user()).toBeNull();
  });

  it('init with successful refresh verifies via API', async () => {
    const fakeUser = createFakeUser();
    const newToken = createFakeJwt({ exp: Math.floor(Date.now() / 1000) + 3600 });

    server.use(
      http.post('/api/auth/refresh', () =>
        HttpResponse.json({
          success: true,
          data: { accessToken: newToken, refreshToken: 'new-refresh' },
        }),
      ),
      http.get('/api/users/me', () =>
        HttpResponse.json({ success: true, data: fakeUser }),
      ),
    );

    const store = await getFreshStore();
    await store.init();
    expect(store.loading()).toBe(false);
    expect(store.user()).toEqual(fakeUser);
  });

  it('login stores tokens and user on success', async () => {
    const store = await getFreshStore();
    const user = await store.login('test@example.com', 'password');
    expect(user).toBeDefined();
    expect(store.user()).toEqual(user);
    expect(store.isAuthenticated()).toBe(true);
    // token 存在内存中，不在 localStorage
  });

  it('login throws on invalid credentials', async () => {
    const store = await getFreshStore();
    await expect(store.login('fail@test.com', 'wrong')).rejects.toThrow();
  });

  it('register stores tokens and user', async () => {
    const store = await getFreshStore();
    const user = await store.register('new@test.com', 'newuser', 'password');
    expect(user).toBeDefined();
    expect(store.user()).toEqual(user);
    expect(store.isAuthenticated()).toBe(true);
  });

  it('logout clears tokens and learning data', async () => {
    const store = await getFreshStore();
    await store.login('test@example.com', 'password');
    localStorage.setItem(PREFIX + STORAGE_KEYS.LEARNING_MODE, JSON.stringify('meaning-to-word'));
    localStorage.setItem(PREFIX + STORAGE_KEYS.LEARNING_QUEUE, JSON.stringify([1, 2]));
    localStorage.setItem(PREFIX + STORAGE_KEYS.LEARNING_SESSION_ID, 'sess-1');

    await store.logout();
    expect(store.user()).toBeNull();
    expect(store.isAuthenticated()).toBe(false);
    expect(localStorage.getItem(PREFIX + STORAGE_KEYS.LEARNING_MODE)).toBeNull();
    expect(localStorage.getItem(PREFIX + STORAGE_KEYS.LEARNING_QUEUE)).toBeNull();
    expect(localStorage.getItem(PREFIX + STORAGE_KEYS.LEARNING_SESSION_ID)).toBeNull();
  });

  it('logout clears even if API fails', async () => {
    const store = await getFreshStore();
    await store.login('test@example.com', 'password');

    server.use(
      http.post('/api/auth/logout', () => HttpResponse.error()),
    );

    await store.logout();
    expect(store.user()).toBeNull();
  });

  it('updateUser updates user signal and storage', async () => {
    const store = await getFreshStore();
    await store.login('test@example.com', 'password');

    const updated = createFakeUser({ username: 'updated-name' });
    store.updateUser(updated);
    expect(store.user()).toEqual(updated);
    // user 存在 localStorage 中（仅保留 id 和 username）
    const storedUser = JSON.parse(localStorage.getItem(PREFIX + STORAGE_KEYS.USER)!);
    expect(storedUser.username).toBe('updated-name');
  });

  it('initialized prevents double init', async () => {
    const newToken = createFakeJwt({ exp: Math.floor(Date.now() / 1000) + 3600 });
    const fakeUser = createFakeUser();

    let callCount = 0;
    server.use(
      http.post('/api/auth/refresh', () =>
        HttpResponse.json({
          success: true,
          data: { accessToken: newToken, refreshToken: 'new-refresh' },
        }),
      ),
      http.get('/api/users/me', () => {
        callCount++;
        return HttpResponse.json({ success: true, data: fakeUser });
      }),
    );

    const store = await getFreshStore();
    await store.init();
    await store.init();
    expect(callCount).toBe(1);
  });
});
