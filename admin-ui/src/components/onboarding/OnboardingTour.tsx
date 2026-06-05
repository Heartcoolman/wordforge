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

// 顺序即步骤顺序：欢迎屏 → 各功能屏 → 结束屏。本波（v1.1.4）= 运营闭环收口。
const STEPS: Step[] = [
  {
    key: 'welcome',
    title: 'WordForge Admin v1.1.4',
    tagline: '设备遥测「操作概览」+ v1.1.3 埋下的运营闭环收口 —— 用一分钟看看新增了什么',
    iconPath: 'M5 3l1.5 4.5L11 9l-4.5 1.5L5 15l-1.5-4.5L-1 9\nM18 9l1 3 3 1-3 1-1 3-1-3-3-1 3-1z\nM12 14l.9 2.7L15.6 18l-2.7.9L12 21.6l-.9-2.7L8.4 18l2.7-.9z',
    highlights: [
      '设备遥测「操作概览」· 点设备历史一眼看懂它做了什么',
      '事件 outbox 死信运维 · 人工重投 / 丢弃',
      '定时广播队列 · 可查看与取消待发',
      '离站备份可观测 · 每 target 上次成功 / 失败',
      '告警收件箱增强 · 自动回滚进箱 + 一键全部已读',
    ],
    accent: 'accent',
    bullet: '▸',
  },
  {
    key: 'device-telemetry-digest',
    route: '/admin/clients',
    title: '设备遥测「操作概览」',
    tagline: '点设备「历史」不用再逐条翻 —— 顶部直接看它做了什么',
    iconPath: 'M3 3v18h18\nM7 14l3-4 4 3 5-7',
    highlights: [
      '面板顶部「这台设备做了什么」全量聚合，一眼看懂',
      '功能使用排行 / 访问页面 / 点击热点 / 错误与事件分布 / 总时长会话',
      '设备画像提顶只显一次，原始记录折叠为明细',
      '展开明细按 event_type 分类筛选，点行看完整会话 / 行为 / 功能',
    ],
    accent: 'accent',
  },
  {
    key: 'dead-letter',
    route: '/admin/monitoring',
    title: '事件 outbox 死信运维',
    tagline: '死信不再只是个计数 —— 点开明细、人工重投或丢弃',
    iconPath: 'M22 12h-4l-3 9L9 3l-3 9H2\nM16 4l4 4-4 4',
    highlights: [
      '监控页死信 chip 可点开抽屉看明细',
      '明细含用户 / 事件类型 / 失败原因 / 进死信时间',
      '人工重投回 outbox（attempts 归零）或丢弃，均二次确认',
      '毒丸消息（解析失败 / 未知类型）跳退避直接进死信',
    ],
    accent: 'llm',
  },
  {
    key: 'scheduled-broadcast',
    route: '/admin/clients',
    title: '定时广播队列 · 查看与取消',
    tagline: '误排的未来广播到点必发？现在可在队列里撤销',
    iconPath: 'M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20z\nM12 6v6l4 2',
    highlights: [
      '推送区新增「待发排程」队列视图',
      '每条显示标题 / 受众 / 计划下发时间',
      '一键取消待发排程（二次确认）',
      '已被 worker 到点下发的返回 409，不会误标取消',
    ],
    accent: 'info',
  },
  {
    key: 'backup-observable',
    route: '/admin/settings',
    title: '离站备份可观测',
    tagline: '配了 s3 / rsync 不知有没有传上去？现在每 target 状态可验证',
    iconPath: 'M4 7c0-1.7 3.6-3 8-3s8 1.3 8 3-3.6 3-8 3-8-1.3-8-3z\nM4 7v10c0 1.7 3.6 3 8 3s8-1.3 8-3V7\nM4 12c0 1.7 3.6 3 8 3s8-1.3 8-3',
    highlights: [
      '设置 · 备份面板每 target 显示上次成功 / 失败',
      '成功记上次时间与字节数，失败记原因',
      'backup_offsite 心跳上 worker 列表与 Prometheus',
      '灾备从「配了不知有没有用」变为可验证',
    ],
    accent: 'accent',
  },
  {
    key: 'inbox-plus',
    title: '告警收件箱增强',
    tagline: 'canary 自动回滚进箱可追溯，未读积压一键清空',
    iconPath: 'M18 8a6 6 0 0 0-12 0c0 7-3 9-3 9h18s-3-2-3-9\nM13.73 21a2 2 0 0 1-3.46 0',
    highlights: [
      'AMAS patch canary 自动回滚现写入收件箱（持久可追溯）',
      '不再只发瞬态 SSE、关掉监控页就错过',
      '收件箱面板头新增「全部已读」一键清空角标',
    ],
    accent: 'info',
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
