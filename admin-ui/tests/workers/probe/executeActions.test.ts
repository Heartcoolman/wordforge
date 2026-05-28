/**
 * executeActions 序固化单测：
 *  - clearCache → signOut → reload 的顺序
 *  - signOut 短路 reload
 *  - clearCache 清 localStorage / sessionStorage / Cache Storage
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { executeActions } from '@/workers/probe/api-bridge';
import type { ProbeAction } from '@/workers/probe/types';

describe('executeActions', () => {
  let reloadSpy: ReturnType<typeof vi.fn>;
  let assignSpy: ReturnType<typeof vi.fn>;
  const realLocation = window.location;

  beforeEach(() => {
    localStorage.setItem('a', '1');
    sessionStorage.setItem('b', '2');

    reloadSpy = vi.fn();
    assignSpy = vi.fn();
    // mock location（happy-dom 允许重写）
    Object.defineProperty(window, 'location', {
      configurable: true,
      value: { ...realLocation, reload: reloadSpy, assign: assignSpy },
    });
    // mock caches API（happy-dom 没有原生支持）
    (window as any).caches = {
      keys: vi.fn().mockResolvedValue(['c1', 'c2']),
      delete: vi.fn().mockResolvedValue(true),
    };
  });

  it('clearCache 清空 LS/SS/Cache Storage', async () => {
    const actions: ProbeAction[] = [{ type: 'clearCache' }];
    await executeActions(actions);
    expect(localStorage.getItem('a')).toBeNull();
    expect(sessionStorage.getItem('b')).toBeNull();
    expect((window as any).caches.keys).toHaveBeenCalled();
    expect((window as any).caches.delete).toHaveBeenCalledTimes(2);
  });

  it('单独 reload 调 location.reload', async () => {
    await executeActions([{ type: 'reload' }]);
    expect(reloadSpy).toHaveBeenCalled();
  });

  it('signOut 跳 /admin/login 且短路 reload', async () => {
    await executeActions([{ type: 'reload' }, { type: 'signOut' }]);
    // signOut 在 reload 之前（顺序固化）
    expect(assignSpy).toHaveBeenCalledWith('/admin/login');
    // reload 不被调（signOut 后短路）
    expect(reloadSpy).not.toHaveBeenCalled();
  });

  it('clearCache + reload 双动作按序执行', async () => {
    await executeActions([{ type: 'reload' }, { type: 'clearCache' }]);
    // 先 clearCache 后 reload
    expect(localStorage.length).toBe(0);
    expect(reloadSpy).toHaveBeenCalled();
  });
});
