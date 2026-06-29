import { For, Show, createEffect, createMemo, createSignal, onCleanup, onMount } from 'solid-js';
import { Portal } from 'solid-js/web';
import { useNavigate } from '@solidjs/router';
import { cn } from '@/utils/cn';
import { Kbd } from './Kbd';

export interface CommandItem {
  /** 唯一 id，用于 keyed render */
  id: string;
  /** 显示名 */
  title: string;
  /** 副说明 */
  desc?: string;
  /** 模糊匹配关键词（中英都加） */
  keywords?: string;
  /** 触发动作：href 跳转；或 onSelect 自定义 */
  href?: string;
  onSelect?: () => void;
  /** 左侧标记色，默认灰；'accent' 高亮新功能 */
  mark?: 'default' | 'accent' | 'warning';
}

interface CommandPaletteProps {
  /** 可选：外部受控开关（如 sidebar 搜索按钮触发） */
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  /** 默认命令清单（17 屏 + 系统动作）；外部可覆盖 */
  items?: CommandItem[];
}

/** 默认 17 屏导航 + 常用 action（与 admin.js:73-92 对齐） */
const defaultItems: CommandItem[] = [
  { id: 'dashboard', title: '全局概览', desc: 'Dashboard · KPI · 系统状态', href: '/admin', keywords: 'dashboard overview kpi 仪表盘 首页' },
  { id: 'users', title: '用户管理', desc: '用户列表 · 角色 / 状态 / 批量操作', href: '/admin/users', keywords: 'users members 用户' },
  { id: 'clients', title: '设备管理', desc: 'Web · iOS · Android · 版本分布', href: '/admin/clients', keywords: 'clients devices 设备 客户端' },
  { id: 'broadcast', title: '系统广播', desc: '在线推送 · 模板 · 送达统计', href: '/admin/broadcast', keywords: 'broadcast announce 广播 公告 推送', mark: 'accent' },
  { id: 'amas-config', title: 'AMAS 调参', desc: 'TOML 编辑 · 灰度发布 · 回滚', href: '/admin/amas-config', keywords: 'amas config toml 调参' },
  { id: 'amas-metrics', title: 'AMAS 指标', desc: 'ELO / MDM / 疲劳 / 决策算法', href: '/admin/amas-metrics', keywords: 'amas metrics elo mdm 指标' },
  { id: 'amas-advisor', title: 'LLM 调参顾问', desc: 'DeepSeek patch · 成本 · 灰度', href: '/admin/amas-advisor', keywords: 'llm advisor deepseek amas 顾问' },
  { id: 'vitals', title: '服务体征', desc: '实时脉冲 · 可用性 · 子系统状态', href: '/admin/vitals', keywords: 'vitals health pulse heartbeat 体征 健康 心跳 脉冲' },
  { id: 'monitoring', title: '系统监控', desc: 'SLO · workers · 实时日志', href: '/admin/monitoring', keywords: 'monitor logs slo 监控 worker' },
  { id: 'analytics', title: '数据分析', desc: '留存 · 漏斗 · 时段 · 导出', href: '/admin/analytics', keywords: 'analytics retention 分析' },
  { id: 'wordbook', title: '词库中心', desc: '官方 / 用户词库 · 导入', href: '/admin/wordbook-center', keywords: 'wordbook vocab 词库' },
  { id: 'feedback', title: '用户反馈', desc: '工单流转 · 详情 · 批量打标', href: '/admin/feedback', keywords: 'feedback tickets 反馈' },
  { id: 'probe-metrics', title: '数据探针', desc: '采样治理 · 实时事件流 · sinks · schema', href: '/admin/probe', keywords: 'probe telemetry sampling 数据探针 采样', mark: 'accent' },
  { id: 'probe', title: '远程探针', desc: 'Probe · ringbuffer · 现场重放', href: '/admin/remote-probe', keywords: 'probe debug 探针 repl' },
  { id: 'updates', title: '版本与自更新', desc: 'GitHub release · 一键更新', href: '/admin/updates', keywords: 'updates version 更新 发版' },
  { id: 'resource-packs', title: '资源包', desc: 'Stable / Beta 通道 · 上传 · 激活', href: '/admin/resource-packs', keywords: 'resource packs assets 资源包 通道', mark: 'accent' },
  { id: 'web-release', title: 'Web 发版', desc: 'web 客户端 tar.gz · 上传 · 签名 · 发布 / 回滚', href: '/admin/web-release', keywords: 'web release deploy 发版 web-app 客户端 前端 上线 回滚', mark: 'accent' },
  { id: 'settings', title: '系统设置', desc: 'JWT · 备份 · 邮件 · 维护模式', href: '/admin/settings', keywords: 'settings config 设置' },
  { id: 'login', title: '管理员登录', desc: 'Login', href: '/admin/login', keywords: 'login signin 登录' },
  { id: 'setup', title: '初始化向导', desc: 'Setup · 首次部署引导', href: '/admin/setup', keywords: 'setup init 首次 部署 引导' },
];

