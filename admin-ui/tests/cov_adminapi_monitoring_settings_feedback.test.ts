import { describe, it, expect, vi, beforeAll, afterAll, afterEach } from 'vitest';
import { setupServer } from 'msw/node';
import { http, HttpResponse } from 'msw';

import { TEST_BASE_URL as BASE } from './helpers/constants';

const server = setupServer();
beforeAll(() => server.listen({ onUnhandledRequest: 'bypass' }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

vi.mock('@/lib/token', () => ({
  tokenManager: {
    getToken: () => null,
    getAdminToken: () => 'fake-admin-token',
    setTokens: vi.fn(),
    clearTokens: vi.fn(),
    clearAdminToken: vi.fn(),
    needsRefresh: () => false,
    isAuthenticated: () => false,
    setAdminToken: vi.fn(),
    refreshAccessToken: vi.fn(),
  },
}));
vi.mock('@/api/auth', () => ({ authApi: { refresh: vi.fn() } }));

import { adminApi } from '@/api/admin';

// 统一 envelope 帮手
const ok = (data: unknown) => HttpResponse.json({ success: true, data });

describe('adminApi monitoring / settings / feedback / updates / broadcast 覆盖', () => {
  // ─────────────────────────── Monitoring ───────────────────────────
  it('getHealth / getDatabase / checkUpdate 命中各端点', async () => {
    server.use(
      http.get(`${BASE}/api/admin/monitoring/health`, () => ok({ status: 'healthy', version: '1.1.4' })),
      http.get(`${BASE}/api/admin/monitoring/database`, () => ok({ sizeOnDisk: 1, tableCount: 2, tables: [] })),
      http.get(`${BASE}/api/admin/monitoring/check-update`, () => ok({ hasUpdate: false, latestVersion: '1.1.4' })),
    );
    expect(await adminApi.getHealth()).toMatchObject({ status: 'healthy' });
    expect(await adminApi.getDatabase()).toMatchObject({ tableCount: 2 });
    expect(await adminApi.checkUpdate()).toMatchObject({ hasUpdate: false });
  });

  it('monitoringWorkers 返回 workers 数组', async () => {
    server.use(
      http.get(`${BASE}/api/admin/monitoring/workers`, () => ok({ workers: [{ name: 'heartbeat', status: 'alive' }] })),
    );
    const r = await adminApi.monitoringWorkers();
    expect(r.workers[0]).toMatchObject({ name: 'heartbeat' });
  });

  it('monitoringRequests 默认 window=1h 与显式传值', async () => {
    server.use(
      http.get(`${BASE}/api/admin/monitoring/requests`, ({ request }) =>
        ok({ window: new URL(request.url).searchParams.get('window') })),
    );
    expect(await adminApi.monitoringRequests()).toEqual({ window: '1h' });
    expect(await adminApi.monitoringRequests('24h')).toEqual({ window: '24h' });
  });

  it('monitoringLogs 默认 limit=200，可带 level', async () => {
    server.use(
      http.get(`${BASE}/api/admin/monitoring/logs`, ({ request }) => {
        const u = new URL(request.url);
        return ok({ logs: [{ limit: u.searchParams.get('limit'), level: u.searchParams.get('level') }] });
      }),
    );
    expect((await adminApi.monitoringLogs()).logs[0]).toEqual({ limit: '200', level: null });
    expect((await adminApi.monitoringLogs(50, 'error')).logs[0]).toEqual({ limit: '50', level: 'error' });
  });

  it('monitoringEvents 默认 hours=6', async () => {
    server.use(
      http.get(`${BASE}/api/admin/monitoring/events`, ({ request }) =>
        ok({ events: [{ hours: new URL(request.url).searchParams.get('hours') }] })),
    );
    expect((await adminApi.monitoringEvents()).events[0]).toEqual({ hours: '6' });
    expect((await adminApi.monitoringEvents(12)).events[0]).toEqual({ hours: '12' });
  });

  it('monitoringDeadLetter / Requeue / Purge 死信运维三件套', async () => {
    server.use(
      http.get(`${BASE}/api/admin/monitoring/dead-letter`, ({ request }) =>
        ok({ entries: [{ limit: new URL(request.url).searchParams.get('limit') }] })),
      http.post(`${BASE}/api/admin/monitoring/dead-letter/7/requeue`, () => ok({ requeued: true, id: 7 })),
      http.delete(`${BASE}/api/admin/monitoring/dead-letter/9`, () => ok({ purged: true, id: 9 })),
    );
    expect((await adminApi.monitoringDeadLetter()).entries[0]).toEqual({ limit: '100' });
    expect((await adminApi.monitoringDeadLetter(20)).entries[0]).toEqual({ limit: '20' });
    expect(await adminApi.monitoringDeadLetterRequeue(7)).toEqual({ requeued: true, id: 7 });
    expect(await adminApi.monitoringDeadLetterPurge(9)).toEqual({ purged: true, id: 9 });
  });

  // ─────────────────────────── Updates ───────────────────────────
  it('updatesStatus / updatesCheck 命中', async () => {
    server.use(
      http.get(`${BASE}/api/admin/updates/status`, () => ok({ currentVersion: 'v1.1.4' })),
      http.post(`${BASE}/api/admin/updates/check`, () => ok({ currentVersion: 'v1.1.4' })),
    );
    expect(await adminApi.updatesStatus()).toMatchObject({ currentVersion: 'v1.1.4' });
    expect(await adminApi.updatesCheck()).toMatchObject({ currentVersion: 'v1.1.4' });
  });

  it('updatesApply / updatesRollback 发送 channel+targetVersion+confirmCurrentVersion', async () => {
    let applyBody: Record<string, unknown> = {};
    let rollbackBody: Record<string, unknown> = {};
    server.use(
      http.post(`${BASE}/api/admin/updates/apply`, async ({ request }) => {
        applyBody = (await request.json()) as Record<string, unknown>;
        return ok({ taskId: 't1', phase: 'pending', percent: 0 });
      }),
      http.post(`${BASE}/api/admin/updates/rollback`, async ({ request }) => {
        rollbackBody = (await request.json()) as Record<string, unknown>;
        return ok({ taskId: 't2', phase: 'pending', percent: 0 });
      }),
    );
    expect(await adminApi.updatesApply('beta', 'v1.1.5-beta.1', 'v1.1.4')).toMatchObject({ taskId: 't1' });
    expect(applyBody).toEqual({ channel: 'beta', targetVersion: 'v1.1.5-beta.1', confirmCurrentVersion: 'v1.1.4' });
    expect(await adminApi.updatesRollback('stable', 'v1.1.0', 'v1.1.4')).toMatchObject({ taskId: 't2' });
    expect(rollbackBody).toEqual({ channel: 'stable', targetVersion: 'v1.1.0', confirmCurrentVersion: 'v1.1.4' });
  });

  it('updatesHistory 返回 entries', async () => {
    server.use(http.get(`${BASE}/api/admin/updates/history`, () => ok({ entries: [{ id: 1 }] })));
    expect((await adminApi.updatesHistory()).entries).toEqual([{ id: 1 }]);
  });

  it('updatesChangelog 仅在传 channel 时带 query', async () => {
    server.use(
      http.get(`${BASE}/api/admin/updates/changelog`, ({ request }) => {
        const u = new URL(request.url);
        return ok({ available: true, channel: u.searchParams.get('channel'), hasChannelParam: u.searchParams.has('channel') });
      }),
    );
    expect(await adminApi.updatesChangelog()).toMatchObject({ hasChannelParam: false });
    expect(await adminApi.updatesChangelog('beta')).toMatchObject({ channel: 'beta', hasChannelParam: true });
  });

  it('updatesBackups / updatesCreateBackup / updatesRestoreBackup', async () => {
    let restorePath = '';
    server.use(
      http.get(`${BASE}/api/admin/updates/backups`, () => ok({ backups: [], totalBytes: 0 })),
      http.post(`${BASE}/api/admin/updates/backups`, () => ok({ name: 'b1.db', sizeBytes: 100 })),
      http.post(`${BASE}/api/admin/updates/backups/:name/restore`, ({ request }) => {
        restorePath = new URL(request.url).pathname;
        return ok({ restored: true, restartRecommended: true, preRestoreBackup: 'pre.db' });
      }),
    );
    expect(await adminApi.updatesBackups()).toMatchObject({ totalBytes: 0 });
    expect(await adminApi.updatesCreateBackup()).toMatchObject({ name: 'b1.db' });
    // name 含特殊字符走 encodeURIComponent
    expect(await adminApi.updatesRestoreBackup('a b.db')).toMatchObject({ restored: true });
    expect(restorePath).toBe('/api/admin/updates/backups/a%20b.db/restore');
  });

  // ─────────────────────────── Broadcast ───────────────────────────
  it('listBroadcasts 透传 offset/limit/filter query', async () => {
    server.use(
      http.get(`${BASE}/api/admin/broadcast`, ({ request }) => {
        const u = new URL(request.url);
        return ok({
          stats: {}, broadcasts: [],
          pagination: {
            offset: u.searchParams.get('offset'),
            limit: u.searchParams.get('limit'),
            filter: u.searchParams.get('filter'),
          },
        });
      }),
    );
    const r = await adminApi.listBroadcasts({ offset: 20, limit: 10, filter: 'failed' });
    expect(r.pagination).toMatchObject({ offset: '20', limit: '10', filter: 'failed' });
  });

  it('broadcast 发送 title/message + 可选 audience/scheduledAt', async () => {
    let body: Record<string, unknown> = {};
    server.use(
      http.post(`${BASE}/api/admin/broadcast`, async ({ request }) => {
        body = (await request.json()) as Record<string, unknown>;
        return ok({ sent: 5, broadcastId: 'bc-1' });
      }),
    );
    const payload = {
      title: '更新',
      message: '请升级',
      audience: { platforms: ['web'], versionMin: '1.0.0', lastActiveDays: 7, userIds: ['u1'] },
      scheduledAt: '2026-07-01T00:00:00Z',
    };
    const r = await adminApi.broadcast(payload);
    expect(r).toMatchObject({ broadcastId: 'bc-1' });
    expect(body).toEqual(payload);
  });

  it('broadcastPreview 返回命中预估', async () => {
    let body: Record<string, unknown> = {};
    server.use(
      http.post(`${BASE}/api/admin/broadcast/preview`, async ({ request }) => {
        body = (await request.json()) as Record<string, unknown>;
        return ok({ matched: 3, total: 10 });
      }),
    );
    expect(await adminApi.broadcastPreview({ audience: { platforms: ['ios'] } })).toEqual({ matched: 3, total: 10 });
    expect(body).toEqual({ audience: { platforms: ['ios'] } });
  });

  it('getPushDraft / savePushDraft / deletePushDraft 推送草稿三件套', async () => {
    let saveBody: Record<string, unknown> = {};
    server.use(
      http.get(`${BASE}/api/admin/broadcast/draft`, () => ok({ draft: null })),
      http.post(`${BASE}/api/admin/broadcast/draft`, async ({ request }) => {
        saveBody = (await request.json()) as Record<string, unknown>;
        return ok({ draft: { title: 't', message: 'm' } });
      }),
      http.delete(`${BASE}/api/admin/broadcast/draft`, () => ok({ deleted: true })),
    );
    expect(await adminApi.getPushDraft()).toEqual({ draft: null });
    const r = await adminApi.savePushDraft({ title: 't', message: 'm', platforms: ['web'], versionMin: null, lastActiveDays: 3 });
    expect(r.draft).toMatchObject({ title: 't' });
    expect(saveBody).toEqual({ title: 't', message: 'm', platforms: ['web'], versionMin: null, lastActiveDays: 3 });
    expect(await adminApi.deletePushDraft()).toEqual({ deleted: true });
  });

  it('listScheduledBroadcasts / cancelScheduledBroadcast 定时队列', async () => {
    let cancelPath = '';
    server.use(
      http.get(`${BASE}/api/admin/broadcast/scheduled`, () => ok({ items: [{ id: 'sb-1' }] })),
      http.delete(`${BASE}/api/admin/broadcast/scheduled/:id`, ({ request }) => {
        cancelPath = new URL(request.url).pathname;
        return ok({ canceled: true });
      }),
    );
    expect((await adminApi.listScheduledBroadcasts()).items).toEqual([{ id: 'sb-1' }]);
    expect(await adminApi.cancelScheduledBroadcast('a/b')).toEqual({ canceled: true });
    expect(cancelPath).toBe('/api/admin/broadcast/scheduled/a%2Fb');
  });

  it('broadcastUpdate 传 payload 或缺省发空对象', async () => {
    const bodies: Record<string, unknown>[] = [];
    server.use(
      http.post(`${BASE}/api/admin/broadcast-update`, async ({ request }) => {
        bodies.push((await request.json()) as Record<string, unknown>);
        return ok({ broadcasted: true });
      }),
    );
    expect(await adminApi.broadcastUpdate({ version: '1.1.5', message: 'hi' })).toEqual({ broadcasted: true });
    await adminApi.broadcastUpdate();
    expect(bodies[0]).toEqual({ version: '1.1.5', message: 'hi' });
    expect(bodies[1]).toEqual({});
  });

  // ─────────────────────────── Settings ───────────────────────────
  it('getSettings / updateSettings round-trip', async () => {
    let putBody: Record<string, unknown> = {};
    server.use(
      http.get(`${BASE}/api/admin/settings`, () => ok({ maxUsers: 1000, registrationEnabled: true, maintenanceMode: false })),
      http.put(`${BASE}/api/admin/settings`, async ({ request }) => {
        putBody = (await request.json()) as Record<string, unknown>;
        return ok({ maxUsers: 1000, registrationEnabled: false, maintenanceMode: false });
      }),
    );
    expect(await adminApi.getSettings()).toMatchObject({ maxUsers: 1000 });
    expect(await adminApi.updateSettings({ registrationEnabled: false } as any)).toMatchObject({ registrationEnabled: false });
    expect(putBody).toEqual({ registrationEnabled: false });
  });

  it('setMaintenance 发送 active 布尔', async () => {
    let body: Record<string, unknown> = {};
    server.use(
      http.post(`${BASE}/api/admin/settings/maintenance`, async ({ request }) => {
        body = (await request.json()) as Record<string, unknown>;
        return ok({ active: true });
      }),
    );
    expect(await adminApi.setMaintenance(true)).toEqual({ active: true });
    expect(body).toEqual({ active: true });
  });

  it('getVersionGate / setVersionGate 版本门控读写', async () => {
    let body: Record<string, unknown> = {};
    server.use(
      http.get(`${BASE}/api/admin/settings/version-gate`, () => ok({ enabled: false, minClientVersion: null })),
      http.put(`${BASE}/api/admin/settings/version-gate`, async ({ request }) => {
        body = (await request.json()) as Record<string, unknown>;
        return ok({ enabled: true, minClientVersion: '1.1.0' });
      }),
    );
    expect(await adminApi.getVersionGate()).toMatchObject({ enabled: false });
    expect(await adminApi.setVersionGate({ enabled: true, minClientVersion: '1.1.0' })).toMatchObject({ enabled: true });
    expect(body).toEqual({ enabled: true, minClientVersion: '1.1.0' });
  });

  it('reloadAmas 透传 config 并回 config', async () => {
    const cfg = { featureFlags: { mdmEnabled: true } };
    let body: Record<string, unknown> = {};
    server.use(
      http.post(`${BASE}/api/admin/settings/reload-amas`, async ({ request }) => {
        body = (await request.json()) as Record<string, unknown>;
        return ok(cfg);
      }),
    );
    expect(await adminApi.reloadAmas(cfg as any)).toEqual(cfg);
    expect(body).toEqual(cfg);
  });

  it('settingsConfig / putSettingsSection 通用配置存储', async () => {
    let body: Record<string, unknown> = {};
    let sectionPath = '';
    server.use(
      http.get(`${BASE}/api/admin/settings/config`, () => ok({ sections: {} })),
      http.put(`${BASE}/api/admin/settings/config/:section`, async ({ request }) => {
        sectionPath = new URL(request.url).pathname;
        body = (await request.json()) as Record<string, unknown>;
        return ok({ section: 'smtp', json: body });
      }),
    );
    expect(await adminApi.settingsConfig()).toEqual({ sections: {} });
    const r = await adminApi.putSettingsSection('smtp', { host: 'mail' });
    expect(r).toMatchObject({ section: 'smtp' });
    expect(sectionPath).toBe('/api/admin/settings/config/smtp');
    expect(body).toEqual({ host: 'mail' });
  });

  it('settingsSnapshots / createSettingsSnapshot(label 与缺省) / restoreSettingsSnapshot', async () => {
    const bodies: Record<string, unknown>[] = [];
    let restorePath = '';
    server.use(
      http.get(`${BASE}/api/admin/settings/snapshots`, () => ok({ snapshots: [] })),
      http.post(`${BASE}/api/admin/settings/snapshots`, async ({ request }) => {
        bodies.push((await request.json()) as Record<string, unknown>);
        return ok({ id: 1, label: 'snap' });
      }),
      http.post(`${BASE}/api/admin/settings/snapshots/5/restore`, ({ request }) => {
        restorePath = new URL(request.url).pathname;
        return ok({ sections: {} });
      }),
    );
    expect(await adminApi.settingsSnapshots()).toEqual({ snapshots: [] });
    await adminApi.createSettingsSnapshot('snap');
    await adminApi.createSettingsSnapshot();
    expect(bodies[0]).toEqual({ label: 'snap' });
    expect(bodies[1]).toEqual({}); // 缺省 label 发空对象
    expect(await adminApi.restoreSettingsSnapshot(5)).toEqual({ sections: {} });
    expect(restorePath).toBe('/api/admin/settings/snapshots/5/restore');
  });

  it('getBackupStatus 离站备份目标状态', async () => {
    server.use(http.get(`${BASE}/api/admin/settings/backup-status`, () => ok({ targets: [{ name: 's3', healthy: true }] })));
    expect((await adminApi.getBackupStatus()).targets[0]).toMatchObject({ name: 's3' });
  });

  it('rbacAdmins / createRbacAdmin / updateRbacAdminRole / deleteRbacAdmin RBAC CRUD', async () => {
    let createBody: Record<string, unknown> = {};
    let roleBody: Record<string, unknown> = {};
    let deletePath = '';
    server.use(
      http.get(`${BASE}/api/admin/settings/admins`, () => ok({ admins: [] })),
      http.post(`${BASE}/api/admin/settings/admins`, async ({ request }) => {
        createBody = (await request.json()) as Record<string, unknown>;
        return ok({ id: 'a1', email: 'a@e', role: 'admin' });
      }),
      http.patch(`${BASE}/api/admin/settings/admins/a1/role`, async ({ request }) => {
        roleBody = (await request.json()) as Record<string, unknown>;
        return ok({ id: 'a1', role: 'super_admin' });
      }),
      http.delete(`${BASE}/api/admin/settings/admins/a2`, ({ request }) => {
        deletePath = new URL(request.url).pathname;
        return ok({ deleted: true });
      }),
    );
    expect(await adminApi.rbacAdmins()).toEqual({ admins: [] });
    await adminApi.createRbacAdmin({ email: 'a@e', password: 'pw', role: 'admin' as any });
    expect(createBody).toEqual({ email: 'a@e', password: 'pw', role: 'admin' });
    await adminApi.updateRbacAdminRole('a1', 'super_admin' as any);
    expect(roleBody).toEqual({ role: 'super_admin' });
    expect(await adminApi.deleteRbacAdmin('a2')).toEqual({ deleted: true });
    expect(deletePath).toBe('/api/admin/settings/admins/a2');
  });

  it('apiKeys / createApiKey / rotateApiKey / deleteApiKey API Key CRUD', async () => {
    let createBody: Record<string, unknown> = {};
    server.use(
      http.get(`${BASE}/api/admin/settings/api-keys`, () => ok({ keys: [] })),
      http.post(`${BASE}/api/admin/settings/api-keys`, async ({ request }) => {
        createBody = (await request.json()) as Record<string, unknown>;
        return ok({ id: 'k1', plaintext: 'sk-xxx' });
      }),
      http.post(`${BASE}/api/admin/settings/api-keys/k1/rotate`, () => ok({ id: 'k1', plaintext: 'sk-new' })),
      http.delete(`${BASE}/api/admin/settings/api-keys/k1`, () => ok({ revoked: true })),
    );
    expect(await adminApi.apiKeys()).toEqual({ keys: [] });
    await adminApi.createApiKey({ name: 'ci', scope: 'read' as any, expiresAt: '2026-12-31' });
    expect(createBody).toEqual({ name: 'ci', scope: 'read', expiresAt: '2026-12-31' });
    expect(await adminApi.rotateApiKey('k1')).toMatchObject({ plaintext: 'sk-new' });
    expect(await adminApi.deleteApiKey('k1')).toEqual({ revoked: true });
  });

  // ─────────────────────────── Feedback ───────────────────────────
  it('listFeedback 透传分页/筛选 query 并 unwrap data 字段', async () => {
    server.use(
      http.get(`${BASE}/api/admin/feedback`, ({ request }) => {
        const u = new URL(request.url);
        return ok({
          data: [{ id: 'fb-1' }],
          total: 1, page: Number(u.searchParams.get('page')), perPage: Number(u.searchParams.get('perPage')), totalPages: 1,
          echoStatus: u.searchParams.get('status'),
          echoUnread: u.searchParams.get('unread'),
          echoAssigned: u.searchParams.get('assigned'),
        });
      }),
    );
    const r = await adminApi.listFeedback({ page: 2, perPage: 10, status: 'open', unread: true, assigned: false });
    expect(r.data).toEqual([{ id: 'fb-1' }]);
    expect((r as any).echoStatus).toBe('open');
    expect((r as any).echoUnread).toBe('true');
    expect((r as any).echoAssigned).toBe('false');
  });

  it('updateFeedback PATCH 发送局部字段', async () => {
    let body: Record<string, unknown> = {};
    server.use(
      http.patch(`${BASE}/api/admin/feedback/fb-9`, async ({ request }) => {
        body = (await request.json()) as Record<string, unknown>;
        return ok({ id: 'fb-9', status: 'resolved' });
      }),
    );
    const r = await adminApi.updateFeedback('fb-9', { status: 'resolved', priority: 'high', assigneeAdminId: null, resolution: '已修' });
    expect(r).toMatchObject({ id: 'fb-9' });
    expect(body).toEqual({ status: 'resolved', priority: 'high', assigneeAdminId: null, resolution: '已修' });
  });

  it('getFeedbackStats / getFeedbackDetail', async () => {
    server.use(
      http.get(`${BASE}/api/admin/feedback/stats`, () => ok({ open: 3, resolved: 7 })),
      http.get(`${BASE}/api/admin/feedback/fb-1`, () => ok({ id: 'fb-1', replies: [], timeline: [] })),
    );
    expect(await adminApi.getFeedbackStats()).toMatchObject({ open: 3 });
    expect(await adminApi.getFeedbackDetail('fb-1')).toMatchObject({ id: 'fb-1' });
  });

  it('createFeedbackReply 发送 body/pushInapp', async () => {
    let body: Record<string, unknown> = {};
    server.use(
      http.post(`${BASE}/api/admin/feedback/fb-2/replies`, async ({ request }) => {
        body = (await request.json()) as Record<string, unknown>;
        return ok({ id: 'rp-1', body: '收到' });
      }),
    );
    const r = await adminApi.createFeedbackReply('fb-2', { body: '收到', pushInapp: true });
    expect(r).toMatchObject({ id: 'rp-1' });
    expect(body).toEqual({ body: '收到', pushInapp: true });
  });

  it('assignFeedback 分派与取消分派(null)', async () => {
    const bodies: Record<string, unknown>[] = [];
    server.use(
      http.post(`${BASE}/api/admin/feedback/fb-3/assign`, async ({ request }) => {
        bodies.push((await request.json()) as Record<string, unknown>);
        return ok({ id: 'fb-3' });
      }),
    );
    await adminApi.assignFeedback('fb-3', 'admin-1');
    await adminApi.assignFeedback('fb-3', null);
    expect(bodies[0]).toEqual({ assigneeAdminId: 'admin-1' });
    expect(bodies[1]).toEqual({ assigneeAdminId: null });
  });

  it('resolveFeedback(带 resolution 与缺省)', async () => {
    const bodies: Record<string, unknown>[] = [];
    server.use(
      http.post(`${BASE}/api/admin/feedback/fb-4/resolve`, async ({ request }) => {
        bodies.push((await request.json()) as Record<string, unknown>);
        return ok({ id: 'fb-4', status: 'resolved' });
      }),
    );
    await adminApi.resolveFeedback('fb-4', '已处理');
    await adminApi.resolveFeedback('fb-4');
    expect(bodies[0]).toEqual({ resolution: '已处理' });
    // 缺省 resolution → JSON.stringify({ resolution: undefined }) 序列化为 {}
    expect(bodies[1]).toEqual({});
  });

  it('mergeFeedback 发送 targetId', async () => {
    let body: Record<string, unknown> = {};
    server.use(
      http.post(`${BASE}/api/admin/feedback/fb-5/merge`, async ({ request }) => {
        body = (await request.json()) as Record<string, unknown>;
        return ok({ merged: true });
      }),
    );
    expect(await adminApi.mergeFeedback('fb-5', 'fb-target')).toEqual({ merged: true });
    expect(body).toEqual({ targetId: 'fb-target' });
  });

  it('createFeedbackGithubIssue 发送空对象 body', async () => {
    let body: Record<string, unknown> = {};
    server.use(
      http.post(`${BASE}/api/admin/feedback/fb-6/github-issue`, async ({ request }) => {
        body = (await request.json()) as Record<string, unknown>;
        return ok({ issueUrl: 'https://github.com/x/y/issues/1' });
      }),
    );
    expect(await adminApi.createFeedbackGithubIssue('fb-6')).toMatchObject({ issueUrl: expect.any(String) });
    expect(body).toEqual({});
  });

  it('markAllFeedbackRead 全部已读', async () => {
    server.use(http.post(`${BASE}/api/admin/feedback/mark-all-read`, () => ok({ updated: 4 })));
    expect(await adminApi.markAllFeedbackRead()).toEqual({ updated: 4 });
  });

  it('feedbackAnnouncements 透传 kind/published query + CRUD', async () => {
    let createBody: Record<string, unknown> = {};
    let patchBody: Record<string, unknown> = {};
    server.use(
      http.get(`${BASE}/api/admin/feedback/announcements`, ({ request }) => {
        const u = new URL(request.url);
        return ok({ data: [{ kind: u.searchParams.get('kind'), published: u.searchParams.get('published') }] });
      }),
      http.post(`${BASE}/api/admin/feedback/announcements`, async ({ request }) => {
        createBody = (await request.json()) as Record<string, unknown>;
        return ok({ id: 'an-1', title: 't' });
      }),
      http.patch(`${BASE}/api/admin/feedback/announcements/an-1`, async ({ request }) => {
        patchBody = (await request.json()) as Record<string, unknown>;
        return ok({ id: 'an-1', published: true });
      }),
      http.delete(`${BASE}/api/admin/feedback/announcements/an-1`, () => ok({ deleted: true })),
    );
    const list = await adminApi.feedbackAnnouncements({ kind: 'faq' as any, published: true });
    expect(list.data[0]).toEqual({ kind: 'faq', published: 'true' });
    await adminApi.createFeedbackAnnouncement({ title: 't', body: 'b', kind: 'notice' as any, published: false });
    expect(createBody).toEqual({ title: 't', body: 'b', kind: 'notice', published: false });
    await adminApi.updateFeedbackAnnouncement('an-1', { published: true });
    expect(patchBody).toEqual({ published: true });
    expect(await adminApi.deleteFeedbackAnnouncement('an-1')).toEqual({ deleted: true });
  });

  it('getFeedbackDraft / saveFeedbackDraft 工单回复草稿', async () => {
    let body: Record<string, unknown> = {};
    server.use(
      http.get(`${BASE}/api/admin/feedback/fb-7/draft`, () => ok({ draft: null })),
      http.post(`${BASE}/api/admin/feedback/fb-7/draft`, async ({ request }) => {
        body = (await request.json()) as Record<string, unknown>;
        return ok({ feedbackId: 'fb-7', body: '草稿正文' });
      }),
    );
    expect(await adminApi.getFeedbackDraft('fb-7')).toEqual({ draft: null });
    const r = await adminApi.saveFeedbackDraft('fb-7', { body: '草稿正文', pushInapp: false });
    expect(r).toMatchObject({ feedbackId: 'fb-7' });
    expect(body).toEqual({ body: '草稿正文', pushInapp: false });
  });

  // ─── 走原始 fetch 的下载/导出端点(覆盖 token 注入 + Blob 路径) ───
  it('updatesBackupDownloadUrl 走原始 fetch 取 Blob 生成 ObjectURL', async () => {
    let downloadPath = '';
    let authHeader: string | null = null;
    server.use(
      http.get(`${BASE}/api/admin/updates/backups/:name/download`, ({ request }) => {
        downloadPath = new URL(request.url).pathname;
        authHeader = request.headers.get('Authorization');
        return new HttpResponse(new Blob(['db-bytes']), { status: 200 });
      }),
    );
    const url = await adminApi.updatesBackupDownloadUrl('a b.db');
    expect(url.startsWith('blob:')).toBe(true);
    expect(downloadPath).toBe('/api/admin/updates/backups/a%20b.db/download');
    expect(authHeader).toBe('Bearer fake-admin-token');
  });

  it('feedbackCsvUrl 走原始 fetch 取 CSV Blob', async () => {
    let authHeader: string | null = null;
    server.use(
      http.get(`${BASE}/api/admin/feedback/export.csv`, ({ request }) => {
        authHeader = request.headers.get('Authorization');
        return new HttpResponse('id,body\n', { status: 200, headers: { 'Content-Type': 'text/csv' } });
      }),
    );
    const url = await adminApi.feedbackCsvUrl();
    expect(url.startsWith('blob:')).toBe(true);
    expect(authHeader).toBe('Bearer fake-admin-token');
  });

  it('exportSettingsToml 走原始 fetch 返回 text/plain', async () => {
    server.use(
      http.get(`${BASE}/api/admin/settings/export.toml`, () =>
        new HttpResponse('[smtp]\nhost = "mail"\n', { status: 200, headers: { 'Content-Type': 'text/plain' } })),
    );
    const text = await adminApi.exportSettingsToml();
    expect(text).toContain('[smtp]');
  });
});
