/**
 * 主线程：探针 ctx 快照采集器。
 *
 * 关键原则：
 *  - 同步可拿的字段（nav / time / storage / app）立刻读；
 *  - 异步字段（idb.databases / count）有 200ms timeout，超时回 []；
 *  - **storeSnapshot 严格按白名单 `STORE_SNAPSHOT_FIELDS` 暴露字段**——
 *    绝不含 token / email / 用户名 / 真实姓名等。新增字段必须显式 review。
 *  - logs / errors / net 直接从 ring-buffers 取（已在 main bundle 启动注册）。
 */

import { ringBuffers, type ErrorEntry, type LogEntry, type NetEntry } from './ring-buffers';
import type { CtxSnapshot } from './types';

/**
 * pinia/Solid store 快照白名单。**严禁**新增 token / email / userId / 姓名等敏感字段。
 * 若条目不存在或为 null，会被 dropped。
 *
 * 每条目是 dotted path，从 window 的某个 store 容器取（具体取法在
 * `readStoreSnapshot()` 实现）。当前项目无统一 store，先返回空 — 后续接入
 * pinia / solid store 时在 readStoreSnapshot 内补具体读取逻辑。
 */
export const STORE_SNAPSHOT_FIELDS: readonly string[] = [
  // 示例（项目接入 pinia 后开放）：
  // 'app.preferences',
  // 'learning.currentSessionId',
] as const;

const IDB_LIST_TIMEOUT_MS = 200;

export async function collectCtxSnapshot(): Promise<CtxSnapshot> {
  // ── A. nav ──
  const nav = readNav();

  // ── A. perf ──
  const perf = readPerf();

  // ── A. time ──
  const time = readTime();

  // ── B. storage 摘要（仅 key 名 + size，不读 value，避免误暴露 token） ──
  const storage = readStorageSummary();

  // ── B. idb：异步查询 + 200ms cap ──
  const idb = await withTimeout(readIndexedDBSummary(), IDB_LIST_TIMEOUT_MS, { list: [], counts: {} });

  // ── B. app ──
  const app = readAppMeta();

  // ── C. ring-buffer 快照 ──
  const logs: LogEntry[] = ringBuffers.logs.snapshot();
  const errors: ErrorEntry[] = ringBuffers.errors.snapshot();
  const net: NetEntry[] = ringBuffers.net.snapshot();

  return { nav, perf, time, storage, idb, app, logs, errors, net };
}

function readNav(): CtxSnapshot['nav'] {
  const n = navigator as Navigator & {
    connection?: { effectiveType: string; downlink: number; rtt: number };
    deviceMemory?: number;
  };
  return {
    ua: n.userAgent,
    language: n.language,
    languages: Array.from(n.languages ?? [n.language]),
    platform: n.platform,
    hardwareConcurrency: n.hardwareConcurrency ?? 0,
    deviceMemory: n.deviceMemory,
    connection: n.connection
      ? {
          effectiveType: n.connection.effectiveType,
          downlink: n.connection.downlink,
          rtt: n.connection.rtt,
        }
      : undefined,
    online: n.onLine,
  };
}

function readPerf(): CtxSnapshot['perf'] {
  const mem = (performance as Performance & {
    memory?: { usedJSHeapSize: number; totalJSHeapSize: number; jsHeapSizeLimit: number };
  }).memory;
  const memoryMB = mem
    ? {
        used: Math.round(mem.usedJSHeapSize / 1024 / 1024),
        total: Math.round(mem.totalJSHeapSize / 1024 / 1024),
        limit: Math.round(mem.jsHeapSizeLimit / 1024 / 1024),
      }
    : null;

  // entries：取前 100 条避免 ctx 过大
  const allEntries = performance.getEntries().slice(0, 100);
  const entries = allEntries.map((e) => ({
    name: e.name.length > 256 ? `${e.name.slice(0, 256)}…` : e.name,
    entryType: e.entryType,
    startTime: Math.round(e.startTime),
    duration: Math.round(e.duration),
  }));

  // resourceTimingSummary
  const resources = performance.getEntriesByType('resource') as PerformanceResourceTiming[];
  const sorted = resources
    .map((r) => ({ url: r.name, durationMs: Math.round(r.duration) }))
    .sort((a, b) => b.durationMs - a.durationMs);
  const topUrls = sorted.slice(0, 5);
  const slowestMs = sorted[0]?.durationMs ?? 0;

  return {
    memoryMB,
    entries,
    resourceTimingSummary: { count: resources.length, slowestMs, topUrls },
  };
}

