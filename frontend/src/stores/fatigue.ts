import { createSignal, createRoot } from 'solid-js';
import { storage, STORAGE_KEYS } from '@/lib/storage';

// 疲劳等级：清醒 → 轻度 → 中度 → 重度
export type FatigueLevel = 'alert' | 'mild' | 'moderate' | 'severe';

// 单帧疲劳检测结果
export interface FatigueResult {
  score: number;        // 0-100 综合疲劳分数
  level: FatigueLevel;
  perclos: number;      // 眼睛闭合百分比
  blinkRate: number;    // 眨眼频率 (次/分钟)
  yawnCount: number;    // 累计哈欠次数
  headDropRatio: number; // 低头比例
  timestamp: number;
}

function createFatigueStore() {
  // 持久化的开关状态
  const savedEnabled = storage.get<boolean>(STORAGE_KEYS.FATIGUE_ENABLED, false);
  const [enabled, setEnabled] = createSignal(savedEnabled);

  // 运行时状态
  const [detecting, setDetecting] = createSignal(false);
  const [wasmReady, setWasmReady] = createSignal(false);
  const [cameraReady, setCameraReady] = createSignal(false);

  // 疲劳指标
  const [fatigueScore, setFatigueScore] = createSignal(0);
  const [fatigueLevel, setFatigueLevel] = createSignal<FatigueLevel>('alert');
  const [blinkRate, setBlinkRate] = createSignal(0);
  const [perclos, setPerclos] = createSignal(0);

  // 学习时长（秒）
  const [sessionDuration, setSessionDuration] = createSignal(0);
  let sessionTimer: ReturnType<typeof setInterval> | null = null;

  // 切换启用状态
  function toggle() {
    const next = !enabled();
    setEnabled(next);
    storage.set(STORAGE_KEYS.FATIGUE_ENABLED, next);
  }

  function enable() {
    setEnabled(true);
    storage.set(STORAGE_KEYS.FATIGUE_ENABLED, true);
  }

  function disable() {
    setEnabled(false);
    storage.set(STORAGE_KEYS.FATIGUE_ENABLED, false);
  }

  // 更新检测结果
  function updateResult(result: FatigueResult) {
    setFatigueScore(result.score);
    setFatigueLevel(result.level);
    setBlinkRate(result.blinkRate);
    setPerclos(result.perclos);
  }

  // 重置所有指标
  function reset() {
    setDetecting(false);
    setWasmReady(false);
    setCameraReady(false);
    setFatigueScore(0);
    setFatigueLevel('alert');
    setBlinkRate(0);
    setPerclos(0);
    stopSession();
  }

  /**
   * 开始计时。
   * 重要：调用方必须在组件卸载时（通过 onCleanup）调用 stopSession()，
   * 否则 setInterval 会泄漏。推荐用法：
   *
   *   fatigueStore.startSession();
   *   onCleanup(() => fatigueStore.stopSession());
   */
  function startSession() {
    setSessionDuration(0);
    if (sessionTimer) clearInterval(sessionTimer);
    sessionTimer = setInterval(() => {
      setSessionDuration((d) => d + 1);
    }, 1000);
  }

  // 停止计时
  function stopSession() {
    if (sessionTimer) {
      clearInterval(sessionTimer);
      sessionTimer = null;
    }
  }

  return {
    // 状态
    enabled,
    detecting,
    wasmReady,
    cameraReady,
    fatigueScore,
    fatigueLevel,
    blinkRate,
    perclos,
    sessionDuration,
    // 内部 setter（供 hook 调用）
    setDetecting,
    setWasmReady,
    setCameraReady,
    // 方法
    toggle,
    enable,
    disable,
    updateResult,
    reset,
    startSession,
    stopSession,
  };
}

export const fatigueStore = createRoot(createFatigueStore);
