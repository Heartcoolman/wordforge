/**
 * buildCtx 单测：把 CtxSnapshot 值对象包装成 Ctx 方法对象，验证：
 *  - 函数返回值与 snapshot 一致
 *  - cmd.* 仅 push 到 actions 数组（M2 中主线程不消费）
 *  - storage.get/keys/size 路由到正确 which
 *  - perf.entries filter + limit 工作正常
 */

import { describe, it, expect } from 'vitest';
import { buildCtx } from '@/workers/probe/build-ctx';
import type { CtxSnapshot, ProbeAction } from '@/workers/probe/types';

function makeSnapshot(): CtxSnapshot {
  return {
    nav: {
      ua: 'TestUA/1.0',
      language: 'zh-CN',
      languages: ['zh-CN', 'en'],
      platform: 'MacIntel',
      hardwareConcurrency: 8,
      deviceMemory: 8,
      online: true,
    },
    perf: {
      memoryMB: { used: 50, total: 100, limit: 2048 },
      entries: [
        { name: 'navigationStart', entryType: 'navigation', startTime: 0, duration: 1234 },
        { name: 'first-paint', entryType: 'paint', startTime: 100, duration: 0 },
        { name: 'slow-res', entryType: 'resource', startTime: 200, duration: 1500 },
      ],
      resourceTimingSummary: { count: 1, slowestMs: 1500, topUrls: [{ url: 'slow-res', durationMs: 1500 }] },
    },
    time: { now: 1716163200000, tz: 'Asia/Shanghai', performanceNow: 1000 },
    storage: {
      local: { keys: ['k1', 'k2'], size: { count: 2, bytes: 30 }, kv: { k1: 'v1', k2: 'v2' } },
      session: { keys: ['s1'], size: { count: 1, bytes: 4 }, kv: { s1: 'sv' } },
    },
    idb: { list: ['db-a', 'db-b'], counts: { 'db-a': { store1: 100 } } },
    app: {
      route: '/admin/probe',
      version: '0.5.6',
      buildHash: 'abc123',
      storeSnapshot: { stub: true },
    },
    logs: [
      { level: 'log', ts: 1, message: 'l1' },
      { level: 'warn', ts: 2, message: 'l2' },
      { level: 'error', ts: 3, message: 'l3' },
    ],
    errors: [{ ts: 1, message: 'oops' }],
    net: [{ ts: 1, url: '/api/x', method: 'GET', status: 200, durationMs: 42 }],
  };
}

describe('buildCtx', () => {
  it('nav / time 字段直接来自 snapshot', () => {
    const ctx = buildCtx(makeSnapshot(), []);
    expect(ctx.nav.ua).toBe('TestUA/1.0');
    expect(ctx.nav.language).toBe('zh-CN');
    expect(ctx.time.tz).toBe('Asia/Shanghai');
  });

  it('perf.memoryMB() 返回快照对象', () => {
    const ctx = buildCtx(makeSnapshot(), []);
    expect(ctx.perf.memoryMB()).toEqual({ used: 50, total: 100, limit: 2048 });
  });

  it('perf.entries filter by type + limit', () => {
    const ctx = buildCtx(makeSnapshot(), []);
    expect(ctx.perf.entries({ type: 'resource' })).toHaveLength(1);
    expect(ctx.perf.entries({ limit: 2 })).toHaveLength(2);
    expect(ctx.perf.entries()).toHaveLength(3);
  });

  it('storage.keys/get/size 区分 local 与 session', () => {
    const ctx = buildCtx(makeSnapshot(), []);
    expect(ctx.storage.keys('local')).toEqual(['k1', 'k2']);
    expect(ctx.storage.keys('session')).toEqual(['s1']);
    expect(ctx.storage.get('k1', 'local')).toBe('v1');
    expect(ctx.storage.get('missing')).toBeNull();
    expect(ctx.storage.size('session').count).toBe(1);
  });

  it('idb.list/count', () => {
    const ctx = buildCtx(makeSnapshot(), []);
    expect(ctx.idb.list()).toEqual(['db-a', 'db-b']);
    expect(ctx.idb.count('db-a', 'store1')).toBe(100);
    expect(ctx.idb.count('db-x', 'nope')).toBe(0);
  });

  it('logs.tail / errors.recent / net.recent', () => {
    const ctx = buildCtx(makeSnapshot(), []);
    expect(ctx.logs.tail(2).map((l) => l.message)).toEqual(['l2', 'l3']);
    expect(ctx.errors.recent()).toHaveLength(1);
    expect(ctx.net.recent()[0].url).toBe('/api/x');
  });

  it('cmd.* 只 push actions，不立即执行', () => {
    const actions: ProbeAction[] = [];
    const ctx = buildCtx(makeSnapshot(), actions);
    ctx.cmd.reload();
    ctx.cmd.clearCache();
    ctx.cmd.signOut();
    expect(actions).toEqual([
      { type: 'reload' },
      { type: 'clearCache' },
      { type: 'signOut' },
    ]);
  });

  it('storeSnapshot 函数返回白名单后的对象', () => {
    const ctx = buildCtx(makeSnapshot(), []);
    expect(ctx.app.storeSnapshot()).toEqual({ stub: true });
  });
});
