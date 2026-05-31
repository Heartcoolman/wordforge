import { createSignal } from 'solid-js';

const STORAGE_KEY = 'wf_admin_onboarding_wave';

/**
 * 把 app 版本号映射为「大版本波次」标识。
 * 本波大版本(1.1.x ~ 1.2.x 的 admin 运维面重构)视为同一波 'v1.1-1.2'：
 * 同波内重复升级(含各 beta / patch)不重弹；升入 1.3+ 视为新波(按 major.minor)，重新弹一次。
 * 拿不到版本时返回 null，整套机制静默(不弹)。
 */
export function waveOf(version: string | null | undefined): string | null {
  if (!version) return null;
  const m = /^(\d+)\.(\d+)/.exec(version);
  if (!m) return null;
  const major = +m[1], minor = +m[2];
  if (major === 1 && minor >= 1 && minor <= 2) return 'v1.1-1.2';
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
