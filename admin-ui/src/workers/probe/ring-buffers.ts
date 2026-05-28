/**
 * 全局环形 buffer：bundle 启动时 installRingBuffers() 注册一次，写满循环覆盖。
 *
 * 三组：logs(200) / errors(50) / net(100)。容量固定，O(1) push + O(n) snapshot。
 * 探针 ctx 通过 .tail(n) / .recent(n) 取最近 N 条 snapshot。
 */

export interface LogEntry {
  level: 'log' | 'warn' | 'error' | 'info' | 'debug';
  ts: number;
  message: string;
}

export interface ErrorEntry {
  ts: number;
  message: string;
  stack?: string;
  source?: string;
  lineNo?: number;
  colNo?: number;
}

export interface NetEntry {
  ts: number;
  url: string;
  method: string;
  status: number;
  durationMs: number;
}

class CircularBuffer<T> {
  private buf: T[] = [];
  private idx = 0;
  constructor(private readonly cap: number) {}

  push(item: T): void {
    if (this.buf.length < this.cap) {
      this.buf.push(item);
    } else {
      this.buf[this.idx] = item;
      this.idx = (this.idx + 1) % this.cap;
    }
  }

  /** 按时间顺序返回快照（旧→新）。每次 snapshot 都是浅拷贝，不与内部共享引用。 */
  snapshot(): T[] {
    if (this.buf.length < this.cap) {
      return this.buf.slice();
    }
    return [...this.buf.slice(this.idx), ...this.buf.slice(0, this.idx)];
  }

  /** 取最后 n 条（旧→新）。 */
  tail(n: number): T[] {
    const all = this.snapshot();
    if (n >= all.length) return all;
    return all.slice(all.length - n);
  }

  clear(): void {
    this.buf = [];
    this.idx = 0;
  }
}

export const ringBuffers = {
  logs: new CircularBuffer<LogEntry>(200),
  errors: new CircularBuffer<ErrorEntry>(50),
  net: new CircularBuffer<NetEntry>(100),
};

let installed = false;
let originals: {
  log?: typeof console.log;
  warn?: typeof console.warn;
  error?: typeof console.error;
  info?: typeof console.info;
  debug?: typeof console.debug;
  fetch?: typeof window.fetch;
} = {};
// 保留 listener 引用以便 uninstall 时 removeEventListener。
let errorListener: ((ev: ErrorEvent) => void) | undefined;
let rejectionListener: ((ev: PromiseRejectionEvent) => void) | undefined;

function safeStringify(arg: unknown): string {
  if (typeof arg === 'string') return arg;
  if (arg instanceof Error) return arg.stack ?? arg.message;
  try {
    return JSON.stringify(arg);
  } catch {
    return String(arg);
  }
}

function buildLogPusher(level: LogEntry['level'], orig: (...args: unknown[]) => void) {
  return (...args: unknown[]) => {
    ringBuffers.logs.push({
      level,
      ts: Date.now(),
      message: args.map(safeStringify).join(' ').slice(0, 1024),
    });
    orig.apply(console, args);
  };
}

/**
 * 启动时注册：拦截 console、window error、unhandledrejection、fetch。
 * 幂等：重复调用是 no-op。
 *
 * `captureConsole=false` 可禁用 console hook（如果嫌噪声大）；默认开。
 */
export function installRingBuffers(opts?: { captureConsole?: boolean }): void {
  if (installed) return;
  installed = true;
  const captureConsole = opts?.captureConsole !== false;

  // console hooks
  if (captureConsole && typeof console !== 'undefined') {
    originals.log = console.log;
    originals.warn = console.warn;
    originals.error = console.error;
    originals.info = console.info;
    originals.debug = console.debug;
    console.log = buildLogPusher('log', originals.log.bind(console));
    console.warn = buildLogPusher('warn', originals.warn.bind(console));
    console.error = buildLogPusher('error', originals.error.bind(console));
    console.info = buildLogPusher('info', originals.info.bind(console));
    console.debug = buildLogPusher('debug', originals.debug.bind(console));
  }

  // error hooks
  if (typeof window !== 'undefined') {
    errorListener = (ev: ErrorEvent) => {
      ringBuffers.errors.push({
        ts: Date.now(),
        message: String(ev.message ?? 'unknown error'),
        stack: ev.error instanceof Error ? ev.error.stack : undefined,
        source: ev.filename,
        lineNo: ev.lineno,
        colNo: ev.colno,
      });
    };
    rejectionListener = (ev: PromiseRejectionEvent) => {
      const reason = ev.reason;
      ringBuffers.errors.push({
        ts: Date.now(),
        message: reason instanceof Error ? reason.message : safeStringify(reason),
        stack: reason instanceof Error ? reason.stack : undefined,
      });
    };
    window.addEventListener('error', errorListener);
    window.addEventListener('unhandledrejection', rejectionListener);
  }

  // fetch wrap
  if (typeof window !== 'undefined' && typeof window.fetch === 'function') {
    originals.fetch = window.fetch.bind(window);
    window.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
      const start = performance.now();
      const url = typeof input === 'string' ? input : input instanceof URL ? input.toString() : input.url;
      const method = init?.method ?? (typeof input === 'object' && 'method' in input ? input.method : 'GET');
      try {
        const res = await originals.fetch!(input, init);
        ringBuffers.net.push({
          ts: Date.now(),
          url,
          method,
          status: res.status,
          durationMs: Math.round(performance.now() - start),
        });
        return res;
      } catch (err) {
        ringBuffers.net.push({
          ts: Date.now(),
          url,
          method,
          status: 0, // 网络层 fail
          durationMs: Math.round(performance.now() - start),
        });
        throw err;
      }
    };
  }
}

/** 测试 / 卸载用：恢复 console 与 fetch、移除 error listener；清空 buffer。 */
export function uninstallRingBuffers(): void {
  if (!installed) return;
  if (originals.log) console.log = originals.log;
  if (originals.warn) console.warn = originals.warn;
  if (originals.error) console.error = originals.error;
  if (originals.info) console.info = originals.info;
  if (originals.debug) console.debug = originals.debug;
  if (originals.fetch) window.fetch = originals.fetch;
  if (errorListener) window.removeEventListener('error', errorListener);
  if (rejectionListener) window.removeEventListener('unhandledrejection', rejectionListener);
  errorListener = undefined;
  rejectionListener = undefined;
  originals = {};
  ringBuffers.logs.clear();
  ringBuffers.errors.clear();
  ringBuffers.net.clear();
  installed = false;
}
