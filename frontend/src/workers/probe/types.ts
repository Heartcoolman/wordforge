/**
 * 远程探针客户端协议类型 + ctx schema 版本常量。
 *
 * `CLIENT_CTX_VERSION` 必须与后端 `PROBE_CTX_VERSION_LATEST`（src/routes/admin/probe.rs）
 * 同步。每次 ctx 字段或方法集发生变化 → 两端同时 +1。
 */

import type { ErrorEntry, LogEntry, NetEntry } from './ring-buffers';

export const CLIENT_CTX_VERSION = 1;

/** Worker 收到的入参（来自主线程 postMessage）。 */
export interface WorkerInput {
  script: string;
  snapshot: CtxSnapshot;
  /** 主线程注入：confirmed=true 时表示这是二次确认后的重跑（M3 用）。 */
  confirmed?: boolean;
}

/** Worker 回传给主线程的结果。 */
export type WorkerOutput =
  | {
      ok: true;
      result: unknown;
      actions: ProbeAction[];
      durationMs: number;
    }
  | {
      ok: false;
      stderr: string;
      durationMs: number;
    };

/** D 类受控写动作记录（M3 才接入主线程执行；M2 中 stub 仍只 push 不消费）。 */
export type ProbeAction =
  | { type: 'reload' }
  | { type: 'clearCache' }
  | { type: 'signOut' };

/** 客户端回传给后端的 result payload（与 `src/routes/probe_results.rs::ResultBody` 对齐）。 */
export interface ResultPayload {
  requestId: string;
  status:
    | 'ok'
    | 'error'
    | 'timeout'
    | 'confirm_required'
    | 'unsupported_ctx_version';
  resultJson?: unknown;
  stderr?: string;
  durationMs: number;
  truncated: boolean;
  confirmToken?: string;
}

/** ctx 快照：主线程一次性采集，序列化进 Worker，无 live 副作用。 */
export interface CtxSnapshot {
  // A. 环境
  nav: {
    ua: string;
    language: string;
    languages: string[];
    platform: string;
    hardwareConcurrency: number;
    deviceMemory?: number;
    connection?: {
      effectiveType: string;
      downlink: number;
      rtt: number;
    };
    online: boolean;
  };
  perf: {
    memoryMB: { used: number; total: number; limit: number } | null;
    entries: Array<{ name: string; entryType: string; startTime: number; duration: number }>;
    resourceTimingSummary: {
      count: number;
      slowestMs: number;
      topUrls: Array<{ url: string; durationMs: number }>;
    };
  };
  time: {
    now: number;
    tz: string;
    performanceNow: number;
  };
  // B. 应用状态
  storage: {
    local: { keys: string[]; size: { count: number; bytes: number }; kv: Record<string, string> };
    session: { keys: string[]; size: { count: number; bytes: number }; kv: Record<string, string> };
  };
  idb: {
    list: string[];
    counts: Record<string, Record<string, number>>; // db → store → count
  };
  app: {
    route: string;
    version: string;
    buildHash: string;
    storeSnapshot: Record<string, unknown>;
  };
  // C. 诊断
  logs: LogEntry[];
  errors: ErrorEntry[];
  net: NetEntry[];
}

/** Ctx 是 Worker 内部呈现给 script 的对象（方法化）。CtxSnapshot 是序列化值。 */
export interface Ctx {
  nav: CtxSnapshot['nav'];
  perf: {
    memoryMB: () => CtxSnapshot['perf']['memoryMB'];
    entries: (filter?: { type?: string; limit?: number }) => CtxSnapshot['perf']['entries'];
    resourceTimingSummary: () => CtxSnapshot['perf']['resourceTimingSummary'];
  };
  time: CtxSnapshot['time'];
  storage: {
    keys: (which?: 'local' | 'session') => string[];
    get: (key: string, which?: 'local' | 'session') => string | null;
    size: (which?: 'local' | 'session') => { count: number; bytes: number };
  };
  idb: {
    list: () => string[];
    count: (db: string, store: string) => number;
  };
  app: {
    route: string;
    version: string;
    buildHash: string;
    storeSnapshot: () => Record<string, unknown>;
  };
  logs: { tail: (n?: number) => LogEntry[] };
  errors: { recent: (n?: number) => ErrorEntry[] };
  net: { recent: (n?: number) => NetEntry[] };
  cmd: {
    reload: () => void;
    clearCache: () => void;
    signOut: () => void;
  };
}
