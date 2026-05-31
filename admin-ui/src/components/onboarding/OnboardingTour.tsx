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
  /** 路由；welcome / outro 无 route */
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

// 顺序即步骤顺序：欢迎屏 → 各功能屏 → 结束屏。
const STEPS: Step[] = [
  {
    key: 'welcome',
    title: 'WordForge Admin v1.1.2',
    tagline: '本次大版本升级了运维面的可观测、治理与分发能力 —— 用一分钟看看新增了什么',
    iconPath: 'M5 3l1.5 4.5L11 9l-4.5 1.5L5 15l-1.5-4.5L-1 9\nM18 9l1 3 3 1-3 1-1 3-1-3-3-1 3-1z\nM12 14l.9 2.7L15.6 18l-2.7.9L12 21.6l-.9-2.7L8.4 18l2.7-.9z',
    highlights: [
      '数据探针 · AMAS 指标看板 · LLM 调参顾问',
      '词库中心 · 反馈工单 · 资源包分发',
      '系统广播 · 用户与 RBAC 管控',
    ],
    accent: 'accent',
    bullet: '▸',
  },
  {
    key: 'probe',
    route: '/admin/probe',
    title: '数据探针',
    tagline: '采样治理 · 实时事件流 · 写入目标全景',
    iconPath: 'M3 12h3l2-5 4 14 3-9h6',
    highlights: [
      '4 派生探针聚合真实遥测源',
      '实时事件流与 KPI 指标看板',
      '多级采样规则可改并写审计',
      '写入目标与 Schema 预览',
    ],
    accent: 'info',
  },
  {
    key: 'amas-metrics',
    route: '/admin/amas-metrics',
    title: 'AMAS 指标看板',
    tagline: '决策算法、ELO、MDM、疲劳信号一屏可观测',
    iconPath: 'M3 3v18h18\nM7 14l3-4 3 3 4-6\nM7 17v.01\nM21 8h.01',
    highlights: [
      '四标签：算法延迟/错误率、版本对比、异常、用户状态',
      'ELO 散点 · MDM 热力 · 疲劳时序 · 决策直方图',
      '1h/24h/7d/30d 时间窗切换',
      '算法时序一键导出 CSV',
    ],
    accent: 'llm',
  },
  {
    key: 'amas-advisor',
    route: '/admin/amas-advisor',
    title: 'LLM 调参顾问',
    tagline: 'DeepSeek 巡查指标，自动产出可灰度的参数 patch',
    iconPath: 'M9 18h6M10 22h4M12 2a7 7 0 0 0-4 12.7c.6.5 1 1.2 1 2h6c0-.8.4-1.5 1-2A7 7 0 0 0 12 2Z',
    highlights: [
      '每 20 分钟跑 DeepSeek 对照 7 日指标',
      '白名单内自动灰度，超界待审批',
      '灰度可扩量/回滚/提升 stable',
      '成本看板 + 审计历史可一键回滚',
    ],
    accent: 'llm',
  },
  {
    key: 'wordbook-center',
    route: '/admin/wordbook-center',
    title: '词库中心',
    tagline: '官方与用户词库的统一管理与数据透视台',
    iconPath: 'M4 19.5A2.5 2.5 0 0 1 6.5 17H20\nM6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z',
    highlights: [
      '词条增删改、导出 JSON、批量录入',
      '看板透视掌握度、热图与用户分布',
      '远程目录一键导入/同步更新',
      'CSV/JSON 上传，含变更历史审计',
    ],
    accent: 'info',
  },
  {
    key: 'feedback',
    route: '/admin/feedback',
    title: '反馈工单',
    tagline: '客户端反馈统一收件箱，左列表右详情闭环处理',
    iconPath: 'M21 11.5a8.38 8.38 0 0 1-.9 3.8 8.5 8.5 0 0 1-7.6 4.7 8.38 8.38 0 0 1-3.8-.9L3 21l1.9-5.7a8.38 8.38 0 0 1-.9-3.8 8.5 8.5 0 0 1 4.7-7.6 8.38 8.38 0 0 1 3.8-.9h.5a8.48 8.48 0 0 1 8 8z',
    highlights: [
      '回复/分派/解决/合并去重四连操作',
      '一键转 GitHub Issue 同步研发',
      '草稿自动恢复 + 回复快捷文案',
      '未处理/响应时长/解决率/CSAT KPI',
    ],
    accent: 'info',
  },
  {
    key: 'resource-packs',
    route: '/admin/resource-packs',
    title: '资源包',
    tagline: '多包多版本三通道分发，签名校验加 SSE 实时激活',
    iconPath: 'M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z\nM3.27 6.96 12 12.01l8.73-5.05\nM12 22.08V12',
    highlights: [
      '上传服务端自动 SHA256 + Ed25519 签名',
      'Stable/Beta/Internal 三通道独立激活',
      '切激活经 SSE 广播在线客户端去重',
      '软删除可回滚加三态安装统计',
    ],
    accent: 'info',
  },
  {
    key: 'broadcast',
    route: '/admin/broadcast',
    title: '系统广播',
    tagline: '一处撰写，全员通知中心即时送达',
    iconPath: 'M3 11l18-5v12L3 14v-3z\nM11.6 16.8a3 3 0 1 1-5.8-1.6',
    highlights: [
      '撰写表单实时预览 + 四类模板',
      '近 30 天送达/阅读率/在线看板',
      '分批 100 入库，幂等 UUID 防重',
      '历史分页/筛选/详情/CSV 导出',
    ],
    accent: 'info',
  },
  {
    key: 'users',
    route: '/admin/users',
    title: '用户与 RBAC',
    tagline: '用户全生命周期管理与三级角色管控',
    iconPath: 'M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2\nM9 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8\nM22 11l-3 3-2-2',
    highlights: [
      '用户/员工/管理员三级角色',
      '搜索筛选 + 答题画像看板',
      '批量封禁/改角色/重置/删除',
      '密码重置与一次性密钥',
    ],
    accent: 'accent',
  },
  {
    key: 'outro',
    title: '随时可以再看一遍',
    tagline: '点击顶栏的「导览」按钮即可重新打开本导览。开始体验吧！',
    iconPath: 'M22 11.08V12a10 10 0 1 1-5.93-9.14\nM22 4 12 14.01l-3-3',
    highlights: [
      '顶栏 ⌘K 旁的「导览」按钮可随时重看',
      '侧边栏标 NEW 的入口为本次新增功能',
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
