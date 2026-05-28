/**
 * 从 CtxSnapshot 构造 Worker 内呈现给 script 的 Ctx 方法对象。
 * 提取到独立模块以便 vitest 在主线程直接 unit-test（不依赖 Worker 环境）。
 */

import type { Ctx, CtxSnapshot, ProbeAction } from './types';

export function buildCtx(snapshot: CtxSnapshot, actions: ProbeAction[]): Ctx {
  const { storage } = snapshot;

  return {
    nav: snapshot.nav,
    perf: {
      memoryMB: () => snapshot.perf.memoryMB,
      entries: (filter) => {
        let list = snapshot.perf.entries;
        if (filter?.type) list = list.filter((e) => e.entryType === filter.type);
        if (filter?.limit && filter.limit > 0) list = list.slice(0, filter.limit);
        return list;
      },
      resourceTimingSummary: () => snapshot.perf.resourceTimingSummary,
    },
    time: snapshot.time,
    storage: {
      keys: (which = 'local') => storage[which].keys,
      get: (key, which = 'local') => storage[which].kv[key] ?? null,
      size: (which = 'local') => storage[which].size,
    },
    idb: {
      list: () => snapshot.idb.list,
      count: (db, store) => snapshot.idb.counts[db]?.[store] ?? 0,
    },
    app: {
      route: snapshot.app.route,
      version: snapshot.app.version,
      buildHash: snapshot.app.buildHash,
      storeSnapshot: () => snapshot.app.storeSnapshot,
    },
    logs: { tail: (n = 50) => snapshot.logs.slice(-n) },
    errors: { recent: (n = 50) => snapshot.errors.slice(-n) },
    net: { recent: (n = 50) => snapshot.net.slice(-n) },
    cmd: {
      reload() {
        actions.push({ type: 'reload' });
      },
      clearCache() {
        actions.push({ type: 'clearCache' });
      },
      signOut() {
        actions.push({ type: 'signOut' });
      },
    },
  };
}
