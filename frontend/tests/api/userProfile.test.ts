import { describe, it, expect, vi, beforeAll, afterAll, afterEach } from 'vitest';
import { setupServer } from 'msw/node';
import { http, HttpResponse } from 'msw';

const BASE = 'http://localhost:3000';

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
        HttpResponse.json({ success: true, data: { preference: 'explorer' } })),
    );
    const result = await userProfileApi.getReward();
    expect(result).toEqual({ preference: 'explorer' });
  });

  it('updateReward sends PUT with preference', async () => {
    server.use(
      http.put(`${BASE}/api/user-profile/reward`, async ({ request }) => {
        const body = await request.json() as Record<string, string>;
        return HttpResponse.json({ success: true, data: { preference: body.preference } });
      }),
    );
    const result = await userProfileApi.updateReward('achiever');
    expect(result).toEqual({ preference: 'achiever' });
  });

  it('getCognitive returns cognitive profile', async () => {
    const data = { attention: 0.8, fatigue: 0.3, motivation: 0.9, confidence: 0.7 };
    server.use(
      http.get(`${BASE}/api/user-profile/cognitive`, () =>
        HttpResponse.json({ success: true, data })),
    );
    const result = await userProfileApi.getCognitive();
    expect(result).toEqual(data);
  });

  it('getLearningStyle returns learning style', async () => {
    const data = { style: 'visual', visual: 0.8, auditory: 0.2, reading: 0.5, kinesthetic: 0.3 };
    server.use(
      http.get(`${BASE}/api/user-profile/learning-style`, () =>
        HttpResponse.json({ success: true, data })),
    );
    const result = await userProfileApi.getLearningStyle();
    expect(result).toEqual(data);
  });

  it('getChronotype returns chronotype', async () => {
    const data = { type: 'morning', preferredHours: [8, 9, 10] };
    server.use(
      http.get(`${BASE}/api/user-profile/chronotype`, () =>
        HttpResponse.json({ success: true, data })),
    );
    const result = await userProfileApi.getChronotype();
    expect(result).toEqual(data);
  });

  it('getHabit returns habit profile', async () => {
    const data = { preferredTimeSlot: 'morning', medianSessionMinutes: 30, dailyFrequency: 2 };
    server.use(
      http.get(`${BASE}/api/user-profile/habit`, () =>
        HttpResponse.json({ success: true, data })),
    );
    const result = await userProfileApi.getHabit();
    expect(result).toEqual(data);
  });

  it('updateHabit sends POST with partial data', async () => {
    const updated = { preferredTimeSlot: 'evening', medianSessionMinutes: 45, dailyFrequency: 3 };
    server.use(
      http.post(`${BASE}/api/user-profile/habit`, () =>
        HttpResponse.json({ success: true, data: updated })),
    );
    const result = await userProfileApi.updateHabit({ preferredTimeSlot: 'evening' });
    expect(result).toEqual(updated);
  });
});
