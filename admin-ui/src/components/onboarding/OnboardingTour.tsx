import { For, Show, createEffect, createSignal, on, onCleanup } from 'solid-js';
import { Portal } from 'solid-js/web';
import { useNavigate } from '@solidjs/router';
import { cn } from '@/utils/cn';
import { Button } from '@/components/ui/Button';
import { Kbd } from '@/components/ui/Kbd';
import { useOnboarding, markSeen } from './useOnboarding';
import './onboarding.css';

type Accent = 'accent' | 'llm' | 'info';

interface Step {
  key: string;
  /** 路由；welcome / outro / 无独立页的功能屏无 route */
  route?: string;
  title: string;
  tagline: string;
  /** 多段 path 用换行分隔 */
  iconPath: string;
  highlights: string[];
  accent: Accent;
  /** 列表前缀符号，默认 ✓ */
  bullet?: '✓' | '▸';
}

// 顺序即步骤顺序：欢迎屏 → 重设计亮点屏 → 结束屏。本波（v1.2.x）= 管理后台蓝玻璃重设计。
const STEPS: Step[] = [
  {
    key: 'welcome',
    title: 'WordForge Admin v1.2.0',
    tagline: '管理后台焕新 —— 全新蓝玻璃设计 + 自动刷新更顺滑，用一分钟看看新界面',
    iconPath: 'M5 3l1.5 4.5L11 9l-4.5 1.5L5 15l-1.5-4.5L-1 9\nM18 9l1 3 3 1-3 1-1 3-1-3-3-1 3-1z\nM12 14l.9 2.7L15.6 18l-2.7.9L12 21.6l-.9-2.7L8.4 18l2.7-.9z',
    highlights: [
      '全新蓝玻璃界面 · 16 页 + 侧栏顶栏整体重做',
      '⌘K 命令面板 · 一键跳转任意页',
      '暗色主题 · 顶栏一键切换',
      '自动刷新更顺滑 · 轮询页不再跳顶 / 闪烁',
      '更轻量 · 移除 echarts / CodeMirror，加载更快',
    ],
    accent: 'accent',
    bullet: '▸',
  },
  {
    key: 'dashboard',
    route: '/admin',
    title: '全新仪表盘',
    tagline: '注册 · 活跃 · 答题 · 系统健康，一屏蓝玻璃概览',
    iconPath: 'M3 3v18h18\nM7 14l3-4 4 3 5-7',
    highlights: [
      'KPI 卡 + 用户活跃趋势 + AMAS 算法分布一屏直达',
      '所有图表改手写 SVG，体积更小、渲染更快',
      '7 / 14 / 30 天窗口切换 + 一键导出 CSV',
      'Worker 心跳 / 系统状态实时刷新，滚动不再被打断',
    ],
    accent: 'accent',
  },
  {
    key: 'shell-nav',
    title: '玻璃侧栏 · 7 组导航',
    tagline: '功能按职责分 7 组，顶栏聚合搜索 / 通知 / 主题 / 账户',
    iconPath: 'M3 3h7v7H3z\nM14 3h7v7h-7z\nM14 14h7v7h-7z\nM3 14h7v7H3z',
    highlights: [
      '侧栏按 7 组归类，16 个功能页一一对应',
      '顶栏 ⌘K 命令面板 · 通知铃 · 主题切换 · 账户',
      '窄屏侧栏自动收起为抽屉，移动端也好用',
      '暗色 / 亮色跟随你此前的偏好',
    ],
    accent: 'info',
  },
  {
    key: 'smooth-refresh',
    title: '自动刷新更顺滑',
    tagline: '5s 轮询页刷新时不再跳回顶部、不再闪烁',
    iconPath: 'M23 4v6h-6\nM1 20v-6h6\nM3.51 9a9 9 0 0 1 14.85-3.36L23 10\nM1 14l4.64 4.36A9 9 0 0 0 20.49 15',
    highlights: [
      '仪表盘 / 设备 / 探针指标 / 版本更新等轮询页已修复',
      '刷新时保留你的滚动位置，不再被打断',
      '不再因加载态把整页内容拆掉重建',
    ],
    accent: 'llm',
  },
  {
    key: 'outro',
    title: '随时可以再看一遍',
    tagline: '点击顶栏的「导览」按钮即可重新打开本导览。开始体验吧！',
    iconPath: 'M22 11.08V12a10 10 0 1 1-5.93-9.14\nM22 4 12 14.01l-3-3',
    highlights: [
      '顶栏 ⌘K 旁的「导览」按钮可随时重看',
      '← → 翻页，Esc 关闭，Enter 进入当前功能',
    ],
    accent: 'accent',
    bullet: '▸',
  },
];

