/**
 * misc_low 覆盖簇：
 *  - src/lib/sentry.ts          —— init/capture/isEnabled 三函数 + DSN 缺省/存在分支
 *  - src/lib/chartTheme.ts      —— chartColor/chartPalette/algorithmColor + 明暗/CSS 变量分支
 *  - src/workers/probe/ctx-factory.ts —— collectCtxSnapshot 在 idb 真实可用 / nav 扩展字段 /
 *                                       perf entries / withTimeout 超时路径下的未测分支
 *
 * 说明：目标簇里 NotificationBell / DiffSummary / VersionList / AgentManager /
 * prompt-templates 在本仓不存在（命名为别仓 wordforge-web 或已废弃），故聚焦实际存在的
 * 三个低覆盖文件，全部走稳定的纯函数 / 可控 mock，不依赖真实网络/时间/随机。
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

// ─────────────────────────────────────────────────────────────────────────────
// sentry.ts
// initialized 是模块级一次性 latch，且 init 读 import.meta.env.*，
// 用 vi.resetModules + 动态 import 隔离每个场景，用 vi.stubEnv 控制环境变量。
// ─────────────────────────────────────────────────────────────────────────────

const sentryInit = vi.fn();
const sentryCapture = vi.fn();

vi.mock('@sentry/browser', () => ({
  init: (...args: unknown[]) => sentryInit(...args),
  captureException: (...args: unknown[]) => sentryCapture(...args),
}));

async function loadSentry() {
  vi.resetModules();
  return import('@/lib/sentry');
}

describe('lib/sentry', () => {
  beforeEach(() => {
    sentryInit.mockClear();
    sentryCapture.mockClear();
  });
  afterEach(() => {
    vi.unstubAllEnvs();
  });

  it('DSN 缺省时 initSentry no-op，isSentryEnabled=false', async () => {
    vi.stubEnv('VITE_SENTRY_DSN', '');
    const m = await loadSentry();
    m.initSentry();
    expect(sentryInit).not.toHaveBeenCalled();
    expect(m.isSentryEnabled()).toBe(false);
  });

  it('DSN 缺省时 captureException 退化为 no-op', async () => {
    vi.stubEnv('VITE_SENTRY_DSN', '');
    const m = await loadSentry();
    m.initSentry();
    m.captureException(new Error('boom'), { route: '/x' });
    expect(sentryCapture).not.toHaveBeenCalled();
  });

  it('DSN 存在时初始化 SDK 且 isSentryEnabled=true', async () => {
    vi.stubEnv('VITE_SENTRY_DSN', 'https://abc@o1.ingest.sentry.io/123');
    vi.stubEnv('VITE_SENTRY_ENV', 'staging');
    vi.stubEnv('VITE_SENTRY_RELEASE', 'v1.2.3');
    const m = await loadSentry();
    m.initSentry();
    expect(sentryInit).toHaveBeenCalledTimes(1);
    const cfg = sentryInit.mock.calls[0][0] as Record<string, unknown>;
    expect(cfg.dsn).toBe('https://abc@o1.ingest.sentry.io/123');
    expect(cfg.environment).toBe('staging');
    expect(cfg.release).toBe('v1.2.3');
    expect(cfg.maxBreadcrumbs).toBe(50);
    expect(m.isSentryEnabled()).toBe(true);
  });

  it('重复 initSentry 幂等：只初始化一次', async () => {
    vi.stubEnv('VITE_SENTRY_DSN', 'https://abc@o1.ingest.sentry.io/123');
    const m = await loadSentry();
    m.initSentry();
    m.initSentry();
    m.initSentry();
    expect(sentryInit).toHaveBeenCalledTimes(1);
  });

  it('合法 tracesSampleRate 字符串被解析为数字', async () => {
    vi.stubEnv('VITE_SENTRY_DSN', 'https://abc@o1.ingest.sentry.io/123');
    vi.stubEnv('VITE_SENTRY_TRACES_SAMPLE_RATE', '0.25');
    const m = await loadSentry();
    m.initSentry();
    const cfg = sentryInit.mock.calls[0][0] as Record<string, unknown>;
    expect(cfg.tracesSampleRate).toBe(0.25);
  });

  it('非法 tracesSampleRate 回退为 0（NaN→disabled）', async () => {
    vi.stubEnv('VITE_SENTRY_DSN', 'https://abc@o1.ingest.sentry.io/123');
    vi.stubEnv('VITE_SENTRY_TRACES_SAMPLE_RATE', 'not-a-number');
    const m = await loadSentry();
    m.initSentry();
    const cfg = sentryInit.mock.calls[0][0] as Record<string, unknown>;
    expect(cfg.tracesSampleRate).toBe(0);
  });

  it('缺省 tracesSampleRate 时为 0', async () => {
    vi.stubEnv('VITE_SENTRY_DSN', 'https://abc@o1.ingest.sentry.io/123');
    vi.stubEnv('VITE_SENTRY_TRACES_SAMPLE_RATE', '');
    const m = await loadSentry();
    m.initSentry();
    const cfg = sentryInit.mock.calls[0][0] as Record<string, unknown>;
    expect(cfg.tracesSampleRate).toBe(0);
  });

  it('初始化后 captureException 带 context 透传到 contexts.solid', async () => {
    vi.stubEnv('VITE_SENTRY_DSN', 'https://abc@o1.ingest.sentry.io/123');
    const m = await loadSentry();
    m.initSentry();
    const err = new Error('x');
    m.captureException(err, { foo: 'bar' });
    expect(sentryCapture).toHaveBeenCalledWith(err, { contexts: { solid: { foo: 'bar' } } });
  });

  it('初始化后 captureException 无 context 时第二参为 undefined', async () => {
    vi.stubEnv('VITE_SENTRY_DSN', 'https://abc@o1.ingest.sentry.io/123');
    const m = await loadSentry();
    m.initSentry();
    const err = new Error('y');
    m.captureException(err);
    expect(sentryCapture).toHaveBeenCalledWith(err, undefined);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// chartTheme.ts —— mock themeStore 控制 effective()，并控制 getComputedStyle
// ─────────────────────────────────────────────────────────────────────────────

const themeEffective = vi.fn<() => 'light' | 'dark'>(() => 'light');
vi.mock('@/stores/theme', () => ({
  themeStore: {
    effective: () => themeEffective(),
  },
}));

import { chartColor, chartPalette, algorithmColor, algoPalette } from '@/lib/chartTheme';
import { algoMeta } from '@/pages/amas/algoMeta';

describe('lib/chartTheme', () => {
  let cssVarMap: Record<string, string>;
  beforeEach(() => {
    themeEffective.mockReturnValue('light');
    cssVarMap = {};
    // 让 getComputedStyle 默认返回空（走 fallback）
    vi.spyOn(window, 'getComputedStyle').mockReturnValue({
      getPropertyValue: (name: string) => cssVarMap[name] ?? '',
    } as unknown as CSSStyleDeclaration);
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('有 CSS 变量映射的 tone：变量为空时回退到 light fallback', () => {
    // accent 有 CSS 变量映射，但变量值为空 → fallback（mono：墨）
    expect(chartColor('accent')).toBe('#0a0a0b');
  });

  it('有 CSS 变量映射的 tone：变量有值时优先取变量', () => {
    cssVarMap['--accent'] = '  #abcabc  '; // 含空白，cssVar 会 trim
    expect(chartColor('accent')).toBe('#abcabc');
  });

  it('暗色模式下无 CSS 变量映射的 tone 取 dark fallback', () => {
    themeEffective.mockReturnValue('dark');
    // pink / violet 没有 CSS 变量映射 → 直接 fallback（mono：灰）
    expect(chartColor('pink')).toBe('#a1a1aa');
    expect(chartColor('violet')).toBe('#c5cad3');
  });

  it('明色模式下无 CSS 变量映射的 tone 取 light fallback', () => {
    themeEffective.mockReturnValue('light');
    expect(chartColor('pink')).toBe('#6b7280');
    expect(chartColor('violet')).toBe('#3f4654');
  });

  it('muted 有 CSS 变量映射（--content-tertiary）', () => {
    cssVarMap['--content-tertiary'] = '#123456';
    expect(chartColor('muted')).toBe('#123456');
  });

  it('chartPalette 返回 6 个颜色', () => {
    const p = chartPalette();
    expect(p).toHaveLength(6);
    expect(p.every((c) => typeof c === 'string' && c.startsWith('#'))).toBe(true);
  });

  it('algorithmColor 命中已知算法名 → 取调色板对应位置', () => {
    const palette = chartPalette();
    expect(algorithmColor('Heuristic')).toBe(palette[0]);
    expect(algorithmColor('IGE')).toBe(palette[1]);
    expect(algorithmColor('SWD')).toBe(palette[2]);
    expect(algorithmColor('Ensemble')).toBe(palette[3]);
    expect(algorithmColor('MDM')).toBe(palette[4]);
    expect(algorithmColor('Mastery')).toBe(palette[5]);
  });

  it('algorithmColor 未知算法名 → 取 muted 色', () => {
    expect(algorithmColor('Unknown')).toBe(chartColor('muted'));
  });

  it('algoPalette 返回 8 个 mono 分类色（首位=accent 墨，均为真实色值）', () => {
    const p = algoPalette();
    expect(p).toHaveLength(8);
    expect(p.every((c) => typeof c === 'string' && c.startsWith('#'))).toBe(true);
    expect(p[0]).toBe(chartColor('accent'));
  });

  it('algoMeta：已知算法取 label + tone 真实色，大小写不敏感，未知取 muted，空名取 —', () => {
    expect(algoMeta('ensemble')).toEqual({ label: 'ensemble', color: chartColor('accent') });
    expect(algoMeta('MDM')).toEqual({ label: 'MDM', color: chartColor('success') });
    expect(algoMeta('swd')).toEqual({ label: 'SWD', color: chartColor('info') });
    expect(algoMeta('IGE')).toEqual({ label: 'IGE', color: chartColor('warning') });
    expect(algoMeta('ssp')).toEqual({ label: 'SSP', color: chartColor('error') });
    expect(algoMeta('nope')).toEqual({ label: 'nope', color: chartColor('muted') });
    expect(algoMeta('').label).toBe('—');
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// ctx-factory.ts —— 补 collectCtxSnapshot 未覆盖分支
//   · nav.connection / nav.deviceMemory 存在路径
//   · perf.memory 存在 + entries 名截断 + resource 排序
//   · idb.databases 真实可用 + openIDB 成功/失败
// ─────────────────────────────────────────────────────────────────────────────

import { collectCtxSnapshot } from '@/workers/probe/ctx-factory';

describe('workers/probe/ctx-factory 扩展分支', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('nav.connection 与 deviceMemory 存在时被采集', async () => {
    const n = navigator as unknown as Record<string, unknown>;
    const origConn = Object.getOwnPropertyDescriptor(navigator, 'connection');
    const origMem = Object.getOwnPropertyDescriptor(navigator, 'deviceMemory');
    Object.defineProperty(navigator, 'connection', {
      configurable: true,
      get: () => ({ effectiveType: '4g', downlink: 10, rtt: 50 }),
    });
    Object.defineProperty(navigator, 'deviceMemory', {
      configurable: true,
      get: () => 8,
    });
    try {
      const snap = await collectCtxSnapshot();
      expect(snap.nav.connection).toEqual({ effectiveType: '4g', downlink: 10, rtt: 50 });
      expect(snap.nav.deviceMemory).toBe(8);
    } finally {
      if (origConn) Object.defineProperty(navigator, 'connection', origConn);
      else delete n.connection;
      if (origMem) Object.defineProperty(navigator, 'deviceMemory', origMem);
      else delete n.deviceMemory;
    }
  });

  it('perf.memory 存在时换算 MB；超长 entry name 被截断；resource 按耗时降序', async () => {
    const longName = 'x'.repeat(300);
    const fakeEntries = [
      { name: longName, entryType: 'mark', startTime: 1.4, duration: 2.6 },
      { name: 'short', entryType: 'measure', startTime: 0, duration: 1 },
    ];
    const fakeResources = [
      { name: 'http://a/slow', duration: 500 },
      { name: 'http://a/fast', duration: 10 },
    ];
    vi.spyOn(performance, 'getEntries').mockReturnValue(fakeEntries as unknown as PerformanceEntryList);
    vi.spyOn(performance, 'getEntriesByType').mockImplementation((type: string) =>
      type === 'resource' ? (fakeResources as unknown as PerformanceEntryList) : ([] as unknown as PerformanceEntryList),
    );
    // 注入 performance.memory
    Object.defineProperty(performance, 'memory', {
      configurable: true,
      get: () => ({
        usedJSHeapSize: 5 * 1024 * 1024,
        totalJSHeapSize: 10 * 1024 * 1024,
        jsHeapSizeLimit: 100 * 1024 * 1024,
      }),
    });
    try {
      const snap = await collectCtxSnapshot();
      expect(snap.perf.memoryMB).toEqual({ used: 5, total: 10, limit: 100 });
      // 第一条 name 被截断（256 + '…'）
      const truncated = snap.perf.entries.find((e) => e.entryType === 'mark');
      expect(truncated!.name.endsWith('…')).toBe(true);
      expect(truncated!.name.length).toBe(257);
      // resource 摘要：最慢在 topUrls[0]
      expect(snap.perf.resourceTimingSummary.count).toBe(2);
      expect(snap.perf.resourceTimingSummary.slowestMs).toBe(500);
      expect(snap.perf.resourceTimingSummary.topUrls[0]).toEqual({ url: 'http://a/slow', durationMs: 500 });
    } finally {
      // 还原 performance.memory（happy-dom 默认无此属性）
      delete (performance as unknown as Record<string, unknown>).memory;
    }
  });

  it('indexedDB.databases 可用时采集 list 与 store 占位计数', async () => {
    // objectStoreNames 需可被 Array.from 遍历——普通数组即满足
    const fakeDb = {
      objectStoreNames: ['kv', 'meta'],
      close: vi.fn(),
    } as unknown as IDBDatabase;

    const origIdb = (globalThis as unknown as Record<string, unknown>).indexedDB;
    const fakeIndexedDB = {
      databases: vi.fn().mockResolvedValue([{ name: 'app-db', version: 1 }, { name: '', version: 1 }]),
      open: vi.fn().mockImplementation(() => {
        const req: Record<string, unknown> = { result: fakeDb };
        queueMicrotask(() => {
          (req.onsuccess as (() => void) | undefined)?.();
        });
        return req;
      }),
    };
    Object.defineProperty(globalThis, 'indexedDB', {
      configurable: true,
      writable: true,
      value: fakeIndexedDB,
    });
    try {
      const snap = await collectCtxSnapshot();
      // 空 name 被过滤
      expect(snap.idb.list).toEqual(['app-db']);
      // store 占位为 -1
      expect(snap.idb.counts['app-db']).toEqual({ kv: -1, meta: -1 });
      expect(fakeDb.close).toHaveBeenCalled();
    } finally {
      if (origIdb === undefined) {
        delete (globalThis as unknown as Record<string, unknown>).indexedDB;
      } else {
        Object.defineProperty(globalThis, 'indexedDB', {
          configurable: true,
          writable: true,
          value: origIdb,
        });
      }
    }
  });

  it('openIDB 失败（onerror）时该 db 被跳过', async () => {
    const origIdb = (globalThis as unknown as Record<string, unknown>).indexedDB;
    const fakeIndexedDB = {
      databases: vi.fn().mockResolvedValue([{ name: 'bad-db', version: 1 }]),
      open: vi.fn().mockImplementation(() => {
        const req: Record<string, unknown> = { result: null };
        queueMicrotask(() => {
          (req.onerror as (() => void) | undefined)?.();
        });
        return req;
      }),
    };
    Object.defineProperty(globalThis, 'indexedDB', {
      configurable: true,
      writable: true,
      value: fakeIndexedDB,
    });
    try {
      const snap = await collectCtxSnapshot();
      expect(snap.idb.list).toEqual(['bad-db']);
      // open 返回 null → counts 不含该 db
      expect(snap.idb.counts['bad-db']).toBeUndefined();
    } finally {
      if (origIdb === undefined) {
        delete (globalThis as unknown as Record<string, unknown>).indexedDB;
      } else {
        Object.defineProperty(globalThis, 'indexedDB', {
          configurable: true,
          writable: true,
          value: origIdb,
        });
      }
    }
  });

  it('indexedDB.databases 抛错时降级为空 idb', async () => {
    const origIdb = (globalThis as unknown as Record<string, unknown>).indexedDB;
    const fakeIndexedDB = {
      databases: vi.fn().mockRejectedValue(new Error('denied')),
      open: vi.fn(),
    };
    Object.defineProperty(globalThis, 'indexedDB', {
      configurable: true,
      writable: true,
      value: fakeIndexedDB,
    });
    try {
      const snap = await collectCtxSnapshot();
      expect(snap.idb).toEqual({ list: [], counts: {} });
    } finally {
      if (origIdb === undefined) {
        delete (globalThis as unknown as Record<string, unknown>).indexedDB;
      } else {
        Object.defineProperty(globalThis, 'indexedDB', {
          configurable: true,
          writable: true,
          value: origIdb,
        });
      }
    }
  });

  it('idb 查询超过 200ms 时 withTimeout 回退 fallback', async () => {
    vi.useFakeTimers();
    const origIdb = (globalThis as unknown as Record<string, unknown>).indexedDB;
    // databases 永不 resolve → 触发 200ms timeout fallback
    const fakeIndexedDB = {
      databases: vi.fn().mockReturnValue(new Promise(() => {})),
      open: vi.fn(),
    };
    Object.defineProperty(globalThis, 'indexedDB', {
      configurable: true,
      writable: true,
      value: fakeIndexedDB,
    });
    try {
      const promise = collectCtxSnapshot();
      // 推进超过 200ms 触发 timeout 分支
      await vi.advanceTimersByTimeAsync(250);
      const snap = await promise;
      expect(snap.idb).toEqual({ list: [], counts: {} });
    } finally {
      vi.useRealTimers();
      if (origIdb === undefined) {
        delete (globalThis as unknown as Record<string, unknown>).indexedDB;
      } else {
        Object.defineProperty(globalThis, 'indexedDB', {
          configurable: true,
          writable: true,
          value: origIdb,
        });
      }
    }
  });
});