/**
 * 全局命令面板（⌘K / Ctrl+K）。
 *
 * 行为：
 * - 自身监听 ⌘K / Ctrl+K 切换开关；Esc 关闭
 * - 也接受外部受控（open + onOpenChange）
 * - 还接受 `[data-palette-open]` 元素点击触发（与设计交付 admin.js 一致）
 * - 模糊匹配 `(title + desc + keywords).includes(query)`，大小写不敏感
 * - ↑↓ 移动选中项，Enter 跳转，鼠标 hover 同步选中
 *
 * 实现选择 Portal + 自定义 CSS（沿用设计交付 admin.js wfp-* 视觉规范，
 * 但去掉裸 DOM 操作改为 Solid 受控渲染）。
 */
export function CommandPalette(props: CommandPaletteProps) {
  const navigate = useNavigate();
  const [internalOpen, setInternalOpen] = createSignal(false);
  const open = () => props.open ?? internalOpen();
  const setOpen = (v: boolean) => {
    if (props.onOpenChange) props.onOpenChange(v);
    else setInternalOpen(v);
  };

  const [query, setQuery] = createSignal('');
  const [activeIdx, setActiveIdx] = createSignal(0);
  let inputRef: HTMLInputElement | undefined;
  let listRef: HTMLUListElement | undefined;

  const items = () => props.items ?? defaultItems;

  const filtered = createMemo<CommandItem[]>(() => {
    const q = query().toLowerCase().trim();
    if (!q) return items();
    return items().filter((it) => {
      const hay = `${it.title}\n${it.desc ?? ''}\n${it.keywords ?? ''}`.toLowerCase();
      return hay.includes(q);
    });
  });

  // 列表变化时把 activeIdx 拉回 0
  createEffect(() => {
    filtered();
    setActiveIdx(0);
  });

  // 打开时聚焦输入、清空 query
  createEffect(() => {
    if (open()) {
      setQuery('');
      setActiveIdx(0);
      // 等待 Portal mount + animation frame
      queueMicrotask(() => inputRef?.focus());
    }
  });

  // 选中项滚动可见
  createEffect(() => {
    if (!open()) return;
    const idx = activeIdx();
    const el = listRef?.querySelectorAll<HTMLLIElement>('[data-cmd-item]')[idx];
    el?.scrollIntoView({ block: 'nearest' });
  });

  const select = (it: CommandItem) => {
    setOpen(false);
    if (it.onSelect) {
      it.onSelect();
    } else if (it.href) {
      navigate(it.href);
    }
  };

  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      setOpen(false);
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      const list = filtered();
      if (list.length === 0) return;
      setActiveIdx((i) => (i + 1) % list.length);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      const list = filtered();
      if (list.length === 0) return;
      setActiveIdx((i) => (i - 1 + list.length) % list.length);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      const it = filtered()[activeIdx()];
      if (it) select(it);
    }
  };

  // 全局 ⌘K / Ctrl+K 监听 + `[data-palette-open]` 点击触发
  onMount(() => {
    const onGlobalKey = (e: KeyboardEvent) => {
      const isToggle = (e.metaKey || e.ctrlKey) && (e.key === 'k' || e.key === 'K');
      if (isToggle) {
        e.preventDefault();
        setOpen(!open());
      }
    };
    const onClick = (e: MouseEvent) => {
      const t = e.target as HTMLElement | null;
      if (t?.closest('[data-palette-open]')) {
        e.preventDefault();
        setOpen(true);
      }
    };
    document.addEventListener('keydown', onGlobalKey);
    document.addEventListener('click', onClick);
    onCleanup(() => {
      document.removeEventListener('keydown', onGlobalKey);
      document.removeEventListener('click', onClick);
    });
  });

  return (
    <Show when={open()}>
      <Portal>
        <div
          class="fixed inset-0 z-[300] flex items-start justify-center pt-[14vh] animate-fade-in"
          role="dialog"
          aria-modal="true"
          aria-label="命令面板"
        >
          {/* backdrop */}
          <div
            class="absolute inset-0 backdrop-blur-md"
            style={{ background: 'color-mix(in oklab, var(--content) 40%, transparent)' }}
            onClick={() => setOpen(false)}
          />

          {/* modal */}
          <div
            class={cn(
              'relative w-[min(560px,calc(100vw-32px))] overflow-hidden rounded-xl',
              'bg-surface-elevated shadow-elevation-3 animate-scale-in',
            )}
          >
            {/* input row */}
            <div class="flex items-center gap-2.5 border-b border-border-hairline px-3.5 py-3 text-content-tertiary">
              <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2">
                <circle cx="11" cy="11" r="7" />
                <path d="m21 21-4.3-4.3" />
              </svg>
              <input
                ref={inputRef}
                type="text"
                class="flex-1 bg-transparent text-[14.5px] text-content placeholder:text-content-tertiary focus:outline-none"
                placeholder="跳转到任意页面或操作…"
                value={query()}
                onInput={(e) => setQuery(e.currentTarget.value)}
                onKeyDown={onKeyDown}
              />
              <Kbd>ESC</Kbd>
            </div>

            {/* list */}
            <ul
              ref={listRef}
              class="max-h-[48vh] list-none overflow-auto p-1.5"
              role="listbox"
              aria-activedescendant={`cmd-item-${activeIdx()}`}
            >
              <Show
                when={filtered().length > 0}
                fallback={
                  <li class="px-6 py-6 text-center text-[13px] text-content-tertiary">无匹配项</li>
                }
              >
                <For each={filtered()}>
                  {(it, i) => (
                    <li
                      id={`cmd-item-${i()}`}
                      data-cmd-item
                      role="option"
                      aria-selected={i() === activeIdx()}
                      class={cn(
                        'flex cursor-pointer items-center gap-3 rounded-md px-2.5 py-2.5',
                        'text-content-secondary',
                        i() === activeIdx() && 'bg-surface-secondary text-content',
                      )}
                      onMouseEnter={() => setActiveIdx(i())}
                      onClick={() => select(it)}
                    >
                      <span
                        aria-hidden="true"
                        class={cn(
                          'inline-block size-1.5 shrink-0 rounded-full',
                          i() === activeIdx()
                            ? 'bg-accent'
                            : it.mark === 'accent'
                              ? 'bg-accent/60'
                              : it.mark === 'warning'
                                ? 'bg-warning'
                                : 'bg-content-quaternary',
                        )}
                      />
                      <div class="min-w-0 flex-1">
                        <div class="truncate text-[13.5px] font-medium">{it.title}</div>
                        <Show when={it.desc}>
                          <div class="mt-0.5 truncate text-[11.5px] text-content-tertiary">{it.desc}</div>
                        </Show>
                      </div>
                      <svg
                        class="size-3.5 shrink-0 text-content-tertiary"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                      >
                        <path d="M9 18l6-6-6-6" />
                      </svg>
                    </li>
                  )}
                </For>
              </Show>
            </ul>

            {/* footer */}
            <div class="flex items-center justify-between gap-3 border-t border-border-hairline px-3.5 py-2 text-[11.5px] text-content-tertiary">
              <span class="flex items-center gap-1.5">
                <Kbd size="sm">↑↓</Kbd> 移动
              </span>
              <span class="flex items-center gap-1.5">
                <Kbd size="sm">↵</Kbd> 打开
              </span>
              <span class="flex items-center gap-1.5">
                <Kbd size="sm">⌘K</Kbd> 关闭
              </span>
            </div>
          </div>
        </div>
      </Portal>
    </Show>
  );
}
