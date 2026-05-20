/**
 * Motion 工具层 — 统一封装 solid-motionone 的常用预设与 helper
 *
 * 设计原则：
 * - 项目 90% 动画走 CSS（index.css 已就位）；本文件只服务"列表增删 / Modal 进出场 /
 *   Toast 出场 / Tab 切换 / Drawer / 路由切换"等需要 Presence 或 spring 的关键交互。
 * - 时长 / 缓动统一从这里读，避免散落硬编码；与 index.css 的 CSS var 同值。
 * - solid-motionone v1 暂不支持 layout animation；indicator 滑动用本文件的
 *   useIndicatorTrack helper（CSS transform + ref 测量）实现。
 */

import { onCleanup, onMount, createSignal, createEffect, type Accessor } from 'solid-js';

// Re-export 主要 API，业务代码统一从此引入
export { Motion, Presence, createMotion, motion } from 'solid-motionone';

// ═══════════════════════════════════════════
// Token 镜像（与 index.css 同值，硬编码避免 SSR/getComputedStyle 开销）
// ═══════════════════════════════════════════

/** 动画时长（秒制，Motion One 约定） */
export const DURATION = {
  fast: 0.15,
  base: 0.22,
  slow: 0.32,
} as const;

/** 缓动曲线（与 index.css --ease-* 同值） */
export const EASE = {
  outExpo: [0.16, 1, 0.3, 1] as [number, number, number, number],
  spring: [0.5, 1.4, 0.5, 1] as [number, number, number, number],
  inOutQuart: [0.76, 0, 0.24, 1] as [number, number, number, number],
} as const;

// ═══════════════════════════════════════════
// motionPresets — 业务侧统一变体
// ═══════════════════════════════════════════

/** Modal 进出场（透明度 + 轻微 scale，配合 backdrop-blur） */
export const modalPreset = {
  initial: { opacity: 0, scale: 0.96 },
  animate: { opacity: 1, scale: 1 },
  exit: { opacity: 0, scale: 0.96 },
  transition: { duration: DURATION.base, easing: EASE.outExpo },
} as const;

/** Drawer / 右侧抽屉 */
export const drawerPreset = {
  initial: { opacity: 0, x: 40 },
  animate: { opacity: 1, x: 0 },
  exit: { opacity: 0, x: 40 },
  transition: { duration: DURATION.base, easing: EASE.outExpo },
} as const;

/** Toast 进出场（右侧滑入，出场收高度避免邻居跳动） */
export const toastPreset = {
  initial: { opacity: 0, x: 40 },
  animate: { opacity: 1, x: 0 },
  exit: { opacity: 0, x: 40 },
  transition: { duration: DURATION.base, easing: EASE.outExpo },
} as const;

/** Dropdown / Popover */
export const dropdownPreset = {
  initial: { opacity: 0, scale: 0.96, y: -4 },
  animate: { opacity: 1, scale: 1, y: 0 },
  exit: { opacity: 0, scale: 0.96, y: -4 },
  transition: { duration: DURATION.fast, easing: EASE.outExpo },
} as const;

/** 列表项进出场（删除时收高度） */
export const listItemPreset = {
  initial: { opacity: 0, y: 12 },
  animate: { opacity: 1, y: 0 },
  exit: { opacity: 0, y: -8, height: 0 },
  transition: { duration: DURATION.base, easing: EASE.outExpo },
} as const;

/** Tab 内容切换 */
export const tabPanelPreset = {
  initial: { opacity: 0, x: 8 },
  animate: { opacity: 1, x: 0 },
  exit: { opacity: 0, x: -8 },
  transition: { duration: DURATION.base, easing: EASE.outExpo },
} as const;

/** 路由页面切换 */
export const pageTransitionPreset = {
  initial: { opacity: 0, y: 6 },
  animate: { opacity: 1, y: 0 },
  exit: { opacity: 0, y: -6 },
  transition: { duration: DURATION.base, easing: EASE.outExpo },
} as const;

// ═══════════════════════════════════════════
// Helper — useCountUp（KPI 数字滚动）
// ═══════════════════════════════════════════

interface CountUpOptions {
  /** 起始值，默认 0 */
  from?: number;
  /** 目标值（reactive） */
  to: Accessor<number>;
  /** 动画时长（秒），默认 0.7s */
  duration?: number;
  /** 缓动，默认 outExpo */
  easing?: [number, number, number, number];
  /** 数字格式化，默认 Math.round */
  format?: (v: number) => string;
}

/** Cubic-bezier 求解（标准 De Casteljau），用于 useCountUp 缓动 */
function bezierY(t: number, p1y: number, p2y: number): number {
  // 3 阶贝塞尔在 x=t 的近似 y（控制点 X 假设线性，简化为 t 作为参数）
  const u = 1 - t;
  return 3 * u * u * t * p1y + 3 * u * t * t * p2y + t * t * t;
}