function StepIcon(props: { paths: string; class?: string }) {
  return (
    <svg
      class={props.class}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      <For each={props.paths.split('\n')}>{(d) => <path d={d} />}</For>
    </svg>
  );
}

export function OnboardingTour() {
  const navigate = useNavigate();
  const { open, close } = useOnboarding();
  const [idx, setIdx] = createSignal(0);

  const total = STEPS.length;
  const step = () => STEPS[idx()];
  const isFirst = () => idx() === 0;
  const isLast = () => idx() === total - 1;

  let primaryRef: HTMLButtonElement | undefined;

  const finish = () => {
    markSeen();
    close();
  };
  const skip = () => {
    markSeen();
    close();
  };
  const next = () => (isLast() ? finish() : setIdx((i) => i + 1));
  const prev = () => setIdx((i) => Math.max(0, i - 1));

  // 当前屏主按钮：功能屏跳转并完成；welcome/outro 进入下一步或完成
  const onPrimary = () => {
    const s = step();
    if (s.route) {
      markSeen();
      close();
      navigate(s.route);
    } else {
      next();
    }
  };
  const primaryLabel = () => (step().route ? '去看看 →' : isLast() ? '完成' : '开始 →');

  // 打开/关闭：锁 body 滚动 + 键盘 + 重置到首屏 + 聚焦主按钮
  let savedOverflow: string | null = null;
  let keyHandler: ((e: KeyboardEvent) => void) | null = null;

  const teardown = () => {
    if (keyHandler) {
      document.removeEventListener('keydown', keyHandler);
      keyHandler = null;
    }
    if (savedOverflow !== null) {
      document.body.style.overflow = savedOverflow;
      savedOverflow = null;
    }
  };

  createEffect(on(open, (isOpen, wasOpen) => {
    if (isOpen === wasOpen) return;
    if (isOpen) {
      setIdx(0);
      savedOverflow = document.body.style.overflow;
      document.body.style.overflow = 'hidden';
      keyHandler = (e: KeyboardEvent) => {
        if (e.key === 'Escape') { e.preventDefault(); skip(); }
        else if (e.key === 'ArrowRight') { e.preventDefault(); next(); }
        else if (e.key === 'ArrowLeft') { e.preventDefault(); prev(); }
        else if (e.key === 'Enter') { e.preventDefault(); onPrimary(); }
      };
      document.addEventListener('keydown', keyHandler);
      requestAnimationFrame(() => primaryRef?.focus());
    } else {
      teardown();
    }
  }));

  onCleanup(teardown);

  return (
    <Show when={open()}>
      <Portal>
        <div
          class="onboarding fixed inset-0 z-[260] flex items-center justify-center p-0 sm:p-4 animate-fade-in"
          data-accent={step().accent}
          role="dialog"
          aria-modal="true"
          aria-label="新功能导览"
        >
          {/* backdrop */}
          <div
            class="absolute inset-0 backdrop-blur-md"
            style={{ background: 'color-mix(in oklab, var(--content) 40%, transparent)' }}
            onClick={skip}
          />

          {/* card */}
          <div
            class={cn(
              'relative flex flex-col w-full h-full sm:h-auto sm:w-[min(640px,92vw)]',
              'bg-surface-elevated sm:rounded-2xl sm:shadow-elevation-4 sm:border sm:border-border-hairline',
              'animate-scale-in overflow-hidden',
            )}
          >
            {/* 顶部进度条 + 跳过 */}
            <div class="flex items-center gap-3 px-5 sm:px-7 pt-5 pb-3">
              <div class="flex-1 flex items-center gap-1.5" aria-hidden="true">
                <For each={STEPS}>
                  {(_, i) => (
                    <span
                      class="ob-dot h-1.5 rounded-full"
                      data-state={i() === idx() ? 'current' : i() < idx() ? 'done' : 'todo'}
                      style={{ width: i() === idx() ? '24px' : '8px' }}
                    />
                  )}
                </For>
              </div>
              <button
                type="button"
                onClick={skip}
                aria-label="跳过导览"
                class="p-1.5 -mr-1.5 rounded-lg text-content-tertiary hover:text-content hover:bg-surface-secondary transition-colors duration-fast cursor-pointer"
              >
                <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>

            {/* 屏内容 —— key 变化触发切屏淡入 */}
            <div class="flex-1 sm:flex-none overflow-y-auto px-6 sm:px-9 pb-2">
              <Show when={step()} keyed>
                {(s) => (
                  <div class="animate-tab-content-slide flex flex-col items-center text-center pt-4 pb-2">
                    <div class="ob-icon-wrap grid place-items-center size-16 rounded-2xl mb-5">
                      <StepIcon paths={s.iconPath} class="size-8" />
                    </div>
                    <h2
                      class="text-[22px] sm:text-[26px] font-bold tracking-[-0.02em] text-content"
                      style={{ 'font-family': 'var(--font-display)' }}
                    >
                      {s.title}
                    </h2>
                    <p class="mt-2 max-w-[46ch] text-[13.5px] leading-relaxed text-content-secondary">
                      {s.tagline}
                    </p>

                    <ul class="mt-6 w-full max-w-[460px] mx-auto flex flex-col gap-2.5 list-none text-left">
                      <For each={s.highlights}>
                        {(h) => (
                          <li class="flex items-start gap-2.5 text-[13.5px] text-content">
                            <span class="ob-check shrink-0 mt-px font-semibold tabular-nums">
                              {s.bullet ?? '✓'}
                            </span>
                            <span class="leading-snug">{h}</span>
                          </li>
                        )}
                      </For>
                    </ul>

                    <div class="mt-7 mb-3">
                      <Button
                        ref={primaryRef}
                        size="lg"
                        onClick={onPrimary}
                      >
                        {primaryLabel()}
                      </Button>
                    </div>
                  </div>
                )}
              </Show>
            </div>

            {/* 底部：上一步 / 步骤指示 / 下一步 */}
            <div class="flex items-center justify-between gap-3 px-5 sm:px-7 py-4 border-t border-border-hairline">
              <Button
                variant="ghost"
                size="sm"
                onClick={prev}
                disabled={isFirst()}
                aria-label="上一步"
              >
                <span class="flex items-center gap-1.5"><Kbd size="sm">←</Kbd> 上一步</span>
              </Button>

              <span
                class="text-[12px] text-content-tertiary tabular-nums"
                aria-current="step"
              >
                {idx() + 1} / {total}
              </span>

              <Button
                variant={isLast() ? 'primary' : 'outline'}
                size="sm"
                onClick={next}
                aria-label={isLast() ? '完成' : '下一步'}
              >
                <span class="flex items-center gap-1.5">
                  {isLast() ? '完成' : <>下一步 <Kbd size="sm">→</Kbd></>}
                </span>
              </Button>
            </div>
          </div>
        </div>
      </Portal>
    </Show>
  );
}
