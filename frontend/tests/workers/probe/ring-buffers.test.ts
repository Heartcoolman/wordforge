import { describe, it, expect, beforeEach } from 'vitest';
import {
  installRingBuffers,
  ringBuffers,
  uninstallRingBuffers,
} from '@/workers/probe/ring-buffers';

describe('ring-buffers', () => {
  beforeEach(() => uninstallRingBuffers());

  it('logs buffer 容量 200，写满后循环覆盖最旧', () => {
    installRingBuffers({ captureConsole: true });
    for (let i = 0; i < 250; i++) {
      console.log(`msg ${i}`);
    }
    const snap = ringBuffers.logs.snapshot();
    expect(snap.length).toBe(200);
    expect(snap[0].message).toBe('msg 50'); // 旧→新，前 50 条被覆盖
    expect(snap[snap.length - 1].message).toBe('msg 249');
  });

  it('errors buffer 容量 50', () => {
    installRingBuffers();
    for (let i = 0; i < 60; i++) {
      window.dispatchEvent(
        new ErrorEvent('error', { message: `err ${i}`, filename: 'x.js', lineno: 1, colno: 1 }),
      );
    }
    const snap = ringBuffers.errors.snapshot();
    expect(snap.length).toBe(50);
    expect(snap[0].message).toBe('err 10');
  });

  it('tail(n) 返回最后 n 条（n 超出时返回全部）', () => {
    installRingBuffers();
    for (let i = 0; i < 30; i++) console.log(`l${i}`);
    expect(ringBuffers.logs.tail(5).map((e) => e.message)).toEqual([
      'l25', 'l26', 'l27', 'l28', 'l29',
    ]);
    expect(ringBuffers.logs.tail(100).length).toBe(30);
  });

  it('fetch 拦截器记录 url/method/status/duration', async () => {
    installRingBuffers();
    // mock fetch 已经被 installRingBuffers 包装，里面调用 originals.fetch
    // happy-dom 自带 fetch 行为是 throw（外网请求），我们 catch 后仍应有 net 条目（status=0）
    try {
      await fetch('https://example.invalid/nope', { method: 'POST' });
    } catch {
      /* expected */
    }
    const snap = ringBuffers.net.snapshot();
    expect(snap.length).toBeGreaterThanOrEqual(1);
    const last = snap[snap.length - 1];
    expect(last.method).toBe('POST');
    expect(last.url).toContain('example.invalid');
    expect(typeof last.durationMs).toBe('number');
  });

  it('install 幂等：重复调用不会重复 wrap', () => {
    installRingBuffers();
    const wrappedLog = console.log;
    installRingBuffers();
    expect(console.log).toBe(wrappedLog);
  });
});
