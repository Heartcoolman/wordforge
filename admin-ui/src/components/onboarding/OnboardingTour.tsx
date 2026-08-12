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

// 顺序即步骤顺序：欢迎屏 → 功能亮点屏 → 结束屏。本波（v1.3.x）= 端侧埋点数据链路 + records 异步化。
const STEPS: Step[] = [
  {
    key: 'welcome',
    title: 'WordForge Admin v1.3.0',
    tagline: '端侧埋点数据链路上线 —— 三端行为 / 错误 / 性能事件统一落库，学习记录上报默认异步受理',
    iconPath: 'M5 3l1.5 4.5L11 9l-4.5 1.5L5 15l-1.5-4.5L-1 9\nM18 9l1 3 3 1-3 1-1 3-1-3-3-1 3-1z\nM12 14l.9 2.7L15.6 18l-2.7.9L12 21.6l-.9-2.7L8.4 18l2.7-.9z',
    highlights: [
      '三端埋点事件流上线 · 行为 / 错误 / 性能命名事件统一摄取落库',
      '8 个埋点分析接口 · 总览 / 趋势 / 错误分组 / 漏斗 / 留存 / 活跃度',
      '埋点采样独立可调 · 采样配置新增 app_behavior / app_perf 两行',
      '学习记录上报默认异步受理 · outbox 重试 + 死信兜底',
    ],
    accent: 'accent',
    bullet: '▸',
  },
  {
    key: 'probe-sampling',
    route: '/admin/probe',
    title: '埋点采样与摄取观测',
    tagline: '数据探针页可调埋点采样率、观测摄取拒绝分布',
    iconPath: 'M3 3v18h18\nM7 14l3-4 4 3 5-7',
    highlights: [
      '采样配置新增 app_behavior / app_perf 行，埋点量可独立调降',
      '错误类埋点恒不采样，线上问题信号不丢',
      '摄取拒绝码留痕照常覆盖新端点，异常上报一眼可见',
    ],
    accent: 'accent',
  },
  {
    key: 'records-async',
    route: '/admin/monitoring',
    title: '学习记录异步受理',
    tagline: '单条作答上报默认 202 受理，AMAS 处理走 outbox 异步消费',
    iconPath: 'M23 4v6h-6\nM1 20v-6h6\nM3.51 9a9 9 0 0 1 14.85-3.36L23 10\nM1 14l4.64 4.36A9 9 0 0 0 20.49 15',
    highlights: [
      '监控页可观测 outbox 消费与死信队列',
      '指数退避重试 + 幂等账本防重复应用',
      '设 RECORDS_OUTBOX_ASYNC=false 可回退同步老路',
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
