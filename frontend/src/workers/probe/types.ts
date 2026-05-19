/**
 * 远程探针客户端协议类型 + ctx schema 版本常量。
 *
 * `CLIENT_CTX_VERSION` 必须与后端 `PROBE_CTX_VERSION_LATEST`（src/routes/admin/probe.rs）
 * 同步。每次 ctx 字段或方法集发生变化 → 两端同时 +1。
 */

export const CLIENT_CTX_VERSION = 1;

/** Worker 收到的入参（来自主线程 postMessage）。 */
export interface WorkerInput {
  script: string;
  ctx: MinimalCtx;
  /** 主线程注入，Worker 不消费——保留供未来的 confirmed 重跑标记。 */
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

/** D 类受控写动作记录（M1 中 cmd stub 为 no-op，actions 始终为空）。 */
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

/**
 * M1 最小 ctx：仅 nav.ua + time.now。
 * M2 扩展为完整 schema（A 环境 + B 应用状态 + C 诊断）。
 * M3 加 cmd stub。
 */
export interface MinimalCtx {
  nav: {
    ua: string;
    language: string;
    platform: string;
  };
  time: {
    now: number;
  };
}