/**
 * 把数字从 from 滚动到 to。target 变化时自动从当前值继续滚到新值。
 * 返回 Accessor<string> 直接挂到 JSX。
 *
 * 实现：requestAnimationFrame 直驱（避免引入 @motionone/dom transitive dep），
 * 缓动用三阶贝塞尔近似。
 *
 * @example
 * const formatted = useCountUp({ to: () => stats().total });
 * return <span>{formatted()}</span>;
 */
export function useCountUp(options: CountUpOptions): Accessor<string> {
  const { from = 0, duration = 0.7, easing = EASE.outExpo, format = (v) => String(Math.round(v)) } = options;
  const [display, setDisplay] = createSignal<string>(format(from));
  let current = from;
  let rafId: number | null = null;

  const reducedMotion =
    typeof window !== 'undefined' && window.matchMedia?.('(prefers-reduced-motion: reduce)').matches;

  createEffect(() => {
    const target = options.to();
    const start = current;
    if (start === target) {
      setDisplay(format(target));
      return;
    }
    if (reducedMotion) {
      current = target;
      setDisplay(format(target));
      return;
    }

    if (rafId !== null) cancelAnimationFrame(rafId);
    const t0 = performance.now();
    const durationMs = duration * 1000;
    const [, p1y, , p2y] = easing;

    const step = (now: number) => {
      const t = Math.min(1, (now - t0) / durationMs);
      const eased = bezierY(t, p1y, p2y);
      current = start + (target - start) * eased;
      setDisplay(format(current));
      if (t < 1) {
        rafId = requestAnimationFrame(step);
      } else {
        rafId = null;
      }
    };
    rafId = requestAnimationFrame(step);

    onCleanup(() => {
      if (rafId !== null) {
        cancelAnimationFrame(rafId);
        rafId = null;
      }
    });
  });

  return display;
}

// ═══════════════════════════════════════════
// Helper — useIndicatorTrack（Tabs / Sidebar 激活 indicator 滑动）
// ═══════════════════════════════════════════

interface IndicatorRect {
  /** 相对 container 的 left（用于水平 indicator） */
  left: number;
  /** 相对 container 的 top（用于垂直 indicator） */
  top: number;
  width: number;
  height: number;
}

/**
 * 测量 container 内"激活元素"的位置，返回 reactive rect。
 * 业务侧用 `style={{ transform: \`translateX(\${rect().left}px)\`, width: \`\${rect().width}px\` }}`
 * 配合 `transition: transform 220ms var(--ease-out-expo)` 实现 indicator 平滑滑动。
 *
 * @param container 父容器 ref（绝对定位上下文）
 * @param activeKey reactive 标识激活项变化的 signal（如 location.pathname / activeTab）
 * @param selector 从 container 查找激活元素的选择器，如 `[data-active="true"]`
 *
 * @example
 * let containerRef!: HTMLDivElement;
 * const rect = useIndicatorTrack(() => containerRef, () => location.pathname, '[data-active="true"]');
 * <div ref={containerRef} class="relative">
 *   <For each={tabs}>{(t) => <button data-active={t.id === active() ? 'true' : undefined}>...</button>}</For>
 *   <div class="absolute bottom-0 h-0.5 bg-accent transition-transform duration-base ease-out-expo"
 *        style={{ transform: `translateX(${rect().left}px)`, width: `${rect().width}px` }} />
 * </div>
 */
export function useIndicatorTrack(
  container: () => HTMLElement | undefined,
  activeKey: () => string | number | undefined,
  selector: string,
): Accessor<IndicatorRect> {
  const [rect, setRect] = createSignal<IndicatorRect>({ left: 0, top: 0, width: 0, height: 0 });

  const measure = () => {
    const root = container();
    if (!root) return;
    const target = root.querySelector(selector) as HTMLElement | null;
    if (!target) return;
    const rootBox = root.getBoundingClientRect();
    const tgtBox = target.getBoundingClientRect();
    setRect({
      left: tgtBox.left - rootBox.left + root.scrollLeft,
      top: tgtBox.top - rootBox.top + root.scrollTop,
      width: tgtBox.width,
      height: tgtBox.height,
    });
  };

  // 激活变化时重新测量（下一帧等 data-active 已生效）
  createEffect(() => {
    activeKey();
    requestAnimationFrame(measure);
  });

  // 容器 resize 时跟随
  onMount(() => {
    measure();
    const root = container();
    if (!root || typeof ResizeObserver === 'undefined') return;
    const ro = new ResizeObserver(() => measure());
    ro.observe(root);
    onCleanup(() => ro.disconnect());
  });

  return rect;
}
