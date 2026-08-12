import { describe, it, expect, beforeAll, afterAll, beforeEach } from 'vitest';
import type { CtxSnapshot, WorkerInput, WorkerOutput } from '@/workers/probe/types';

function makeSnapshot(): CtxSnapshot {
  return {
    nav: {
      ua: 'TestUA/1.0',
      language: 'zh-CN',
      languages: ['zh-CN', 'en'],
      platform: 'MacIntel',
      hardwareConcurrency: 8,
      online: true,
    },
    perf: {
      memoryMB: { used: 50, total: 100, limit: 2048 },
      entries: [
        { name: 'nav', entryType: 'navigation', startTime: 0, duration: 12 },
        { name: 'res', entryType: 'resource', startTime: 1, duration: 34 },
      ],
      resourceTimingSummary: { count: 1, slowestMs: 34, topUrls: [{ url: 'res', durationMs: 34 }] },
    },
    time: { now: 1716163200000, tz: 'Asia/Shanghai', performanceNow: 1000 },
    storage: {
      local: { keys: ['k1'], size: { count: 1, bytes: 4 }, kv: { k1: 'v1' } },
      session: { keys: ['s1'], size: { count: 1, bytes: 4 }, kv: { s1: 'sv' } },
    },
    idb: { list: ['db-a'], counts: { 'db-a': { store1: 7 } } },
    app: { route: '/admin/probe', version: '0.5.6', buildHash: 'abc123', storeSnapshot: { stub: true } },
    logs: [
      { level: 'log', ts: 1, message: 'l1' },
      { level: 'error', ts: 2, message: 'l2' },
    ],
    errors: [{ ts: 1, message: 'oops' }],
    net: [{ ts: 1, url: '/api/x', method: 'GET', status: 200, durationMs: 42 }],
  };
}

const posted: WorkerOutput[] = [];
const realPostMessage = self.postMessage;
let handler: NonNullable<typeof self.onmessage>;

beforeAll(async () => {
  // Worker 里 `self.postMessage` 在 happy-dom 下就是 window.postMessage，直接替换以捕获输出
  self.postMessage = ((msg: unknown) => {
    posted.push(msg as WorkerOutput);
  }) as typeof self.postMessage;
  // runner.worker 在模块求值时自装 self.onmessage
  await import('@/workers/probe/runner.worker');
  handler = self.onmessage!;
  expect(typeof handler).toBe('function');
});

afterAll(() => {
  self.postMessage = realPostMessage;
});

beforeEach(() => {
  posted.length = 0;
});

function run(script: string, snapshot: CtxSnapshot = makeSnapshot()): WorkerOutput {
  const input: WorkerInput = { script, snapshot };
  handler.call(self, { data: input } as unknown as MessageEvent);
  expect(posted).toHaveLength(1);
  return posted[0];
}

describe('runner.worker onmessage', () => {
  it('脚本正常返回值 → ok:true 且带 result / durationMs', () => {
    const out = run('return ctx.app.version + "|" + ctx.nav.ua;');
    expect(out.ok).toBe(true);
    if (!out.ok) throw new Error('unreachable');
    expect(out.result).toBe('0.5.6|TestUA/1.0');
    expect(out.actions).toEqual([]);
    expect(Number.isInteger(out.durationMs)).toBe(true);
    expect(out.durationMs).toBeGreaterThanOrEqual(0);
  });

  it('ctx 方法在 Worker 侧可用（storage / idb / logs / perf 过滤）', () => {
    const out = run(`
      return {
        local: ctx.storage.get('k1'),
        session: ctx.storage.get('s1', 'session'),
        idb: ctx.idb.count('db-a', 'store1'),
        lastLog: ctx.logs.tail(1)[0].message,
        resources: ctx.perf.entries({ type: 'resource' }).length,
        mem: ctx.perf.memoryMB().used,
      };
    `);
    if (!out.ok) throw new Error('expected ok');
    expect(out.result).toEqual({
      local: 'v1',
      session: 'sv',
      idb: 7,
      lastLog: 'l2',
      resources: 1,
      mem: 50,
    });
  });

  it('ctx.cmd.* 收集为 actions 一并回传', () => {
    const out = run('ctx.cmd.reload(); ctx.cmd.clearCache(); ctx.cmd.signOut();');
    if (!out.ok) throw new Error('expected ok');
    expect(out.result).toBeUndefined();
    expect(out.actions).toEqual([{ type: 'reload' }, { type: 'clearCache' }, { type: 'signOut' }]);
  });

  it('actions 不跨消息累积', () => {
    run('ctx.cmd.reload();');
    posted.length = 0;
    const out = run('return 1;');
    if (!out.ok) throw new Error('expected ok');
    expect(out.actions).toEqual([]);
  });

  it('脚本抛 Error → ok:false，stderr 取 stack（含消息）', () => {
    const out = run('throw new Error("boom in script");');
    expect(out.ok).toBe(false);
    if (out.ok) throw new Error('unreachable');
    expect(out.stderr).toContain('boom in script');
    expect(Number.isInteger(out.durationMs)).toBe(true);
  });

  it('Error 无 stack 时退回 message', () => {
    const out = run('var e = new Error("no stack here"); delete e.stack; throw e;');
    if (out.ok) throw new Error('expected failure');
    expect(out.stderr).toBe('no stack here');
  });

  it('脚本抛非 Error → stderr 为 String(err)', () => {
    const out = run('throw { code: 42 };');
    if (out.ok) throw new Error('expected failure');
    expect(out.stderr).toBe('[object Object]');
  });

  it('语法错误在 new Function 阶段就被捕获', () => {
    const out = run('this is not javascript(');
    if (out.ok) throw new Error('expected failure');
    expect(out.stderr).toMatch(/SyntaxError/);
  });
});
