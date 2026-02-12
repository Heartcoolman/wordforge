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

  it('getLearningStyle returns learning style with nested scores', async () => {
    const data = { style: 'visual', scores: { visual: 0.8, auditory: 0.2, reading: 0.5, kinesthetic: 0.3 } };
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
});