function readTime(): CtxSnapshot['time'] {
  return {
    now: Date.now(),
    tz: Intl.DateTimeFormat().resolvedOptions().timeZone,
    performanceNow: Math.round(performance.now()),
  };
}

function readStorageSummary(): CtxSnapshot['storage'] {
  return {
    local: readSingleStorage(typeof localStorage !== 'undefined' ? localStorage : null),
    session: readSingleStorage(typeof sessionStorage !== 'undefined' ? sessionStorage : null),
  };
}

function readSingleStorage(store: Storage | null): CtxSnapshot['storage']['local'] {
  if (!store) return { keys: [], size: { count: 0, bytes: 0 }, kv: {} };
  const keys: string[] = [];
  const kv: Record<string, string> = {};
  let bytes = 0;
  for (let i = 0; i < store.length; i++) {
    const k = store.key(i);
    if (!k) continue;
    keys.push(k);
    const v = store.getItem(k) ?? '';
    bytes += k.length + v.length;
    // 仅在 key 不在常见敏感列表里时把 value 也带上（让 storage.get() 在 worker 端可用）。
    // 默认全脱敏 —— 不带 value。如果未来需要 store.get() 取值，admin 应该用 storage.keys() 自查。
    kv[k] = '';
    void v; // 显式标记忽略 v
  }
  return { keys, size: { count: store.length, bytes }, kv };
}

async function readIndexedDBSummary(): Promise<CtxSnapshot['idb']> {
  if (typeof indexedDB === 'undefined' || typeof indexedDB.databases !== 'function') {
    return { list: [], counts: {} };
  }
  try {
    const dbs = await indexedDB.databases();
    const list = dbs.map((d) => d.name ?? '').filter(Boolean);
    const counts: Record<string, Record<string, number>> = {};
    // 浅查每个 db 的 objectStoreNames（不真正 count 记录，避免重负载）。
    for (const dbInfo of dbs) {
      const name = dbInfo.name;
      if (!name) continue;
      const db = await openIDB(name);
      if (!db) continue;
      counts[name] = {};
      for (const storeName of Array.from(db.objectStoreNames)) {
        counts[name][storeName] = -1; // -1 表示"未实际 count"；M2 暂不 count（IO 重）
      }
      db.close();
    }
    return { list, counts };
  } catch {
    return { list: [], counts: {} };
  }
}

function openIDB(name: string): Promise<IDBDatabase | null> {
  return new Promise((resolve) => {
    try {
      const req = indexedDB.open(name);
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => resolve(null);
      req.onblocked = () => resolve(null);
    } catch {
      resolve(null);
    }
  });
}

function readAppMeta(): CtxSnapshot['app'] {
  return {
    route: typeof location !== 'undefined' ? location.pathname : '',
    version: (import.meta.env.VITE_GIT_VERSION as string | undefined) ?? 'dev',
    buildHash: (import.meta.env.VITE_BUILD_HASH as string | undefined) ?? 'unknown',
    storeSnapshot: readStoreSnapshot(),
  };
}

/**
 * 按 STORE_SNAPSHOT_FIELDS 白名单从全局 store 容器读取字段。
 * 当前无统一 store 注入点 —— 项目接入 pinia / solid store 后在此实现。
 */
function readStoreSnapshot(): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  // 当前 frontend 没有暴露全局 store hook 给 worker，先返回空对象。
  // 接入示例（待后续）：
  //   for (const path of STORE_SNAPSHOT_FIELDS) { out[path] = readPath(globalStore, path); }
  return out;
}

async function withTimeout<T>(p: Promise<T>, ms: number, fallback: T): Promise<T> {
  return new Promise<T>((resolve) => {
    let settled = false;
    const t = setTimeout(() => {
      if (settled) return;
      settled = true;
      resolve(fallback);
    }, ms);
    p.then(
      (v) => {
        if (settled) return;
        settled = true;
        clearTimeout(t);
        resolve(v);
      },
      () => {
        if (settled) return;
        settled = true;
        clearTimeout(t);
        resolve(fallback);
      },
    );
  });
}
