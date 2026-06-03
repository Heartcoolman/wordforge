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

// 顺序即步骤顺序：欢迎屏 → 各功能屏 → 结束屏。本波（v1.1.3）= 运维闭环新功能。
const STEPS: Step[] = [
  {
    key: 'welcome',
    title: 'WordForge Admin v1.1.3',
    tagline: '本次更新聚焦运维闭环 —— 用一分钟看看新增了什么',
    iconPath: 'M5 3l1.5 4.5L11 9l-4.5 1.5L5 15l-1.5-4.5L-1 9\nM18 9l1 3 3 1-3 1-1 3-1-3-3-1 3-1z\nM12 14l.9 2.7L15.6 18l-2.7.9L12 21.6l-.9-2.7L8.4 18l2.7-.9z',
    highlights: [
      '应用内告警收件箱 · 顶栏未读角标',
      '设备推送定时调度 + 草稿存储',
      '客户端版本门控 · 事件 outbox 异步可观测',
    ],
    accent: 'accent',
    bullet: '▸',
  },
  {
    key: 'notifications',
    title: '告警收件箱',
    tagline: '顶栏铃铛集中 AMAS 软拦截等系统告警，未读角标 + 一键已读',
    iconPath: 'M18 8a6 6 0 0 0-12 0c0 7-3 9-3 9h18s-3-2-3-9\nM13.73 21a2 2 0 0 1-3.46 0',
    highlights: [
      '顶栏铃铛集中查看系统告警 / 通知',
      '未读数角标，新告警实时累计',
      '点条目标记已读，持久化不重复提醒',
      '告警不再只能轮询监控页时间线',
    ],
    accent: 'info',
  },
  {
    key: 'device-push',
    route: '/admin/clients',
    title: '设备推送 · 定时调度与草稿',
    tagline: '推送编辑器支持立即 / 指定时间下发，草稿随存随取',
    iconPath: 'M3 11l18-5v12L3 14v-3z\nM11.6 16.8a3 3 0 1 1-5.8-1.6',
    highlights: [
      '投递时机：立即 / 指定时间定时下发',
      '到期由后台 worker 自动 fan-out',
      '编辑中一键保存草稿，下次自动恢复',
      '受众过滤非法 / 0 人时即时可操作提示',
    ],
    accent: 'info',
  },
  {
    key: 'version-gate',
    route: '/admin/clients',
    title: '客户端版本门控',
    tagline: 'admin 运行时设最低客户端版本，即时切流拒旧端',
    iconPath: 'M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z\nM9 12l2 2 4-4',
    highlights: [
      '运行时配置 min_client_version，免改 env 重发版',
      '门控开关独立生效，低于阈值返回 CLIENT_OUTDATED',
      'semver 前端预校验 + 实际生效阈值展示',
    ],
    accent: 'accent',
  },
  {
    key: 'outbox',
    route: '/admin/monitoring',
    title: '事件 outbox 可观测',
    tagline: 'records→AMAS 领域事件异步消费链路上监控页',
    iconPath: 'M22 12h-4l-3 9L9 3l-3 9H2',
    highlights: [
      '监控页新增 outbox 待处理 / lag / 死信展示',
      '异步消费 worker 指数退避重试 + 死信兜底',
      '默认走同步老路，异步路径按开关 opt-in',
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
