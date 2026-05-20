/**
 * collectCtxSnapshot 单测：
 *  - happy-dom 提供 navigator / performance / localStorage / sessionStorage
 *  - 验 nav / time / storage / app 字段齐全
 *  - 验 ring-buffers 已注册时 logs/errors/net 能跟着流转
 *  - 验 STORE_SNAPSHOT_FIELDS 默认空白名单 → storeSnapshot 返回 {}
 *  - 验 idb.list 缺失环境时降级为 []
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { collectCtxSnapshot } from '@/workers/probe/ctx-factory';
import { installRingBuffers, uninstallRingBuffers, ringBuffers } from '@/workers/probe/ring-buffers';

describe('collectCtxSnapshot', () => {
  beforeEach(() => {
    uninstallRingBuffers();
    if (typeof localStorage !== 'undefined') localStorage.clear();
    if (typeof sessionStorage !== 'undefined') sessionStorage.clear();
  });

  it('nav 字段齐全', async () => {
    const snap = await collectCtxSnapshot();
    expect(typeof snap.nav.ua).toBe('string');
    expect(typeof snap.nav.language).toBe('string');
    expect(snap.nav.languages).toBeInstanceOf(Array);
    expect(typeof snap.nav.hardwareConcurrency).toBe('number');
    expect(typeof snap.nav.online).toBe('boolean');
  });

  it('time 含 now / tz / performanceNow', async () => {
    const snap = await collectCtxSnapshot();
    expect(typeof snap.time.now).toBe('number');
    expect(typeof snap.time.tz).toBe('string');
    expect(typeof snap.time.performanceNow).toBe('number');
  });

  it('storage 摘要包含 key 名但 kv 值不暴露（脱敏）', async () => {
    localStorage.setItem('user_token', 'super_secret');
    localStorage.setItem('preferences', '{"theme":"dark"}');
    const snap = await collectCtxSnapshot();
    expect(snap.storage.local.keys).toContain('user_token');
    expect(snap.storage.local.keys).toContain('preferences');
    expect(snap.storage.local.size.count).toBeGreaterThanOrEqual(2);
    // kv 在 ctx-factory 里被强制置空字符串，admin 拿不到敏感 value
    expect(snap.storage.local.kv['user_token']).toBe('');
    expect(snap.storage.local.kv['preferences']).toBe('');
  });

  it('app 元信息携带 route / version / buildHash / storeSnapshot', async () => {
    const snap = await collectCtxSnapshot();
    expect(typeof snap.app.route).toBe('string');
    expect(typeof snap.app.version).toBe('string');
    expect(typeof snap.app.buildHash).toBe('string');
    expect(typeof snap.app.storeSnapshot).toBe('object');
    // 默认空白名单 → 不带敏感字段
    expect(snap.app.storeSnapshot).toEqual({});
  });

  it('logs / errors / net 反映 ring-buffer 流转', async () => {
    installRingBuffers({ captureConsole: false });
    ringBuffers.logs.push({ level: 'log', ts: Date.now(), message: 'hello' });
    ringBuffers.errors.push({ ts: Date.now(), message: 'boom' });
    ringBuffers.net.push({ ts: Date.now(), url: '/api/y', method: 'GET', status: 200, durationMs: 5 });
    const snap = await collectCtxSnapshot();
    expect(snap.logs[0].message).toBe('hello');
    expect(snap.errors[0].message).toBe('boom');
    expect(snap.net[0].url).toBe('/api/y');
  });

  it('idb.databases 缺失时降级 list 为 []', async () => {
    const snap = await collectCtxSnapshot();
    // happy-dom 不支持 indexedDB.databases() → 200ms 内 fallback []
    expect(snap.idb.list).toEqual([]);
  });

  it('STORE_SNAPSHOT_FIELDS 默认空 → storeSnapshot 返回空对象', async () => {
    const snap = await collectCtxSnapshot();
    expect(Object.keys(snap.app.storeSnapshot).length).toBe(0);
  });
});
