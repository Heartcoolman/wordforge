import { expect, type Page, test } from '@playwright/test';

type MockState = {
  calls: Array<{ method: string; path: string; body?: unknown }>;
  settings: Record<string, unknown>;
};

const ok = (data: unknown) => ({ success: true, data });

const baseConfig = {
  featureFlags: {
    ensembleEnabled: true,
    heuristicEnabled: true,
    igeEnabled: true,
    swdEnabled: true,
    mdmEnabled: true,
    iadEnabled: false,
    mtpEnabled: false,
    sspEnabled: false,
  },
  ensemble: {
    baseWeightHeuristic: 0.4,
    baseWeightIge: 0.3,
    baseWeightSwd: 0.3,
    warmupSamples: 20,
    blendScale: 100,
    blendMax: 0.5,
    minWeight: 0.15,
    warmupHeuristicBoost: 0.2,
  },
  modeling: {
    attentionSmoothing: 0.3,
    confidenceDecay: 0.99,
    minConfidence: 0.1,
    fatigueIncreaseRate: 0.02,
    fatigueRecoveryRate: 0.001,
    motivationMomentum: 0.1,
    visualFatigueWeight: 0.4,
  },
  constraints: {
    highFatigueThreshold: 0.9,
    lowAttentionThreshold: 0.3,
    lowMotivationThreshold: -0.5,
    maxBatchSizeWhenFatigued: 5,
    maxNewRatioWhenFatigued: 0.2,
    maxDifficultyWhenFatigued: 0.55,
    minDifficulty: 0.1,
  },
  objectiveWeights: {
    retention: 0.35,
    accuracy: 0.25,
    speed: 0.15,
    fatigue: 0.15,
    frustration: 0.1,
  },
  memoryModel: {
    w: [
      0.4072, 1.1829, 3.1262, 15.4722, 7.1949, 0.5345, 1.4604, 0.0046, 1.54575,
      0.1192, 1.01925, 1.9395, 0.11, 0.29605, 2.2698, 0.2315, 2.9898, 0.51655,
      0.6621,
    ],
    baseDesiredRetention: 0.92,
    maxIntervalDays: 90,
    alphaScale: 0.3,
    alphaMin: 0.1,
    alphaMax: 0.5,
    retentionMin: 0.7,
    retentionMax: 0.95,
  },
  monitoring: { sampleRate: 0.05, metricsFlushIntervalSecs: 300 },
  coldStart: {
    classifyToExploreEvents: 20,
    classifyToExploreConfidence: 0.6,
    exploreToExploitEvents: 80,
  },
};

function jwt() {
  const payload = btoa(JSON.stringify({ exp: Math.floor(Date.now() / 1000) + 3600 }));
  return `e2e.${payload}.token`;
}

