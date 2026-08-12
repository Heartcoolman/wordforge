import { createSignal } from 'solid-js';

const STORAGE_KEY = 'wf_admin_onboarding_wave';

/**
 * 把 app 版本号映射为「新功能导览波次」标识。
 * - 1.3.x 为独立波次 'v1.3-app-events'（端侧埋点数据链路上线 + 学习记录上报默认异步化）。
 * - 1.2.x 为独立波次 'v1.2-redesign'（管理后台蓝玻璃整体重设计；强制看过旧导览的人再弹一次）。
 * - v1.1.4 起为独立波次 'v1.1.4r2'（运营闭环收口 + 设备遥测操作概览）。
 * - v1.1.3 为独立波次 'v1.1.3'（告警收件箱 / 设备推送调度 / 版本门控 / 事件 outbox）。
 * - 1.1.0–1.1.2 仍属旧「admin 运维面重构」波 'v1.1-1.2'。
 * 同波内重复升级(含各 beta / patch)不重弹；跨入下一波(或波次标识翻新)重新弹一次。拿不到版本返回 null(静默不弹)。
 * 注：开新功能波 / 翻新波次标识时须同步改 waveOf + OnboardingTour STEPS + e2e 预置值（admin-real-flows.spec.ts loginAsAdmin）。
 */
export function waveOf(version: string | null | undefined): string | null {
  if (!version) return null;
  const m = /^(\d+)\.(\d+)\.(\d+)/.exec(version);
  if (!m) return null;
  const major = +m[1], minor = +m[2], patch = +m[3];
  if (major === 1 && minor === 3) return 'v1.3-app-events';
  if (major === 1 && minor === 2) return 'v1.2-redesign';
  if (major === 1 && minor === 1 && patch >= 4) return 'v1.1.4r2';
  if (major === 1 && minor === 1 && patch === 3) return 'v1.1.3';
  if (major === 1 && minor === 1 && patch <= 2) return 'v1.1-1.2';
  return `${major}.${minor}`;
}

// 由 autoShowIfNeeded/shouldAutoShow 注入版本后算出，供 markSeen 写回。
let currentWave: string | null = null;

/** 当前版本所属波次是否未看过则应自动弹；拿不到版本则不弹(安全降级)。 */
export function shouldAutoShow(version: string | null | undefined): boolean {
  currentWave = waveOf(version);
  if (!currentWave) return false;
  try {
    return localStorage.getItem(STORAGE_KEY) !== currentWave;
  } catch {
    return false;
  }
}

/** 标记当前波次为已看(此后同波不再自动弹，直到跨入下一大版本)。 */
export function markSeen(): void {
  if (!currentWave) return;
  try {
    localStorage.setItem(STORAGE_KEY, currentWave);
  } catch { /* 隐私模式等场景静默 */ }
}

/** 控制导览开关的全局单例 signal —— 顶栏重看按钮与 onMount 自动弹共用。 */
const [tourOpen, setTourOpen] = createSignal(false);

export function useOnboarding() {
  return {
    open: tourOpen,
    /** 重看：不依赖 localStorage，直接打开 */
    show: () => setTourOpen(true),
    close: () => setTourOpen(false),
    /** 进入 admin 区域时调用：传入当前 app 版本，未看过其波次则自动打开 */
    autoShowIfNeeded: (version: string | null | undefined) => {
      if (shouldAutoShow(version)) setTourOpen(true);
    },
  };
}
