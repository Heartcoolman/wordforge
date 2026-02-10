import { http, HttpResponse } from 'msw';
import { createFakeUser, createFakeJwt } from './factories';

const fakeUser = createFakeUser();
const fakeTokens = {
  accessToken: createFakeJwt({ exp: Math.floor(Date.now() / 1000) + 3600 }),
  refreshToken: createFakeJwt({ exp: Math.floor(Date.now() / 1000) + 86400 }),
};

export const defaultHandlers = [
  http.post('/api/auth/login', async ({ request }) => {
    const body = await request.json() as Record<string, string>;
    if (body.email === 'fail@test.com') {
      return HttpResponse.json(
        { success: false, code: 'INVALID_CREDENTIALS', message: '邮箱或密码错误' },
        { status: 401 },
      );
    }
    return HttpResponse.json({
      success: true,
      data: { ...fakeTokens, user: fakeUser },
    });
  }),

  http.post('/api/auth/register', () => {
    return HttpResponse.json({
      success: true,
      data: { ...fakeTokens, user: fakeUser },
    });
  }),

  http.post('/api/auth/refresh', () => {
    return HttpResponse.json({
      success: true,
      data: {
        accessToken: createFakeJwt({ exp: Math.floor(Date.now() / 1000) + 3600 }),
        refreshToken: createFakeJwt({ exp: Math.floor(Date.now() / 1000) + 86400 }),
        user: fakeUser,
      },
    });
  }),

  http.post('/api/auth/logout', () => {
    return HttpResponse.json({ success: true, data: { loggedOut: true } });
  }),

  http.get('/api/users/me', () => {
    return HttpResponse.json({ success: true, data: fakeUser });
  }),

  http.get('/api/users/me/stats', () => {
    return HttpResponse.json({
      success: true,
      data: {
        totalWordsLearned: 150,
        totalSessions: 30,
        totalRecords: 500,
        streakDays: 7,
        accuracyRate: 0.85,
      },
    });
  }),
];