async function mockAdminApi(page: Page, overrides: { wordbookUrl?: string } = {}): Promise<MockState> {
  const state: MockState = {
    calls: [],
    settings: {
      maxUsers: 1000,
      registrationEnabled: true,
      maintenanceMode: false,
      defaultDailyWords: 20,
      wordbookCenterUrl: overrides.wordbookUrl ?? 'https://wordbooks.example.test',
      amasAutoApplyEnabled: false,
      amasAutoApplyMaxPerDay: 2,
      amasAutoApplyMinConfidence: 0.8,
    },
  };

  await page.route('**/api/realtime/events', (route) =>
    route.fulfill({ status: 200, contentType: 'text/event-stream', body: '' }),
  );

  await page.route('**/*', async (route) => {
    const request = route.request();
    const url = new URL(request.url());
    const path = url.pathname;
    const method = request.method();

    if (!path.startsWith('/api/') && !path.startsWith('/health')) {
      return route.fallback();
    }

    let body: unknown;
    if (method !== 'GET') {
      try {
        body = request.postDataJSON();
      } catch {
        body = request.postData();
      }
    }
    state.calls.push({ method, path, body });

    const json = (data: unknown, status = 200) =>
      route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(data) });

    if (path === '/api/status') {
      return json(ok({ maintenanceMode: false, version: '0.4.3' }));
    }
    if (path === '/health') {
      return json({
        status: 'ok',
        uptimeSecs: 7200,
        services: {
          store: { healthy: true },
          amas: { healthy: true },
          sse: { healthy: true },
          wordbookCenter: { healthy: true, url: state.settings.wordbookCenterUrl },
        },
      });
    }
    if (path === '/api/admin/auth/status') {
      return json(ok({ initialized: true }));
    }
    if (path === '/api/admin/auth/verify') {
      return json(ok({ id: 'admin-1', email: 'admin@example.com' }));
    }
    if (path === '/api/admin/auth/logout') {
      return json(ok({ loggedOut: true }));
    }

    if (path === '/api/admin/stats') {
      return json(ok({
        users: 42,
        words: 1200,
        records: 3400,
        trend: { users: { value: 12, label: '较昨日' }, records: { value: 8, label: '较昨日' } },
      }));
    }
    if (path === '/api/admin/analytics/engagement') {
      return json(ok({ totalUsers: 42, activeToday: 11, retentionRate: 0.74, trend: { activeToday: { value: 22, label: '较昨日' } } }));
    }
    if (path === '/api/admin/analytics/learning') {
      return json(ok({ totalWords: 1200, totalRecords: 3400, totalCorrect: 2600, overallAccuracy: 0.76, trend: { totalRecords: { value: 10, label: '较昨日' }, overallAccuracy: { value: 3, label: '较昨日' } } }));
    }
    if (path === '/api/admin/analytics/study-overview') {
      return json(ok({
        generatedAt: '2026-05-19T00:00:00Z',
        days: Number(url.searchParams.get('days') ?? 7),
        category: 'all',
        summary: { totalDurationSecs: 7200, sessionCount: 12, recordCount: 180, correctCount: 150, accuracy: 0.83, newWords: 32, reviewWords: 148, masteredWords: 9 },
        daily: [{ date: '2026-05-19', durationSecs: 1200, sessionCount: 2, recordCount: 30, correctCount: 25, accuracy: 0.83, newWords: 5, reviewWords: 25, masteredWords: 1 }],
      }));
    }
    if (path === '/api/admin/analytics/daily-active-users') {
      return json(ok([
        { date: '2026-05-18', count: 9, registered: 2 },
        { date: '2026-05-19', count: 11, registered: 3 },
      ]));
    }
    if (path === '/api/admin/analytics/daily-records') {
      return json(ok([
        { date: '2026-05-18', correct: 18, total: 24, durationSecs: 800, newWords: 4 },
        { date: '2026-05-19', correct: 25, total: 30, durationSecs: 1200, newWords: 5 },
      ]));
    }
    if (path === '/api/admin/analytics/record-types') {
      return json(ok({
        generatedAt: '2026-05-19T00:00:00Z',
        days: 7,
        totals: [],
        daily: [{ date: '2026-05-19', learning: 12, review: 18, all: 0 }],
      }));
    }
    if (path === '/api/admin/analytics/retention-curve') {
      return json(ok({
        generatedAt: '2026-05-19T00:00:00Z',
        category: 'all',
        averageRetention: 0.82,
        points: [{ daysSinceLearn: 1, retention: 0.92, sampleSize: 20 }, { daysSinceLearn: 7, retention: 0.74, sampleSize: 12 }],
      }));
    }
    if (path === '/api/admin/analytics/word-states') {
      return json(ok({
        generatedAt: '2026-05-19T00:00:00Z',
        category: 'all',
        states: { newCount: 20, learning: 30, reviewing: 40, mastered: 50, forgotten: 2 },
        totals: { trackedWords: 142, bookmarkedWords: 18, dueReviewWords: 22, overdueReviewWords: 5, averageMasteryLevel: 0.66 },
      }));
    }

    if (path === '/api/admin/users' && method === 'GET') {
      return json(ok({
        data: [
          { id: 'u-1', username: 'alice', email: 'alice@example.com', isBanned: false, failedLoginCount: 0, lockedUntil: null, createdAt: '2026-05-19T00:00:00Z', updatedAt: '2026-05-19T00:00:00Z' },
        ],
        total: 1,
        page: 1,
        perPage: 20,
        totalPages: 1,
      }));
    }
    if (path === '/api/admin/users/u-1/ban') {
      return json(ok({ banned: true, userId: 'u-1', sessionsRevoked: 1 }));
    }
    if (path === '/api/admin/users/u-1/unban') {
      return json(ok({ banned: false, userId: 'u-1' }));
    }
    if (path === '/api/admin/users/u-1/reset-password') {
      return json(ok({ resetCreated: true, resetKey: 'reset-key-123', expiresInHours: 4 }));
    }
    if (path === '/api/admin/users/u-1/set-password') {
      return json(ok({ passwordReset: true, userId: 'u-1', sessionsRevoked: 2 }));
    }

    if (path === '/api/admin/settings' && method === 'GET') {
      return json(ok(state.settings));
    }
    if (path === '/api/admin/settings' && method === 'PUT') {
      state.settings = { ...state.settings, ...(body as Record<string, unknown>) };
      return json(ok(state.settings));
    }
    if (path === '/api/admin/broadcast') {
      return json(ok({ sent: 3 }));
    }
    if (path === '/api/admin/broadcast-update') {
      return json(ok({ broadcasted: true }));
    }
    if (path === '/api/admin/settings/reload-amas') {
      return json(ok(body ?? baseConfig));
    }

    if (path === '/api/admin/monitoring/health') {
      return json(ok({ status: 'healthy', dbSizeBytes: 3_145_728, uptimeSecs: 7200, version: '0.4.3' }));
    }
    if (path === '/api/admin/monitoring/database') {
      return json(ok({ sizeOnDisk: 3_145_728, tableCount: 18, tables: ['users', 'words'], pageSize: 4096, pageCount: 768, walEnabled: true }));
    }
    if (path === '/api/admin/monitoring/check-update') {
      return json(ok({ currentVersion: '0.4.3', latestVersion: '0.4.4', hasUpdate: true, releaseUrl: 'https://example.test/release', releaseNotes: 'Bug fixes' }));
    }
    if (path === '/api/admin/amas/monitoring') {
      return json(ok([{ timestamp: '2026-05-19T00:00:00Z', eventType: 'invariant_violation', data: { field: 'fatigue', value: 1.2 } }]));
    }

    if (path === '/api/admin/clients') {
      return json(ok({
        sseLive: [{
          deviceId: 'device-live-123456',
          platform: 'web',
          userId: 'user-alice-123456',
          connectedSecs: 300,
          connectionCount: 1,
          isBanned: false,
          dataChannels: { amas: 'uploaded', learning: 'nil', telemetry: 'none' },
        }],
        recentlyActive: [{
          deviceId: 'device-recent-987654',
          platform: 'desktop',
          userId: 'user-bob-987654',
          lastSeenAt: '2026-05-19 12:00:00',
          isBanned: true,
          dataChannels: { amas: 'uploaded', learning: 'uploaded', telemetry: 'uploaded' },
        }],
      }));
    }
    if (path === '/api/admin/clients/device-live-123456/ban') {
      return json(ok({ banned: true, deviceId: 'device-live-123456' }));
    }
    if (path === '/api/admin/clients/device-recent-987654/unban') {
      return json(ok({ banned: false, deviceId: 'device-recent-987654' }));
    }
    if (path.endsWith('/request-telemetry')) {
      return json(ok({ requestId: 'telemetry-request-1' }));
    }
    if (path === '/api/admin/telemetry/device-live-123456' || path === '/api/admin/telemetry/device-recent-987654') {
      return json(ok({
        total: 1,
        records: [{
          id: 't-1',
          deviceId: path.split('/').pop(),
          userId: 'u-1',
          eventType: 'snapshot',
          serverTs: '2026-05-19 12:01:00',
          deviceProfile: { cpuCores: 8, memoryGb: 16, screenWidth: 1440, screenHeight: 900, pixelRatio: 2, osName: 'macOS', browserName: 'Chromium', browserVersion: '124', timezone: 'Asia/Shanghai', language: 'zh-CN', touchSupport: false, onlineStatus: true },
          sessionStats: { sessionDurationSecs: 180, actionsPerMin: 4.2, errorCount: 0, avgResponseTimeMs: 120 },
          behaviorSummary: { currentRoute: '/learning', clickCount: 12, clickTargets: [], scrollDepthPct: 40, visibilityChanges: 1, routeChanges: 2 },
          featureUsage: { learning: 3 },
        }],
      }));
    }

    if (path === '/api/admin/wordbook-center/browse') {
      return json(ok([
        { id: 'cet4', name: 'CET-4 核心词', description: '大学英语四级高频词', wordCount: 3200, tags: ['考试', '英语'], version: '1.2.0', author: 'WordForge', imported: false, hasUpdate: false },
        { id: 'ielts', name: 'IELTS 学术词', description: '雅思学术词汇', wordCount: 2200, tags: ['留学'], version: '2.0.0', author: 'WordForge', imported: true, localVersion: '1.9.0', hasUpdate: true },
      ]));
    }
    if (path === '/api/admin/wordbook-center/browse/cet4') {
      return json(ok({ id: 'cet4', name: 'CET-4 核心词', description: '大学英语四级高频词', wordCount: 3200, tags: ['考试', '英语'], version: '1.2.0', author: 'WordForge', words: { data: [{ spelling: 'abandon', phonetic: '/əˈbændən/', meanings: ['放弃'], examples: [] }], total: 3200, page: 1, perPage: 20, totalPages: 160 } }));
    }
    if (path === '/api/admin/wordbook-center/import/cet4') {
      return json(ok({ wordbook: { id: 'wb-1', name: 'CET-4 核心词', description: '大学英语四级高频词', type: 'system', wordCount: 3200 }, wordsImported: 3200, wordsSkipped: 0 }));
    }
    if (path === '/api/admin/wordbook-center/updates') {
      return json(ok([{ remoteId: 'ielts', name: 'IELTS 学术词', localVersion: '1.9.0', remoteVersion: '2.0.0', localWordbookId: 'wb-2' }]));
    }
    if (path === '/api/admin/wordbook-center/updates/ielts/sync') {
      return json(ok({ wordbook: { id: 'wb-2', name: 'IELTS 学术词', wordCount: 2250 }, wordsAdded: 20, wordsUpdated: 30, wordsRemoved: 0 }));
    }

    if (path === '/api/admin/updates/status') {
      return json(ok({ currentVersion: '0.4.3', latestVersion: '0.4.4', latestPublishedAt: '2026-05-19T00:00:00Z', releaseNotes: 'Bug fixes', releaseUrl: 'https://example.test/release', hasUpdate: true, canApply: true, lastCheckedAt: '2026-05-19T00:00:00Z', autoCheckEnabled: true, allowDowngrade: false }));
    }
    if (path === '/api/admin/updates/check') {
      return json(ok({ currentVersion: '0.4.3', latestVersion: '0.4.4', latestPublishedAt: '2026-05-19T00:00:00Z', releaseNotes: 'Bug fixes', releaseUrl: 'https://example.test/release', hasUpdate: true, canApply: true, lastCheckedAt: '2026-05-19T00:00:00Z', autoCheckEnabled: true, allowDowngrade: false }));
    }

    if (path === '/api/admin/amas/config' && method === 'GET') {
      return json(ok(baseConfig));
    }
    if (path === '/api/admin/amas/config' && method === 'PUT') {
      return json(ok({ updated: true, versionHash: 'ghi789current', versionId: 3 }));
    }
    if (path === '/api/admin/amas/metrics') {
      return json(ok({ heuristic: { callCount: 20, totalLatencyUs: 140_000, errorCount: 1 } }));
    }
    if (path === '/api/admin/amas/config/versions') {
      return json(ok([
        { id: 2, versionHash: 'def456current', authorAdminId: 'admin-1', source: 'manual', note: 'current config', parentVersionHash: null, createdAt: '2026-05-19T00:00:00Z' },
        { id: 1, versionHash: 'abc123older', authorAdminId: 'admin-1', source: 'manual', note: 'previous config', parentVersionHash: null, createdAt: '2026-05-18T00:00:00Z' },
      ]));
    }
    if (path === '/api/admin/amas/config/versions/abc123older') {
      return json(ok({ id: 1, versionHash: 'abc123older', authorAdminId: 'admin-1', source: 'manual', note: 'previous config', parentVersionHash: null, createdAt: '2026-05-18T00:00:00Z', snapshotJson: { ...baseConfig, memoryModel: { ...baseConfig.memoryModel, baseDesiredRetention: 0.86 } } }));
    }
    if (path === '/api/admin/amas/config/versions/abc123older/restore') {
      return json(ok({ updated: true, versionHash: 'restore789', versionId: 4 }));
    }
    if (path === '/api/admin/amas/metrics/timeseries') {
      return json(ok([
        { date: '2026-05-18', algorithm: 'heuristic', callCount: 10, avgLatencyUs: 6000, errorCount: 0 },
        { date: '2026-05-19', algorithm: 'ige', callCount: 12, avgLatencyUs: 8000, errorCount: 1 },
      ]));
    }
    if (path === '/api/admin/amas/anomalies') {
      return json(ok({ totalEvents: 120, anomalyCount: 3, violationCount: 1, coldStartExplore: 10, coldStartExploit: 20, byDay: [{ date: '2026-05-19', total: 120, anomalies: 3, violations: 1 }], topViolationFields: [{ field: 'fatigue', count: 1 }] }));
    }
    if (path === '/api/admin/amas/user-state/distribution') {
      const hist = (field: string) => ({ field, min: 0, max: 1, bins: [1, 2, 3, 2, 1], mean: 0.5, median: 0.5, sampleSize: 9 });
      return json(ok({ attention: hist('attention'), fatigue: hist('fatigue'), motivation: hist('motivation'), confidence: hist('confidence'), coldStartExplore: 5, coldStartExploit: 8 }));
    }
    if (path === '/api/admin/amas/compare') {
      return json(ok({
        a: { versionHash: 'abc123older', eventCount: 20, anomalyCount: 2, anomalyRate: 0.1, meanLatencyMs: 8, meanReward: 0.3, meanAttention: 0.7, meanFatigue: 0.4, meanMotivation: 0.5, meanConfidence: 0.6, firstEventAt: null, lastEventAt: null },
        b: { versionHash: 'def456current', eventCount: 40, anomalyCount: 1, anomalyRate: 0.025, meanLatencyMs: 6, meanReward: 0.45, meanAttention: 0.8, meanFatigue: 0.3, meanMotivation: 0.6, meanConfidence: 0.7, firstEventAt: null, lastEventAt: null },
      }));
    }
    if (path === '/api/admin/amas/suggestions') {
      const status = url.searchParams.get('status');
      const suggestions = [
        { id: 7, createdAt: '2026-05-19T00:00:00Z', basedOnVersionHash: 'def456current', patchJson: { 'memoryModel.baseDesiredRetention': 0.86 }, rationale: '提高近期留存稳定性', evidenceJson: { sample: 120 }, status: 'pending', decidedBy: null, decidedAt: null, decisionNote: null, costUsd: 0.0123, tokensInput: 1000, tokensOutput: 400, confidence: 0.91 },
        { id: 6, createdAt: '2026-05-18T00:00:00Z', basedOnVersionHash: 'abc123older', patchJson: { 'modeling.fatigueIncreaseRate': 0.018 }, rationale: '降低疲劳累积', evidenceJson: {}, status: 'approved', decidedBy: 'admin-1', decidedAt: '2026-05-18T01:00:00Z', decisionNote: null, costUsd: 0.01, tokensInput: 800, tokensOutput: 300, confidence: 0.88 },
      ];
      return json(ok(status === 'pending' ? suggestions.filter((s) => s.status === 'pending') : suggestions));
    }
    if (path === '/api/admin/amas/suggestions/7/approve') {
      return json(ok({ updated: true, versionHash: 'approved789', versionId: 5 }));
    }
    if (path === '/api/admin/amas/suggestions/7/reject') {
      return json(ok({ rejected: true }));
    }
    if (path === '/api/admin/amas/suggestions/spend') {
      return json(ok({ todayCostUsd: 0.023, todayTokensInput: 1800, todayTokensOutput: 700, dailyCapUsd: 1, remainingUsd: 0.977 }));
    }
    if (path === '/api/admin/amas/suggestions/explain') {
      return json(ok({ explanation: '该参数控制目标留存率。', model: 'test', costUsd: 0.001, tokensInput: 10, tokensOutput: 10 }));
    }

    return json({ success: false, code: 'NOT_MOCKED', message: `No mock for ${method} ${path}` }, 500);
  });

  return state;
}

