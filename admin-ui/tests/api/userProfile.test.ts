import { describe, it, expect, vi, beforeAll, afterAll, afterEach } from 'vitest';
import { setupServer } from 'msw/node';
import { http, HttpResponse } from 'msw';

import { TEST_BASE_URL as BASE } from '../helpers/constants';

vi.mock('@/lib/token', () => ({
  tokenManager: {
    getToken: () => null,
    getAdminToken: () => null,
    setTokens: vi.fn(),
    clearTokens: vi.fn(),
    needsRefresh: () => false,
    isAuthenticated: () => false,
    setAdminToken: vi.fn(),
  },
}));
vi.mock('@/api/auth', () => ({ authApi: { refresh: vi.fn() } }));

import { userProfileApi } from '@/api/userProfile';

const server = setupServer();
beforeAll(() => server.listen({ onUnhandledRequest: 'bypass' }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

describe('userProfileApi', () => {
  it('getReward fetches reward preference', async () => {
    server.use(
      http.get(`${BASE}/api/user-profile/reward`, () =>
        HttpResponse.json({ success: true, data: { rewardType: 'explorer' } })),
    );
    const result = await userProfileApi.getReward();
    expect(result).toEqual({ rewardType: 'explorer' });
  });

  it('updateReward sends PUT with rewardType', async () => {
    server.use(
      http.put(`${BASE}/api/user-profile/reward`, async ({ request }) => {
        const body = await request.json() as Record<string, string>;
        return HttpResponse.json({ success: true, data: { rewardType: body.rewardType } });
      }),
    );
    const result = await userProfileApi.updateReward('achiever');
    expect(result).toEqual({ rewardType: 'achiever' });
  });

  it('getCognitive returns cognitive profile', async () => {
    const data = { memoryCapacity: 0.5, processingSpeed: 0.5, stability: 0.5 };
    server.use(
      http.get(`${BASE}/api/user-profile/cognitive`, () =>
        HttpResponse.json({ success: true, data })),
    );
    const result = await userProfileApi.getCognitive();
    expect(result).toEqual(data);
  });

  it('getLearningStyle returns cognitive-style summary', async () => {
    const data = { processingSpeed: 0.8, memoryCapacity: 0.6, stability: 0.7 };
    server.use(
      http.get(`${BASE}/api/user-profile/learning-style`, () =>
        HttpResponse.json({ success: true, data })),
    );
    const result = await userProfileApi.getLearningStyle();
    expect(result).toEqual(data);
  });

  it('getChronotype returns chronotype', async () => {
    const data = { chronotype: 'morning', preferredHours: [8, 9, 10] };
    server.use(
      http.get(`${BASE}/api/user-profile/chronotype`, () =>
        HttpResponse.json({ success: true, data })),
    );
    const result = await userProfileApi.getChronotype();
    expect(result).toEqual(data);
  });

  it('getHabit returns habit profile', async () => {
    const data = { preferredHours: [9, 14, 20], medianSessionLengthMins: 15, sessionsPerDay: 1 };
    server.use(
      http.get(`${BASE}/api/user-profile/habit`, () =>
        HttpResponse.json({ success: true, data })),
    );
    const result = await userProfileApi.getHabit();
    expect(result).toEqual(data);
  });

  it('updateHabit sends POST with partial data', async () => {
    const updated = { preferredHours: [18, 19, 20], medianSessionLengthMins: 30, sessionsPerDay: 2 };
    server.use(
      http.post(`${BASE}/api/user-profile/habit`, () =>
        HttpResponse.json({ success: true, data: updated })),
    );
    const result = await userProfileApi.updateHabit({ preferredHours: [18, 19, 20] });
    expect(result).toEqual(updated);
  });

  it('updateHabit rounds preferredHours and medianSessionLengthMins', async () => {
    let body: Record<string, unknown> = {};
    server.use(
      http.post(`${BASE}/api/user-profile/habit`, async ({ request }) => {
        body = await request.json() as Record<string, unknown>;
        return HttpResponse.json({ success: true, data: {} });
      }),
    );
    await userProfileApi.updateHabit({ preferredHours: [8.6, 14.4], medianSessionLengthMins: 22.7 } as any);
    expect(body).toEqual({ preferredHours: [9, 14], medianSessionLengthMins: 23 });
  });

  it('uploadAvatar reads file ArrayBuffer and POSTs with provided Content-Type', async () => {
    let contentType = '';
    let bodyLen = 0;
    server.use(
      http.post(`${BASE}/api/user-profile/avatar`, async ({ request }) => {
        contentType = request.headers.get('content-type') ?? '';
        bodyLen = (await request.arrayBuffer()).byteLength;
        return HttpResponse.json({ success: true, data: { avatarUrl: 'https://cdn.example/avatar.png' } });
      }),
    );
    const file = new File([new Uint8Array([1, 2, 3, 4, 5])], 'avatar.png', { type: 'image/png' });
    const result = await userProfileApi.uploadAvatar(file);
    expect(result).toEqual({ avatarUrl: 'https://cdn.example/avatar.png' });
    expect(contentType).toBe('image/png');
    expect(bodyLen).toBe(5);
  });

  it('uploadAvatar falls back to octet-stream when file has no type', async () => {
    let contentType = '';
    server.use(
      http.post(`${BASE}/api/user-profile/avatar`, async ({ request }) => {
        contentType = request.headers.get('content-type') ?? '';
        return HttpResponse.json({ success: true, data: { avatarUrl: 'x' } });
      }),
    );
    const file = new File([new Uint8Array([0])], 'blob', { type: '' });
    await userProfileApi.uploadAvatar(file);
    expect(contentType).toBe('application/octet-stream');
  });
});
