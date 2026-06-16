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
        summary: { totalDurationSecs: 7200, sessionCount: 12, recordCount: 180, correctCount: 150, accuracy: 0.83, newWords: 32, reviewWords: 148, masteredWords: 9, recordCountDeltaPct: 0.1, accuracyDeltaPt: 0.03, prevAccuracy: 0.8 },
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
    // v1.1.4 dashboard / analytics 新增聚合端点
    if (path === '/api/admin/analytics/hourly') {
      const matrix = Array.from({ length: 7 }, () => Array.from({ length: 24 }, () => 1));
      return json(ok({ generatedAt: '2026-05-19T00:00:00Z', days: Number(url.searchParams.get('days') ?? 7), matrix, total: 168 }));
    }
    if (path === '/api/admin/analytics/kpi-summary') {
      return json(ok({
        generatedAt: '2026-05-19T00:00:00Z',
        days: Number(url.searchParams.get('days') ?? 7),
        rangeStart: '2026-05-12', rangeEnd: '2026-05-19',
        newRegistrations: { value: 12, prevValue: 10, deltaPct: 0.2 },
        dauAverage: { value: 11, prevValue: 9, deltaPct: 0.22 },
        d7Retention: { value: 0.74, prevValue: 0.7, deltaPt: 0.04 },
        studyDurationSecs: { value: 7200, prevValue: 6000, deltaPct: 0.2 },
      }));
    }
    if (path === '/api/admin/analytics/funnel') {
      return json(ok({
        generatedAt: '2026-05-19T00:00:00Z',
        days: Number(url.searchParams.get('days') ?? 7),
        rangeStart: '2026-05-12', rangeEnd: '2026-05-19',
        steps: [
          { key: 'register', label: '注册', sublabel: '本窗口队列', count: 42, pct: 1, deltaPt: null, tone: 'normal' },
          { key: 'pick_wordbook', label: '选词书', sublabel: '挑选词书', count: 30, pct: 0.71, deltaPt: 0.05, tone: 'good' },
          { key: 'first_answer', label: '首次答题', sublabel: '完成第一题', count: 22, pct: 0.52, deltaPt: -0.03, tone: 'warn' },
          { key: 'd1_return', label: '次日回访', sublabel: 'D1 留存', count: 14, pct: 0.33, deltaPt: -0.02, tone: 'warn' },
        ],
        biggestDropFrom: '选词书', biggestDropTo: '首次答题', biggestDropPt: 0.19,
      }));
    }
    if (path === '/api/admin/analytics/retention-matrix') {
      return json(ok({
        generatedAt: '2026-05-19T00:00:00Z',
        weeks: 3,
        cohorts: [
          { cohortStart: '2026-05-05', size: 20, cells: [1, 0.6, 0.4] },
          { cohortStart: '2026-05-12', size: 18, cells: [1, 0.5, null] },
        ],
      }));
    }
    if (path === '/api/admin/analytics/question-distribution') {
      return json(ok({
        generatedAt: '2026-05-19T00:00:00Z',
        days: Number(url.searchParams.get('days') ?? 7),
        totalRecords: 180,
        questionTypes: [{ key: 'mcq', label: '选择题', count: 100, pct: 0.55 }, { key: 'spell', label: '拼写', count: 80, pct: 0.45 }],
        difficultyBins: [
          { label: '简单', min: 0, max: 1100, count: 40, pct: 0.22 },
          { label: '中等', min: 1100, max: 1300, count: 100, pct: 0.55 },
          { label: '困难', min: 1300, max: null, count: 40, pct: 0.22 },
        ],
      }));
    }
    if (path === '/api/admin/analytics/word-frequency') {
      return json(ok({
        generatedAt: '2026-05-19T00:00:00Z',
        days: Number(url.searchParams.get('days') ?? 7),
        limit: 10,
        sort: url.searchParams.get('sort') ?? 'count',
        rows: [
          { rank: 1, wordId: 'w-1', spelling: 'abandon', pos: 'v.', recordCount: 30, accuracy: 0.7, elo: 1250, mastery: 0.5 },
          { rank: 2, wordId: 'w-2', spelling: 'benevolent', pos: 'adj.', recordCount: 22, accuracy: 0.55, elo: 1320, mastery: 0.4 },
        ],
      }));
    }
    if (path === '/api/admin/analytics/insights') {
      return json(ok({
        generatedAt: '2026-05-19T00:00:00Z',
        days: Number(url.searchParams.get('days') ?? 7),
        items: [
          { tone: 'success', title: '留存改善', body: 'd7 留存较上窗口提升 4 个百分点。' },
          { tone: 'warning', title: '首答流失', body: '选词书到首次答题流失 19 个百分点,建议优化引导。' },
        ],
      }));
    }

    // be:analytics-misc:用户筛选 chip 计数
    if (path === '/api/admin/users/facets') {
      return json(ok({ total: 42, active: 11, inactive7d: 8, banned: 2, admins: 1 }));
    }

    if (path === '/api/admin/users' && method === 'GET') {
      return json(ok({
        data: [
          {
            id: 'u-1', username: 'alice', email: 'alice@example.com', role: 'user', status: 'active',
            isBanned: false, failedLoginCount: 0, lockedUntil: null,
            createdAt: '2026-05-19T00:00:00Z', updatedAt: '2026-05-19T00:00:00Z', lastLoginAt: '2026-05-19T00:00:00Z',
            stats: { recordCount: 120, correctCount: 90, last20Outcomes: [1, 0, 1, 1, 0, 1, 1, 1, 0, 1] },
          },
        ],
        total: 1,
        page: 1,
        perPage: 20,
        totalPages: 1,
      }));
    }
    // m024/m025:Drawer 数据源（详情抽屉打开时拉取,返回空集即可）
    if (/^\/api\/admin\/users\/u-1\/(profile|sessions|devices|audit-log|extras|activity-log)$/.test(path)) {
      if (path.endsWith('/profile')) return json(ok({ userId: 'u-1', totalRecords: 120, correctRecords: 90, accuracy: 0.75, avgResponseTimeMs: 120, sessionCount: 8, wordbookDistribution: [] }));
      if (path.endsWith('/sessions')) return json(ok({ sessions: [] }));
      if (path.endsWith('/devices')) return json(ok({ devices: [] }));
      if (path.endsWith('/audit-log')) return json(ok({ entries: [] }));
      if (path.endsWith('/extras')) return json(ok({}));
      if (path.endsWith('/activity-log')) return json(ok({ entries: [] }));
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
    // v1.1.4 系统设置页:section 化 config + RBAC + API keys + 维护模式
    if (path === '/api/admin/settings/config' && method === 'GET') {
      return json(ok({
        sections: [
          // m036:新设置页「配置板块」只展示运行时热更 live section（后端从 Config 投影下发）
          { section: 'live-ratelimit', json: { apiWindowSecs: 60, apiAnonMaxRequests: 1000, apiAuthedMaxRequests: 5000, authWindowSecs: 60, authMaxRequests: 100, telemetryWindowSecs: 60, telemetryMaxRequests: 500 }, updatedAt: '2026-05-19T00:00:00Z' },
          { section: 'live-limits', json: { maxBatchSize: 100, maxImportWords: 10000, maxRecordsFetch: 5000, maxStatsRecords: 1000, maxExcludeWordIds: 1000, maxSseConnections: 1000, maxSseConnectionsPerUser: 10, candidateWordPoolSize: 5000, rateLimitMaxEntries: 10000, defaultPageSize: 20, maxPageSize: 100 }, updatedAt: '2026-05-19T00:00:00Z' },
          { section: 'live-auth', json: { accessTokenHours: 2, refreshTokenHours: 720, adminTokenHours: 24 }, updatedAt: '2026-05-19T00:00:00Z' },
        ],
      }));
    }
    if (path.startsWith('/api/admin/settings/config/') && method === 'PUT') {
      const section = path.split('/').pop()!;
      return json(ok({ section, json: body ?? {}, updatedAt: '2026-06-04T00:00:00Z' }));
    }
    if (path === '/api/admin/settings/snapshots' && method === 'GET') {
      return json(ok({ snapshots: [] }));
    }
    if (path === '/api/admin/settings/snapshots' && method === 'POST') {
      return json(ok({ id: 1, label: null, createdAt: '2026-06-04T00:00:00Z' }));
    }
    if (path === '/api/admin/settings/admins') {
      return json(ok({ admins: [{ id: 'admin-1', email: 'admin@example.com', role: 'super_admin', createdAt: '2026-05-19T00:00:00Z' }] }));
    }
    if (path === '/api/admin/settings/api-keys') {
      return json(ok({ keys: [] }));
    }
    if (path === '/api/admin/settings/backup-status') {
      return json(ok({ targets: [] }));
    }
    if (path === '/api/admin/settings/maintenance' && method === 'POST') {
      const next = (body as { active?: boolean } | undefined)?.active ?? false;
      return json(ok({ active: next }));
    }
    if (path === '/api/admin/settings/version-gate') {
      return json(ok({ enabled: false, minClientVersion: null, effectiveMinClientVersion: null }));
    }
    // GET /api/admin/broadcast → 看板（stats + broadcasts + pagination）
    if (path === '/api/admin/broadcast' && method === 'GET') {
      return json(ok({
        stats: { total: 3, totalSent: 1024, avgReadRate: 0.62, online: 8 },
        broadcasts: [
          { id: 'bc-1', createdAt: '2026-05-19T00:00:00Z', author: 'admin', title: '历史广播', message: '内容', sentCount: 1024, readCount: 600, readRate: 0.6 },
        ],
        pagination: { total: 1, offset: 0, limit: 20 },
      }));
    }
    if (path === '/api/admin/broadcast/preview') {
      return json(ok({ matched: 428, total: 1024 }));
    }
    if (path === '/api/admin/broadcast/scheduled') {
      return json(ok({ items: [] }));
    }
    // m042/D2:推送草稿存/取/删
    if (path === '/api/admin/broadcast/draft' && method === 'GET') {
      return json(ok({ draft: null }));
    }
    if (path === '/api/admin/broadcast/draft' && method === 'POST') {
      return json(ok({ draft: { ...((body as Record<string, unknown>) ?? {}), authorId: 'admin-1', updatedAt: '2026-06-02T00:00:00Z' } }));
    }
    if (path === '/api/admin/broadcast/draft' && method === 'DELETE') {
      return json(ok({ deleted: true }));
    }
    if (path === '/api/admin/broadcast') {
      // m042/D2:scheduledAt 为未来时间→排程响应;否则即时
      const scheduledAt = (body as { scheduledAt?: string } | undefined)?.scheduledAt;
      if (scheduledAt) {
        return json(ok({ scheduled: true, scheduledAt, broadcastId: 'bc-scheduled-1' }));
      }
      return json(ok({ sent: 3, broadcastId: 'bc-1' }));
    }
    if (path === '/api/admin/broadcast-update') {
      return json(ok({ broadcasted: true }));
    }
    if (path === '/api/admin/settings/reload-amas') {
      return json(ok(body ?? baseConfig));
    }

    if (path === '/api/admin/monitoring/health') {
      return json(ok({
        status: 'healthy', storeProbeOk: true, dbSizeBytes: 3_145_728, uptimeSecs: 7200, version: '0.4.3',
        services: { store: { healthy: true }, amas: { healthy: true }, sse: { healthy: true, activeConnections: 1, activeDevices: 1 } },
        resources: { cpuPct: 12.5, memoryRssBytes: 52_428_800, memoryTotalBytes: 100_000_000, pool: { connections: 2, max: 8 }, inflightRequests: 1 },
      }));
    }
    if (path === '/api/admin/monitoring/database') {
      return json(ok({ sizeOnDisk: 3_145_728, tableCount: 18, tables: ['users', 'words'], pageSize: 4096, pageCount: 768, walEnabled: true }));
    }
    if (path === '/api/admin/monitoring/check-update') {
      return json(ok({ currentVersion: '0.4.3', latestVersion: '0.4.4', hasUpdate: true, releaseUrl: 'https://example.test/release', releaseNotes: 'Bug fixes' }));
    }
    // v1.1.4 系统监控页:滚动请求指标 / 实时日志 / 派生告警时间线
    if (path === '/api/admin/monitoring/requests') {
      return json(ok({
        window: url.searchParams.get('window') ?? '1h',
        totalRequests: 1200, errorCount: 4, error5xx: 1, p50Ms: 12, p95Ms: 80, p99Ms: 140, qps: 3.2, qpsAvg: 3.2, availabilityPct: 99.95,
        series: [
          { t: Math.floor(Date.now() / 1000) - 60, qps: 3.0, p99Ms: 130 },
          { t: Math.floor(Date.now() / 1000), qps: 3.4, p99Ms: 140 },
        ],
      }));
    }
    if (path === '/api/admin/monitoring/logs') {
      return json(ok({ logs: [{ ts: Date.now(), level: 'INFO', target: 'learning_backend', message: 'request handled' }] }));
    }
    if (path === '/api/admin/monitoring/events') {
      return json(ok({ events: [{ id: 'ev-1', kind: 'worker_failure', severity: 'warn', title: 'worker 抖动', message: 'llm_advisor 一次失败', at: '2026-05-19T00:00:00Z' }] }));
    }
    if (path === '/api/admin/amas/monitoring') {
      return json(ok([{ timestamp: '2026-05-19T00:00:00Z', eventType: 'invariant_violation', data: { field: 'fatigue', value: 1.2 } }]));
    }
    if (path === '/api/admin/amas/advisor/cost') {
      return json(ok({ monthYuan: 1.2, monthCapYuan: 100, quotaPct: 0.012, forecastYuan: 3.5, avg7dCostYuan: 0.1, monthCalls: 12, acceptedCount: 5, rejectedCount: 2, acceptanceRate: 0.71, todayYuan: 0.05, usdToCny: 7.3 }));
    }
    if (path === '/api/admin/monitoring/workers') {
      return json(ok({
        workers: [
          { workerName: 'amas_decision_loop', lastRunAt: 1747641600, lastDurationMs: 12, lastOutcome: 'ok', lastError: null },
          { workerName: 'session_aggregator', lastRunAt: 1747641600, lastDurationMs: 8, lastOutcome: 'ok', lastError: null },
        ],
      }));
    }
    // Resource packs:list（AdminPackEntry[]）+ summary + stats
    if (path === '/api/admin/resource-packs' && method === 'GET') {
      return json(ok([
        {
          packId: 'cet4-2k',
          description: 'CET-4 2K 核心词',
          createdAt: '2026-04-01T00:00:00Z',
          updatedAt: '2026-05-12T00:00:00Z',
          active: { stable: 'v1.2.0', beta: null, internal: null },
          totalInstalls: 128,
          outcomes7d: { installed: 120, verify_failed: 5, rollback: 3 },
          versions: [
            { packId: 'cet4-2k', version: 'v1.2.0', sha256: 'a'.repeat(64), signature: 'sig', signatureAlg: 'Ed25519', sizeBytes: 524288, minAppVersion: '0.7.0', channel: 'stable', payloadPath: 'static/packs/cet4-2k/v1.2.0/payload.json', publishedAt: '2026-04-01T00:00:00Z', deactivatedAt: null },
            { packId: 'cet4-2k', version: 'v1.3.0-beta.1', sha256: 'b'.repeat(64), signature: 'sig', signatureAlg: 'Ed25519', sizeBytes: 540000, minAppVersion: null, channel: 'beta', payloadPath: 'static/packs/cet4-2k/v1.3.0-beta.1/payload.json', publishedAt: '2026-05-12T00:00:00Z', deactivatedAt: null },
          ],
        },
      ]));
    }
    if (path === '/api/admin/resource-packs/summary') {
      return json(ok({
        totalPacks: 1, newPacksThisMonth: 0, totalVersions: 2,
        versionsByChannel: { stable: 1, beta: 1, internal: 0 },
        installsToday: 32, installsTodaySuccess: 30, failureRate7d: 0.06,
        failures7dByOutcome: { verify_failed: 5, rollback: 3 }, onlineClients: 8,
      }));
    }
    if (path.startsWith('/api/admin/resource-packs/') && method === 'GET' && path.endsWith('/stats')) {
      return json(ok({ packId: 'cet4-2k', stats: [{ version: 'v1.2.0', outcome: 'installed', count: 120 }] }));
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
    // m027:设备页平台聚合 + 版本分布 + 升级策略
    if (path === '/api/admin/clients/distribution') {
      return json(ok({
        platforms: [
          { platform: 'web', total: 30, active7d: 12, monthOverMonthPct: 8 },
          { platform: 'ios', total: 10, active7d: 4, monthOverMonthPct: -2 },
          { platform: 'android', total: 5, active7d: 2, monthOverMonthPct: 1 },
        ],
        versions: [
          { platform: 'web', version: '0.7.0', count: 20 },
          { platform: 'web', version: '0.6.0', count: 10 },
        ],
        policies: [
          { platform: 'web', minVersion: null, suggestedVersion: '0.7.0', grayscalePct: 0, pwaSilentUpdate: true },
          { platform: 'ios', minVersion: null, suggestedVersion: null, grayscalePct: 0, pwaSilentUpdate: false },
          { platform: 'android', minVersion: null, suggestedVersion: null, grayscalePct: 0, pwaSilentUpdate: false },
        ],
      }));
    }
    if (path === '/api/admin/clients/paginated') {
      return json(ok({ data: [], total: 0, page: 1, perPage: 20, totalPages: 1 }));
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
    // 遥测分类总览（设备画像 + 每类聚合 + 操作概览）
    if (path.startsWith('/api/admin/telemetry/') && path.endsWith('/summary')) {
      return json(ok({
        total: 1,
        firstTs: '2026-05-19 12:00:00',
        lastTs: '2026-05-19 12:01:00',
        byEventType: [{ eventType: 'snapshot', count: 1, avgDurationSecs: 180, totalErrors: 0, avgActionsPerMin: 4.2, avgResponseMs: 120 }],
        deviceProfile: { cpuCores: 8, memoryGb: 16, screenWidth: 1440, screenHeight: 900, pixelRatio: 2, osName: 'macOS', browserName: 'Chromium', browserVersion: '124', timezone: 'Asia/Shanghai', language: 'zh-CN', touchSupport: false, onlineStatus: true },
        featureUsage: [{ name: 'learning', count: 3 }],
        routes: [{ name: '/learning', count: 2 }],
        clickTargets: [],
        totalClicks: 12, totalErrors: 0, totalDurationSecs: 180, sessionCount: 1,
      }));
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

    // v1.1.4 词库中心:本地词库列表 + 单本统计（左列表 + 右详情数据源）
    if (path === '/api/admin/wordbooks' && method === 'GET') {
      return json(ok({
        items: [
          { id: 'wb-1', name: 'CET-4 核心词', description: '大学英语四级高频词', type: 'system', wordCount: 3200, activeUsers: 42, tags: ['考试'], createdAt: '2026-05-19T00:00:00Z' },
        ],
        total: 1, page: 1, perPage: 200, totalPages: 1,
        counts: { all: 1, system: 1, user: 0, totalEntries: 3200 },
      }));
    }
    if (/^\/api\/admin\/wordbooks\/[^/]+\/stats$/.test(path)) {
      return json(ok({ wordbookId: 'wb-1', totalWords: 3200, activeUsers: 42, avgMastery: 0.62, weeklyAnswers: 180 }));
    }
    if (/^\/api\/admin\/wordbooks\/[^/]+\/words$/.test(path) && method === 'GET') {
      return json(ok({ data: [], total: 0, page: 1, perPage: 20, totalPages: 1 }));
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
      // v0.6.0-beta.3：双通道嵌套结构。Stable 卡有 0.4.4 升级；Beta=null
      return json(ok({
        currentVersion: '0.4.3',
        stable: { latestVersion: '0.4.4', latestPublishedAt: '2026-05-19T00:00:00Z', releaseNotes: 'Bug fixes', releaseUrl: 'https://example.test/release', hasUpdate: true, canApply: true },
        beta: null,
        lastCheckedAt: '2026-05-19T00:00:00Z', autoCheckEnabled: true, allowDowngrade: false,
      }));
    }
    if (path === '/api/admin/updates/check') {
      return json(ok({
        currentVersion: '0.4.3',
        stable: { latestVersion: '0.4.4', latestPublishedAt: '2026-05-19T00:00:00Z', releaseNotes: 'Bug fixes', releaseUrl: 'https://example.test/release', hasUpdate: true, canApply: true },
        beta: null,
        lastCheckedAt: '2026-05-19T00:00:00Z', autoCheckEnabled: true, allowDowngrade: false,
      }));
    }
    if (path === '/api/admin/updates/history') {
      return json(ok({ entries: [] }));
    }
    if (path === '/api/admin/updates/backups') {
      return json(ok({ backups: [], totalBytes: 0 }));
    }
    if (path === '/api/admin/updates/changelog') {
      return json(ok({ available: false, channel: 'stable', totalCommits: 0, categories: [] }));
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
    if (path === '/api/admin/amas/metrics/kpi') {
      return json(ok({ decisionTotal: 120, hitCount: 90, hitRate: 0.75, avgLatencyMs: 7, fatigueTriggerCount: 8, fatigueTriggerRate: 0.06, fatigueThreshold: 0.9 }));
    }
    if (path === '/api/admin/amas/metrics/algorithm-distribution') {
      return json(ok([
        { algorithm: 'heuristic', count: 60, pct: 0.5 },
        { algorithm: 'ige', count: 40, pct: 0.33 },
        { algorithm: 'swd', count: 20, pct: 0.17 },
      ]));
    }
    if (path === '/api/admin/amas/metrics/stage-distribution') {
      return json(ok({
        totalUsers: 42,
        stages: [
          { stage: 'cold', users: 12, pct: 0.29, avgDecisions: 5, retention7d: 0.4, mainRoute: 'heuristic' },
          { stage: 'transition', users: 18, pct: 0.43, avgDecisions: 30, retention7d: 0.6, mainRoute: 'ige' },
          { stage: 'stable', users: 12, pct: 0.29, avgDecisions: 90, retention7d: 0.8, mainRoute: 'swd' },
        ],
        trend: [{ date: '2026-05-19', cold: 12, transition: 18, stable: 12 }],
      }));
    }
    if (path === '/api/admin/amas/metrics/elo-scatter') {
      return json(ok({ points: [{ elo: 1250, decisions: 30, deltaElo: 15 }], total: 1, meanElo: 1250 }));
    }
    if (path === '/api/admin/amas/metrics/mdm-heatmap') {
      return json(ok({ days: ['2026-05-19'], bandCount: 14, cells: [Array.from({ length: 14 }, () => 0.3)], peak: 0.3 }));
    }
    if (path === '/api/admin/amas/metrics/fatigue-timeseries') {
      return json(ok({ points: [{ date: '2026-05-19', avgFatigue: 0.4, peakFatigue: 0.7, triggerCount: 2 }], avgIntensity: 0.4, totalTriggers: 2, threshold: 0.9 }));
    }
    if (path === '/api/admin/amas/metrics/decision-histogram') {
      return json(ok({ buckets: [{ label: '0-10', count: 12 }, { label: '10-50', count: 18 }, { label: '50+', count: 12 }], p50: 30, p95: 90, totalUsers: 42 }));
    }
    if (path === '/api/admin/amas/anomalies') {
      return json(ok({ totalEvents: 120, anomalyCount: 3, violationCount: 1, coldStartExplore: 10, coldStartExploit: 20, byDay: [{ date: '2026-05-19', total: 120, anomalies: 3, violations: 1 }], topViolationFields: [{ field: 'fatigue', count: 1 }] }));
    }
    if (path === '/api/admin/amas/anomalies/feed') {
      return json(ok({
        summary: { error: 1, warn: 2, info: 0, invariantPass: 118, invariantTotal: 120, passRate: 0.983 },
        items: [
          { id: 'anom-1', timestamp: '2026-05-19T00:00:00Z', severity: 'error', code: 'fatigue_out_of_range', title: '疲劳越界', userId: 'user-alice-1', value: 1.2, expectedRange: '[0,1]', impactedUsers: 1, impactPct: 0.02 },
        ],
      }));
    }
    if (path === '/api/admin/amas/user-state/distribution') {
      const hist = (field: string) => ({ field, min: 0, max: 1, bins: [1, 2, 3, 2, 1], mean: 0.5, median: 0.5, sampleSize: 9 });
      return json(ok({ attention: hist('attention'), fatigue: hist('fatigue'), motivation: hist('motivation'), confidence: hist('confidence'), coldStartExplore: 5, coldStartExploit: 8 }));
    }
    if (path === '/api/admin/amas/user-state/transitions') {
      return json(ok({ windowHours: 24, transitions: [{ from: 'cold', to: 'transition', count: 5 }] }));
    }
    if (path === '/api/admin/amas/user-state/clusters') {
      return json(ok({ k: 4, totalUsers: 42, clusters: [{ label: '快而准', count: 12, pct: 0.29, avgResponseMs: 800, errorRate: 0.1, recordsPerActiveDay: 30 }] }));
    }
    if (path === '/api/admin/amas/compare') {
      return json(ok({
        a: { versionHash: 'abc123older', eventCount: 20, anomalyCount: 2, anomalyRate: 0.1, meanLatencyMs: 8, meanReward: 0.3, meanAttention: 0.7, meanFatigue: 0.4, meanMotivation: 0.5, meanConfidence: 0.6, firstEventAt: null, lastEventAt: null },
        b: { versionHash: 'def456current', eventCount: 40, anomalyCount: 1, anomalyRate: 0.025, meanLatencyMs: 6, meanReward: 0.45, meanAttention: 0.8, meanFatigue: 0.3, meanMotivation: 0.6, meanConfidence: 0.7, firstEventAt: null, lastEventAt: null },
      }));
    }
    if (path === '/api/admin/amas/compare/ext') {
      const slice = (versionHash: string, eventCount: number) => ({
        versionHash, eventCount, hitRate: 0.75, p95LatencyMs: 80, meanLatencyMs: 7, fatigueRate: 0.06,
        ensembleShare: 0.4, meanReward: 0.4, anomalyRate: 0.05, retention7d: 0.6, p95CompletionMs: 1200,
        spark: [1, 2, 3], configEpsilon: 0.1, firstEventAt: '2026-05-18T00:00:00Z', lastEventAt: '2026-05-19T00:00:00Z',
      });
      return json(ok({ a: slice('abc123older', 20), b: slice('def456current', 40) }));
    }
    if (path === '/api/admin/amas/suggestions' && method === 'GET') {
      const status = url.searchParams.get('status');
      // 仅 pending 返回待审建议;approved/rejected/auto_applied 返回空,避免重复决策按钮干扰断言
      const pending = [
        { id: 7, createdAt: '2026-05-19T00:00:00Z', basedOnVersionHash: 'def456current', patchJson: { 'memoryModel.baseDesiredRetention': 0.86 }, baseValuesJson: { 'memoryModel.baseDesiredRetention': 0.92 }, rationale: '提高近期留存稳定性', evidenceJson: { sample: 120 }, status: 'pending', decidedBy: null, decidedAt: null, decisionNote: null, costUsd: 0.0123, tokensInput: 1000, tokensOutput: 400, confidence: 0.91 },
      ];
      return json(ok(status ? (status === 'pending' ? pending : []) : pending));
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
    // LLM 顾问页附属端点（成本曲线 / 顾问配置 / 白名单 / canary）
    if (path === '/api/admin/amas/advisor/cost/daily') {
      return json(ok([{ date: '2026-05-19', costYuan: 0.1 }]));
    }
    if (path === '/api/admin/amas/advisor/config') {
      return json(ok({ model: 'test-model', pollCron: '0 * * * *', apiKeyTail: '••1234', monthCapYuan: 100, autoApplyEnabled: false, autoApplyMaxPerDay: 2, autoApplyMinConfidence: 0.8, grayscaleSteps: [20, 50, 100], advisorEnabled: true }));
    }
    if (path === '/api/admin/amas/advisor/whitelist') {
      return json(ok([]));
    }
    if (path === '/api/admin/amas/advisor/canary') {
      return json(ok([]));
    }

    return json({ success: false, code: 'NOT_MOCKED', message: `No mock for ${method} ${path}` }, 500);
  });

  return state;
}

async function loginAsAdmin(page: Page) {
  await page.addInitScript((token) => {
    sessionStorage.setItem('eng_admin_token', token);
    // 预置已看过本波导览，避免首登 overlay 拦截既有 admin e2e
    // （值须与 useOnboarding.ts 的 waveOf() 对当前大版本的返回一致；当前波 = v1.2-redesign）
    localStorage.setItem('wf_admin_onboarding_wave', 'v1.2-redesign');
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
    // v1.1.4 侧边栏:旧「AMAS 配置」拆为「AMAS 调参 / AMAS 指标 / LLM 顾问」
    await expect(page.getByRole('link', { name: 'AMAS 调参' })).toBeVisible();
    await page.getByRole('link', { name: '用户管理' }).click();
    await expect(page.getByText('alice', { exact: true })).toBeVisible();
  });

  test('dashboard loads KPI cards and changes time window', async ({ page }) => {
    await mockAdminApi(page);
    await page.goto('/admin');

    await expect(page.getByRole('heading', { name: '全局概览' })).toBeVisible();
    await expect(page.getByText('注册用户')).toBeVisible();
    // WindowPicker 用 role="radio"（a11y 加固，原 button）
    await page.getByRole('button', { name: '14 天' }).click();
    // v1.2 窗口切换只改内部信号（不写 URL）；KPI 卡标题随窗口拼为「{days} 天答题数」
    await expect(page.getByText('14 天答题数')).toBeVisible();
  });

  test('analytics switches windows and renders funnel/cohort panels', async ({ page }) => {
    await mockAdminApi(page);
    await page.goto('/admin/analytics');

    // v1.1.4 深度分析重构为漏斗 + cohort 矩阵 + 答题分布 + 词频,heading 改「数据分析」
    await expect(page.getByRole('heading', { name: '数据分析' })).toBeVisible();
    // 时间窗用 segmented button（非 role=radio），按钮文案带空格「30 天」
    await page.getByRole('button', { name: '30 天' }).click();
    await expect(page).toHaveURL(/days=30/);
    await expect(page.getByRole('heading', { name: '增长漏斗' })).toBeVisible();
    await expect(page.getByRole('heading', { name: '周 cohort 留存矩阵' })).toBeVisible();
    await expect(page.getByRole('heading', { name: '题型分布' })).toBeVisible();
  });

  test('user management supports ban confirmation', async ({ page }) => {
    const state = await mockAdminApi(page);
    await page.goto('/admin/users');

    await expect(page.getByText('alice', { exact: true })).toBeVisible();
    // v1.2 行内「封禁」直接调用（无二次确认弹窗）。exact 避免命中「已封禁 N」筛选标签
    await page.getByRole('button', { name: '封禁', exact: true }).click();
    await expect.poll(() => state.calls.some((c) => c.path === '/api/admin/users/u-1/ban')).toBe(true);
  });

  test('user management generates reset key', async ({ page }) => {
    await mockAdminApi(page);
    await page.goto('/admin/users');

    // v1.2：打开用户详情抽屉 → 底部「重置密码」直接生成重置密钥（无直接设密码流程）
    await expect(page.getByText('alice', { exact: true })).toBeVisible();
    await page.getByRole('button', { name: '详情' }).first().click();
    await page.getByRole('button', { name: '重置密码' }).click();
    await expect(page.getByText('reset-key-123')).toBeVisible();
  });
  // 注：v1.2 已移除「直接设置新密码」流程（重置统一改为生成一次性重置密钥），
  // 故删除原 'validates direct password mismatch' / 'submits direct password reset' 两个用例。

  // m036:v1.2 设置页「配置板块」只保留运行时热更 live section（site 等 store-only 已隐藏）。
  // 改「速率限制」section 首个数字字段 →「保存配置」触发 PUT /settings/config/live-ratelimit。
  test('settings edits a live runtime field and saves the section', async ({ page }) => {
    const state = await mockAdminApi(page);
    await page.goto('/admin/settings');

    // 左侧「运行时 · 即时生效」分组的「速率限制」section（live-ratelimit）
    await page.getByRole('button', { name: '速率限制' }).click();
    // 首个 number 字段（通用 API 限流窗口秒）改值 → dirty → 保存
    await page.locator('input[type="number"]').first().fill('30');
    await page.getByRole('button', { name: '保存配置' }).click();
    await expect
      .poll(() => state.calls.some((c) => c.method === 'PUT' && c.path === '/api/admin/settings/config/live-ratelimit'))
      .toBe(true);
  });

  test('settings confirms maintenance mode before applying', async ({ page }) => {
    const state = await mockAdminApi(page);
    await page.goto('/admin/settings');

    // v1.2:维护模式在「维护 · 备份」tab 内用 Switch 直接切换（无二次确认弹窗）
    await page.getByRole('button', { name: '维护 · 备份' }).click();
    // wf Switch 渲染为 label.switch > input[checkbox]（无 role=switch）；点 label 触发切换
    await page.locator('label.switch').first().click();
    await expect
      .poll(() => state.calls.some((c) => c.method === 'POST' && c.path === '/api/admin/settings/maintenance'))
      .toBe(true);
    await expect(page.getByText('已开启 · 用户访问受限')).toBeVisible();
  });

  test('broadcast page sends system message after confirmation', async ({ page }) => {
    const state = await mockAdminApi(page);
    await page.goto('/admin/broadcast');

    // v1.2 撰写表单:标题/正文用 placeholder 定位，发送按钮「立即发送」（schedule=now）
    await page.getByPlaceholder('如：v1.4.3 更新公告').fill('维护通知');
    await page.locator('textarea[placeholder="支持纯文本…"]').fill('今晚维护 10 分钟');
    await page.getByRole('button', { name: '立即发送' }).click();
    // ConfirmDialog 标题「确认发送广播？」，确认按钮「确认发送」
    await expect(page.getByRole('heading', { name: /确认发送广播/ })).toBeVisible();
    await page.getByRole('button', { name: '确认发送' }).click();
    await expect
      .poll(() => state.calls.some((c) => c.path === '/api/admin/broadcast' && c.method === 'POST'))
      .toBe(true);
  });

  // 注:旧「更新通知」tab + broadcast-update 流程在 v1.1.4 系统广播页已整体移除
  // （broadcastUpdate API 仍在但无 UI 入口），故删除原 'switches to update tab' 用例。

  test('command palette opens on ⌘K and navigates via Enter', async ({ page }) => {
    await mockAdminApi(page);
    await page.goto('/admin');

    // Mac Meta+K 或 Linux/Windows Ctrl+K
    const isMac = process.platform === 'darwin';
    await page.keyboard.press(isMac ? 'Meta+k' : 'Control+k');
    await expect(page.getByRole('dialog', { name: '命令面板' })).toBeVisible();

    // 输入 'broadcast' 模糊匹配,Enter 跳转
    await page.getByPlaceholder('跳转到任意页面或操作…').fill('broadcast');
    await page.keyboard.press('Enter');
    await expect(page).toHaveURL(/\/admin\/broadcast/);
    await expect(page.getByRole('heading', { name: '消息推送' })).toBeVisible();
  });

  test('resource-packs page lists packs and switches channel', async ({ page }) => {
    await mockAdminApi(page);
    await page.goto('/admin/resource-packs');

    // hero（v1.1.4 标题为「资源包」）
    await expect(page.getByRole('heading', { name: '资源包' })).toBeVisible();
    // 通道过滤器（v1.2 用 Tabs/Seg 渲染普通 button，非 role=tab）
    await expect(page.getByRole('button', { name: /Stable/i })).toBeVisible();
    await expect(page.getByRole('button', { name: /Beta/i })).toBeVisible();
  });

  test('wordbook center shows unconfigured state', async ({ page }) => {
    await mockAdminApi(page, { wordbookUrl: '' });
    await page.goto('/admin/wordbook-center');

    // v1.2 远程目录默认折叠，点「远程目录」展开后才显示空态
    await page.getByRole('button', { name: '远程目录', exact: true }).click();
    await expect(page.getByText('尚未配置词书中心 URL')).toBeVisible();
  });

  test('wordbook center previews and imports a remote wordbook', async ({ page }) => {
    const state = await mockAdminApi(page);
    await page.goto('/admin/wordbook-center');

    // v1.2 远程目录默认折叠，先点「远程目录」展开（卡片无 class）。cet4 为首个未导入卡，含「预览」「导入」
    await page.getByRole('button', { name: '远程目录', exact: true }).click();
    await expect(page.getByText('CET-4 核心词').first()).toBeVisible();
    await page.getByRole('button', { name: '预览' }).first().click();
    await expect(page.getByText('abandon')).toBeVisible();
    // 预览 Modal 顶部关闭按钮（aria-label 关闭）
    await page.getByRole('button', { name: '关闭', exact: true }).click();
    await page.getByRole('button', { name: '导入', exact: true }).click();
    // 二次确认「确认导入词库」；Confirm 确认按钮文案为「导入」，取最后一个（弹窗内）避开卡片同名
    await expect(page.getByRole('heading', { name: '确认导入词库' })).toBeVisible();
    // 确认按钮限定在弹窗 .scale-in 内；dispatchEvent 直接在按钮元素上触发 click，
    // 绕过外层 grid-collapse 对指针坐标的拦截（弹窗定位上下文怪癖）
    await page.locator('.scale-in').getByRole('button', { name: '导入', exact: true }).dispatchEvent('click');
    await expect.poll(() => state.calls.some((c) => c.path === '/api/admin/wordbook-center/import/cet4')).toBe(true);
  });

  test('wordbook center checks updates and syncs imported wordbook', async ({ page }) => {
    const state = await mockAdminApi(page);
    await page.goto('/admin/wordbook-center');

    // v1.2 远程目录默认折叠，展开后更新自动加载在「可同步更新」面板（已移除独立「检查更新」按钮）
    await page.getByRole('button', { name: '远程目录', exact: true }).click();
    await expect(page.getByText('可同步更新')).toBeVisible();
    await page.getByRole('button', { name: '同步' }).first().click();
    await expect.poll(() => state.calls.some((c) => c.path === '/api/admin/wordbook-center/updates/ielts/sync')).toBe(true);
  });

  test('monitoring page renders health, workers, requests, and live log panels', async ({ page }) => {
    await mockAdminApi(page);
    await page.goto('/admin/monitoring');

    // v1.1.4 系统监控重构:SLO + 服务状态 + worker 心跳 + 实时请求/日志/告警时间线
    await expect(page.getByRole('heading', { name: '系统监控' })).toBeVisible();
    await expect(page.getByRole('heading', { name: '服务状态' })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Worker 心跳' })).toBeVisible();
    await expect(page.getByRole('heading', { name: '实时日志' })).toBeVisible();
    await expect(page.getByRole('heading', { name: '告警时间线' })).toBeVisible();
  });

  test('clients page bans live device with reason', async ({ page }) => {
    const state = await mockAdminApi(page);
    await page.goto('/admin/clients');

    await expect(page.getByText('device-li…3456')).toBeVisible();
    await page.getByRole('button', { name: '封禁', exact: true }).first().click();
    await page.getByPlaceholder('异常高频请求').fill('异常请求');
    await page.getByRole('button', { name: '确认封禁' }).click();
    await expect.poll(() => state.calls.some((c) => c.path === '/api/admin/clients/device-live-123456/ban')).toBe(true);
  });

  test('clients page requests telemetry and opens history panel', async ({ page }) => {
    const state = await mockAdminApi(page);
    await page.goto('/admin/clients');

    // 在线设备行「拉取遥测」触发 request-telemetry（遥测详情抽屉的 client-detail/summary/records
    // 三个数据源未在本 mock 覆盖，抽屉内容由 store/单测覆盖，这里只验证拉取动作）
    await page.getByRole('button', { name: '拉取遥测' }).first().click();
    await expect.poll(() => state.calls.some((c) => c.path.endsWith('/request-telemetry'))).toBe(true);
  });

  test('clients page switches to recent devices and unbans device', async ({ page }) => {
    const state = await mockAdminApi(page);
    await page.goto('/admin/clients');

    // Clients tab 用 role="tab"
    await page.getByRole('button', { name: /近期活跃/ }).click();
    await expect(page.getByText('desktop')).toBeVisible();
    await page.getByRole('button', { name: '解封' }).click();
    await page.getByRole('button', { name: '确认解封' }).click();
    await expect.poll(() => state.calls.some((c) => c.path === '/api/admin/clients/device-recent-987654/unban')).toBe(true);
  });

  // v1.2：广播编辑器（投递时机调度 + 保存草稿）已从设备页迁至独立「消息推送」页 /admin/broadcast
  test('broadcast schedules a message at a future time', async ({ page }) => {
    const state = await mockAdminApi(page);
    await page.goto('/admin/broadcast');

    await page.getByPlaceholder('如：v1.4.3 更新公告').fill('定时维护通知');
    await page.locator('textarea[placeholder="支持纯文本…"]').fill('今晚 02:00 维护');
    // 投递时机 Seg 切到「定时」，填一个未来时间（稳过未来校验）
    await page.getByRole('button', { name: '定时', exact: true }).click();
    await page.locator('input[type="datetime-local"]').fill('2099-01-01T02:00');
    await page.getByRole('button', { name: '排程发送' }).click();
    await page.getByRole('button', { name: '确认发送' }).click();
    await expect
      .poll(() => state.calls.some((c) => c.path === '/api/admin/broadcast' && c.method === 'POST'))
      .toBe(true);
    const call = state.calls.find((c) => c.path === '/api/admin/broadcast' && c.method === 'POST');
    expect((call?.body as { scheduledAt?: string } | undefined)?.scheduledAt).toBeTruthy();
  });

  test('broadcast saves a push draft', async ({ page }) => {
    const state = await mockAdminApi(page);
    await page.goto('/admin/broadcast');

    await page.getByPlaceholder('如：v1.4.3 更新公告').fill('草稿标题');
    await page.locator('textarea[placeholder="支持纯文本…"]').fill('草稿正文');
    await page.getByRole('button', { name: '存草稿' }).click();
    await expect
      .poll(() => state.calls.some((c) => c.path === '/api/admin/broadcast/draft' && c.method === 'POST'))
      .toBe(true);
  });

  test('updates page checks release and opens apply confirmation', async ({ page }) => {
    const state = await mockAdminApi(page);
    await page.goto('/admin/updates');

    await expect(page.getByText('当前', { exact: true })).toBeVisible();
    await page.getByRole('button', { name: '检查更新' }).click();
    // v1.2 升级主按钮文案为「一键升级到 X」
    await page.getByRole('button', { name: /一键升级到 0\.4\.4/ }).click();
    await expect(page.getByRole('heading', { name: '确认一键更新' })).toBeVisible();
    await expect.poll(() => state.calls.some((c) => c.path === '/api/admin/updates/check')).toBe(true);
  });

  test('AMAS config edits, discards, saves, and hot reloads', async ({ page }) => {
    const state = await mockAdminApi(page);
    await page.goto('/admin/amas-config');

    await expect(page.getByRole('heading', { name: 'AMAS 调参' })).toBeVisible();
    // v1.2 参数为滑块；用「一键预设」按钮套用使配置 dirty（真实 onClick，稳定可靠）
    // dirty 角标在页头与预设卡各出现一次，取 first 收敛
    const dirtyBadge = page.getByText('有未保存修改').first();
    await page.getByRole('button', { name: /激进/ }).click();
    await expect(dirtyBadge).toBeVisible();
    // 「回滚到 baseline」直接重置，无二次确认
    await page.getByRole('button', { name: '回滚到 baseline' }).click();
    await expect(dirtyBadge).toBeHidden();
    await page.getByRole('button', { name: /激进/ }).click();
    await expect(dirtyBadge).toBeVisible();
    // 保存：触发「保存为新版本」→ Confirm（title 同名）→ 确认按钮「保存」。
    // dispatchEvent 直接在弹窗内按钮上触发 click，绕过外层 grid 对指针坐标的拦截。
    await page.getByRole('button', { name: '保存为新版本' }).click();
    await expect(page.getByRole('heading', { name: '保存为新版本' })).toBeVisible();
    await page.locator('.scale-in').getByRole('button', { name: '保存', exact: true }).dispatchEvent('click');
    await expect.poll(() => state.calls.some((c) => c.method === 'PUT' && c.path === '/api/admin/amas/config')).toBe(true);
    await expect(dirtyBadge).toBeHidden();
    // 保存后 baseline 已含「激进」补丁；改套「稳妥」预设再次 dirty，触发热重载
    await page.getByRole('button', { name: /稳妥/ }).click();
    await expect(dirtyBadge).toBeVisible();
    await page.getByRole('button', { name: '热重载', exact: true }).first().click();
    await expect(page.getByRole('heading', { name: '热重载配置' })).toBeVisible();
    await page.locator('.scale-in').getByRole('button', { name: '热重载', exact: true }).dispatchEvent('click');
    await expect.poll(() => state.calls.some((c) => c.path === '/api/admin/settings/reload-amas')).toBe(true);
  });

  test('AMAS config version drawer expands and restores an older version', async ({ page }) => {
    const state = await mockAdminApi(page);
    await page.goto('/admin/amas-config');

    await page.getByRole('button', { name: '版本历史', exact: true }).click();
    await expect(page.getByText('previous config')).toBeVisible();
    // 抽屉内仅非 live 版本(abc123older)有「回滚到此」→ Confirm「回滚到此版本」→ 确认按钮「回滚」
    await page.getByRole('button', { name: '回滚到此' }).click();
    await page.getByRole('button', { name: '回滚', exact: true }).click();
    await expect.poll(() => state.calls.some((c) => c.path === '/api/admin/amas/config/versions/abc123older/restore')).toBe(true);
  });

  test('AMAS metrics renders all dashboard panels', async ({ page }) => {
    await mockAdminApi(page);
    await page.goto('/admin/amas-metrics');

    // v1.2 AMAS 指标合并为单页（无 tab）：PageHead + 多个 Panel
    await expect(page.getByRole('heading', { name: 'AMAS 指标', level: 1 })).toBeVisible();
    await expect(page.getByRole('heading', { name: '每用户决策数分布' })).toBeVisible();
    await expect(page.getByRole('heading', { name: '异常检测' })).toBeVisible();
    await expect(page.getByRole('heading', { name: '版本对比' })).toBeVisible();
    // 版本对比面板 A/B 选择器
    await expect(page.getByLabel('版本 A')).toBeVisible();
    await expect(page.getByLabel('版本 B')).toBeVisible();
  });

  test('AMAS advisor approves and rejects a pending suggestion', async ({ page }) => {
    const state = await mockAdminApi(page);
    await page.goto('/admin/amas-advisor');

    // v1.1.4 LLM 顾问:SuggestionCard 的「批准并应用」「拒绝」直接调 API（无二次确认弹窗）。
    // 待审区只有一条建议（id 7,mock 保持返回故批准后仍在列表),用 .first() 收敛到该卡。
    await expect(page.getByText('提高近期留存稳定性')).toBeVisible();
    await page.getByRole('button', { name: '批准并应用' }).first().click();
    await expect.poll(() => state.calls.some((c) => c.path === '/api/admin/amas/suggestions/7/approve')).toBe(true);
    await page.getByRole('button', { name: '拒绝' }).first().click();
    await expect.poll(() => state.calls.some((c) => c.path === '/api/admin/amas/suggestions/7/reject')).toBe(true);
  });
});
