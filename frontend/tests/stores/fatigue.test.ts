import { describe, it, expect, vi, beforeEach } from 'vitest';
import { createRoot } from 'solid-js';

// fatigue store 是 createRoot 单例，需要 resetModules 获取新实例
describe('fatigueStore', () => {
  beforeEach(() => {
    vi.resetModules();
    // 清除 localStorage
    localStorage.clear();
  });

  async function getFreshStore() {
    const mod = await import('@/stores/fatigue');
    return mod.fatigueStore;
  }

  it('初始状态为关闭', async () => {
    const store = await getFreshStore();
    createRoot(() => {
      expect(store.enabled()).toBe(false);
      expect(store.detecting()).toBe(false);
      expect(store.wasmReady()).toBe(false);
      expect(store.cameraReady()).toBe(false);
      expect(store.fatigueScore()).toBe(0);
      expect(store.fatigueLevel()).toBe('alert');
    });
  });

  it('toggle 切换启用状态', async () => {
    const store = await getFreshStore();
    createRoot(() => {
      expect(store.enabled()).toBe(false);
      store.toggle();
      expect(store.enabled()).toBe(true);
      store.toggle();
      expect(store.enabled()).toBe(false);
    });
  });

  it('enable/disable 方法', async () => {
    const store = await getFreshStore();
    createRoot(() => {
      store.enable();
      expect(store.enabled()).toBe(true);
      store.disable();
      expect(store.enabled()).toBe(false);
    });
  });

  it('updateResult 更新疲劳指标', async () => {
    const store = await getFreshStore();
    createRoot(() => {
      store.updateResult({
        score: 60,
        level: 'moderate',
        perclos: 0.3,
        blinkRate: 12,
        yawnCount: 2,
        headDropRatio: 0.1,
        timestamp: Date.now(),
      });
      expect(store.fatigueScore()).toBe(60);
      expect(store.fatigueLevel()).toBe('moderate');
      expect(store.blinkRate()).toBe(12);
      expect(store.perclos()).toBe(0.3);
    });
  });

  it('reset 重置所有状态', async () => {
    const store = await getFreshStore();
    createRoot(() => {
      store.enable();
      store.setDetecting(true);
      store.updateResult({
        score: 80,
        level: 'severe',
        perclos: 0.5,
        blinkRate: 20,
        yawnCount: 5,
        headDropRatio: 0.3,
        timestamp: Date.now(),
      });
      store.reset();
      expect(store.detecting()).toBe(false);
      expect(store.wasmReady()).toBe(false);
      expect(store.cameraReady()).toBe(false);
      expect(store.fatigueScore()).toBe(0);
      expect(store.fatigueLevel()).toBe('alert');
    });
  });

  it('scoreToLevel 分级逻辑正确', async () => {
    const store = await getFreshStore();
    createRoot(() => {
      // alert: < 25
      store.updateResult({ score: 10, level: 'alert', perclos: 0, blinkRate: 0, yawnCount: 0, headDropRatio: 0, timestamp: 0 });
      expect(store.fatigueLevel()).toBe('alert');

      // mild: 25-49
      store.updateResult({ score: 30, level: 'mild', perclos: 0, blinkRate: 0, yawnCount: 0, headDropRatio: 0, timestamp: 0 });
      expect(store.fatigueLevel()).toBe('mild');

      // moderate: 50-74
      store.updateResult({ score: 60, level: 'moderate', perclos: 0, blinkRate: 0, yawnCount: 0, headDropRatio: 0, timestamp: 0 });
      expect(store.fatigueLevel()).toBe('moderate');

      // severe: >= 75
      store.updateResult({ score: 80, level: 'severe', perclos: 0, blinkRate: 0, yawnCount: 0, headDropRatio: 0, timestamp: 0 });
      expect(store.fatigueLevel()).toBe('severe');
    });
  });

  it('session 计时器启动和停止', async () => {
    vi.useFakeTimers();
    const store = await getFreshStore();
    createRoot(() => {
      store.startSession();
      expect(store.sessionDuration()).toBe(0);

      vi.advanceTimersByTime(3000);
      expect(store.sessionDuration()).toBe(3);

      store.stopSession();
      vi.advanceTimersByTime(2000);
      // 停止后不再增长
      expect(store.sessionDuration()).toBe(3);
    });
    vi.useRealTimers();
  });

  it('持久化启用状态到 localStorage', async () => {
    const store = await getFreshStore();
    createRoot(() => {
      store.enable();
    });
    // 检查 localStorage 中有相关数据
    const raw = localStorage.getItem('eng_fatigue_enabled');
    expect(raw).not.toBeNull();
  });
});