async function loginAsAdmin(page: Page) {
  await page.addInitScript((token) => {
    sessionStorage.setItem('eng_admin_token', token);
  }, jwt());
}

test.describe('Admin real UI flows', () => {
  test.beforeEach(async ({ page }) => {
    await loginAsAdmin(page);
  });

  test('mobile navigation opens real admin sections', async ({ page }) => {
    await mockAdminApi(page);
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto('/admin/users');

    await expect(page.getByRole('heading', { name: '用户管理' })).toBeVisible();
    await page.getByRole('button', { name: '打开导航菜单' }).click();
    await expect(page.getByRole('link', { name: /AMAS 配置/ })).toBeVisible();
    await page.getByRole('link', { name: /用户管理/ }).click();
    await expect(page.getByText('alice')).toBeVisible();
  });

  test('dashboard loads KPI cards and changes time window', async ({ page }) => {
    await mockAdminApi(page);
    await page.goto('/admin');

    await expect(page.getByRole('heading', { name: '全局概览' })).toBeVisible();
    await expect(page.getByText('注册用户')).toBeVisible();
    // WindowPicker 用 role="radio"（a11y 加固，原 button）
    await page.getByRole('radio', { name: '14天' }).click();
    await expect(page).toHaveURL(/days=14/);
    await expect(page.getByText('14天答题数')).toBeVisible();
  });

  test('analytics switches windows and renders retention/state panels', async ({ page }) => {
    await mockAdminApi(page);
    await page.goto('/admin/analytics');

    await expect(page.getByRole('heading', { name: '深度分析' })).toBeVisible();
    await page.getByRole('radio', { name: '30天' }).click();
    await expect(page).toHaveURL(/days=30/);
    await expect(page.getByText('记忆遗忘曲线')).toBeVisible();
    await expect(page.getByText('单词状态分布')).toBeVisible();
  });

  test('user management supports ban confirmation', async ({ page }) => {
    const state = await mockAdminApi(page);
    await page.goto('/admin/users');

    await expect(page.getByText('alice')).toBeVisible();
    await page.getByRole('button', { name: '封禁' }).click();
    await expect(page.getByRole('heading', { name: '确认封禁' })).toBeVisible();
    await page.getByRole('button', { name: '确认封禁' }).click();
    await expect.poll(() => state.calls.some((c) => c.path === '/api/admin/users/u-1/ban')).toBe(true);
  });

  test('user management generates reset key', async ({ page }) => {
    await mockAdminApi(page);
    await page.goto('/admin/users');

    await page.getByRole('button', { name: '重置密码' }).click();
    await page.getByText('生成重置密钥').click();
    await expect(page.getByText('reset-key-123')).toBeVisible();
  });

  test('user management validates direct password mismatch', async ({ page }) => {
    await mockAdminApi(page);
    await page.goto('/admin/users');

    await page.getByRole('button', { name: '重置密码' }).click();
    await page.getByText('直接重置密码').click();
    await page.getByLabel('新密码').fill('Strongp4ssword!');
    await page.getByLabel('确认密码').fill('Differentp4ssword!');
    await page.getByRole('button', { name: '确认重置' }).click();
    await expect(page.getByText('两次密码输入不一致')).toBeVisible();
  });

  test('user management submits direct password reset', async ({ page }) => {
    const state = await mockAdminApi(page);
    await page.goto('/admin/users');

    await page.getByRole('button', { name: '重置密码' }).click();
    await page.getByText('直接重置密码').click();
    await page.getByLabel('新密码').fill('Strongp4ssword!');
    await page.getByLabel('确认密码').fill('Strongp4ssword!');
    await page.getByRole('button', { name: '确认重置' }).click();
    await expect.poll(() => state.calls.some((c) => c.path === '/api/admin/users/u-1/set-password')).toBe(true);
  });

  test('settings saves basic settings', async ({ page }) => {
    const state = await mockAdminApi(page);
    await page.goto('/admin/settings');

    await page.getByLabel('最大用户数').fill('2000');
    await page.getByRole('button', { name: '保存设置' }).first().click();
    await expect.poll(() => state.calls.some((c) => c.method === 'PUT' && c.path === '/api/admin/settings')).toBe(true);
  });

  test('settings confirms maintenance mode before saving', async ({ page }) => {
    await mockAdminApi(page);
    await page.goto('/admin/settings');

    await page.getByLabel('维护模式').click();
    await expect(page.getByRole('heading', { name: '确认开启维护模式' })).toBeVisible();
    await page.getByRole('button', { name: '确认开启' }).click();
    await expect(page.getByLabel('维护模式')).toBeChecked();
  });

  test('settings broadcasts user message after confirmation', async ({ page }) => {
    const state = await mockAdminApi(page);
    await page.goto('/admin/settings');

    await page.getByLabel('标题').fill('维护通知');
    await page.getByPlaceholder('通知内容').fill('今晚维护 10 分钟');
    await page.getByRole('button', { name: '发送广播' }).click();
    await expect(page.getByRole('heading', { name: '确认发送广播' })).toBeVisible();
    await page.getByRole('button', { name: '确认发送' }).click();
    await expect.poll(() => state.calls.some((c) => c.path === '/api/admin/broadcast')).toBe(true);
  });

  test('settings sends update broadcast', async ({ page }) => {
    const state = await mockAdminApi(page);
    await page.goto('/admin/settings');

    await page.getByPlaceholder('有新版本可用，请刷新页面获取最新内容').fill('请刷新页面');
    await page.getByRole('button', { name: '发送更新通知' }).click();
    await expect(page.getByRole('heading', { name: '确认发送更新通知' })).toBeVisible();
    await page.getByRole('button', { name: '确认发送' }).click();
    await expect.poll(() => state.calls.some((c) => c.path === '/api/admin/broadcast-update')).toBe(true);
  });

  test('wordbook center shows unconfigured state', async ({ page }) => {
    await mockAdminApi(page, { wordbookUrl: '' });
    await page.goto('/admin/wordbook-center');

    await expect(page.getByText('尚未配置词书中心 URL')).toBeVisible();
  });

  test('wordbook center filters, previews, and imports a wordbook', async ({ page }) => {
    const state = await mockAdminApi(page);
    await page.goto('/admin/wordbook-center');

    await expect(page.getByText('CET-4 核心词')).toBeVisible();
    await page.getByPlaceholder('搜索词书...').fill('CET');
    await expect(page.getByText('IELTS 学术词')).toBeHidden();
    await page.getByText('CET-4 核心词').click();
    await expect(page.getByText('abandon')).toBeVisible();
    await page.getByRole('button', { name: '关闭' }).click();
    await page.getByRole('button', { name: '导入为系统词书' }).click();
    // 新增二次确认 ConfirmDialog
    await page.getByRole('button', { name: '确认导入' }).click();
    await expect.poll(() => state.calls.some((c) => c.path === '/api/admin/wordbook-center/import/cet4')).toBe(true);
  });

  test('wordbook center checks updates and syncs imported wordbook', async ({ page }) => {
    const state = await mockAdminApi(page);
    await page.goto('/admin/wordbook-center');

    await page.getByRole('button', { name: '检查更新' }).click();
    await expect(page.getByText('1 本词书有更新')).toBeVisible();
    await page.getByRole('button', { name: /同步「IELTS 学术词」/ }).click();
    await expect.poll(() => state.calls.some((c) => c.path === '/api/admin/wordbook-center/updates/ielts/sync')).toBe(true);
  });

  test('monitoring page renders health, database, public probe, and AMAS events', async ({ page }) => {
    await mockAdminApi(page);
    await page.goto('/admin/monitoring');

    await expect(page.getByText('系统健康')).toBeVisible();
    await expect(page.getByText('数据库信息')).toBeVisible();
    await expect(page.getByText('公开健康探针')).toBeVisible();
    await expect(page.getByText('invariant_violation')).toBeVisible();
  });

  test('clients page bans live device with reason', async ({ page }) => {
    const state = await mockAdminApi(page);
    await page.goto('/admin/clients');

    await expect(page.getByText('device-l...3456')).toBeVisible();
    await page.getByRole('button', { name: '封禁' }).click();
    await page.getByPlaceholder('封禁原因（可选）').fill('异常请求');
    await page.getByRole('button', { name: '确认封禁' }).click();
    await expect.poll(() => state.calls.some((c) => c.path === '/api/admin/clients/device-live-123456/ban')).toBe(true);
  });

  test('clients page requests telemetry and opens history panel', async ({ page }) => {
    const state = await mockAdminApi(page);
    await page.goto('/admin/clients');

    await page.getByRole('button', { name: '拉取遥测' }).click();
    await page.getByRole('button', { name: '历史' }).click();
    await expect(page.getByText('遥测记录')).toBeVisible();
    await expect(page.getByText('Chromium 124')).toBeVisible();
    await expect.poll(() => state.calls.some((c) => c.path.endsWith('/request-telemetry'))).toBe(true);
  });

  test('clients page switches to recent devices and unbans device', async ({ page }) => {
    const state = await mockAdminApi(page);
    await page.goto('/admin/clients');

    // Clients tab 用 role="tab"
    await page.getByRole('tab', { name: /近期活跃/ }).click();
    await expect(page.getByText('desktop')).toBeVisible();
    await page.getByRole('button', { name: '解封' }).click();
    await page.getByRole('button', { name: '确认解封' }).click();
    await expect.poll(() => state.calls.some((c) => c.path === '/api/admin/clients/device-recent-987654/unban')).toBe(true);
  });

  test('updates page checks release and opens apply confirmation', async ({ page }) => {
    const state = await mockAdminApi(page);
    await page.goto('/admin/updates');

    await expect(page.getByText('当前版本')).toBeVisible();
    await page.getByRole('button', { name: '立即检查' }).click();
    await page.getByRole('button', { name: /一键更新到 0.4.4/ }).click();
    await expect(page.getByRole('heading', { name: '确认一键更新' })).toBeVisible();
    await expect.poll(() => state.calls.some((c) => c.path === '/api/admin/updates/check')).toBe(true);
  });

  test('AMAS config edits, discards, saves, and hot reloads', async ({ page }) => {
    const state = await mockAdminApi(page);
    await page.goto('/admin/amas-config');

    await expect(page.getByText('AMAS 调参')).toBeVisible();
    const firstNumber = page.locator('input[type="number"]').first();
    await firstNumber.fill('0.5');
    await expect(page.getByText('未保存的修改')).toBeVisible();
    await page.getByRole('button', { name: '放弃修改' }).click();
    await expect(page.getByText('未保存的修改')).toBeHidden();
    await firstNumber.fill('0.5');
    await page.getByRole('button', { name: '保存配置' }).click();
    // 新增 ConfirmDialog："确认保存 AMAS 配置"
    await page.getByRole('button', { name: '确认保存' }).click();
    await expect.poll(() => state.calls.some((c) => c.method === 'PUT' && c.path === '/api/admin/amas/config')).toBe(true);
    // 保存成功后 baseline 同步 → dirty=false → 热重载按钮被 disabled (title="无修改")
    // 想触发热重载需再造一次脏修改
    await firstNumber.fill('0.6');
    await expect(page.getByText('未保存的修改')).toBeVisible();
    await page.getByRole('button', { name: '热重载' }).click();
    // 热重载同样走 ConfirmDialog："确认热重载 AMAS 配置"
    await page.getByRole('button', { name: '确认热重载' }).click();
    await expect.poll(() => state.calls.some((c) => c.path === '/api/admin/settings/reload-amas')).toBe(true);
  });

  test('AMAS config version drawer expands and restores an older version', async ({ page }) => {
    const state = await mockAdminApi(page);
    await page.goto('/admin/amas-config');

    await page.getByRole('button', { name: '版本历史' }).click();
    await expect(page.getByText('previous config')).toBeVisible();
    await page.getByText('abc123olde').click();
    await page.getByRole('button', { name: '回滚到此版本' }).click();
    await page.getByRole('button', { name: '确认回滚' }).click();
    await expect.poll(() => state.calls.some((c) => c.path === '/api/admin/amas/config/versions/abc123older/restore')).toBe(true);
  });

  test('AMAS metrics switches between all dashboard tabs', async ({ page }) => {
    await mockAdminApi(page);
    await page.goto('/admin/amas-metrics');

    await expect(page.getByRole('heading', { name: '算法延迟 / 错误率' })).toBeVisible();
    await page.getByRole('tab', { name: /异常/ }).click();
    await expect(page.getByText('异常次数')).toBeVisible();
    await page.getByRole('tab', { name: /用户状态分布/ }).click();
    await expect(page.getByText('注意力')).toBeVisible();
    await page.getByRole('tab', { name: /版本对比/ }).click();
    await expect(page.getByText('差异表')).toBeVisible();
  });

  test('AMAS advisor approves and rejects pending suggestions', async ({ page }) => {
    const state = await mockAdminApi(page);
    // 批准/拒绝改用项目自带 ConfirmDialog/Modal，不再触发浏览器原生 confirm()/prompt()
    await page.goto('/admin/amas-advisor');

    await expect(page.getByText('提高近期留存稳定性')).toBeVisible();
    // 卡片"批准并应用" → 打开 ConfirmDialog；dialog 内同名按钮才真正调 API（取 last）
    await page.getByRole('button', { name: '批准并应用' }).first().click();
    await expect(page.getByRole('heading', { name: '确认批准并应用 patch' })).toBeVisible();
    await page.getByRole('button', { name: '批准并应用' }).last().click();
    await expect.poll(() => state.calls.some((c) => c.path === '/api/admin/amas/suggestions/7/approve')).toBe(true);
    // 卡片"拒绝" → 打开 Modal；Modal 内"确认拒绝"才真正调 API
    await page.getByRole('button', { name: '拒绝' }).click();
    await page.getByRole('button', { name: '确认拒绝' }).click();
    await expect.poll(() => state.calls.some((c) => c.path === '/api/admin/amas/suggestions/7/reject')).toBe(true);
  });
});
