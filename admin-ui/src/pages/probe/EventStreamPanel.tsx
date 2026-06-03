import { createResource, createSignal, createMemo, createEffect, on, For, Show, onMount, onCleanup } from 'solid-js';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { probeTelemetryApi } from '@/api/probeTelemetry';
import type { StreamEvent } from '@/types/probeTelemetry';
import { hms } from './util';

const STREAM_LIMIT = 30;
const POLL_MS = 5000;

/** 事件 type → 过滤分类（与设计稿 chip 对齐，剔除虚构 biz）。 */
function category(type: string): 'learn' | 'perf' | 'err' | 'behavior' {
  const t = type.toLowerCase();
  if (t.includes('error') || t.includes('err')) return 'err';
  if (t.includes('lesson') || t.includes('word') || t.includes('answer') || t.includes('session') || t.includes('review')) return 'learn';
  if (t.includes('perf') || t.includes('resource') || t.includes('nav')) return 'perf';
  return 'behavior';
}

const CHIPS: Array<{ id: string; label: string }> = [
  { id: 'all', label: 'All' },
  { id: 'behavior', label: 'behavior' },
  { id: 'learn', label: 'learn' },
  { id: 'perf', label: 'perf' },
  { id: 'err', label: 'err' },
];

/** 右栏：实时事件流。每 5 秒轮询 /stream（非真实 SSE，头部文案诚实标注）。
 *  filter chip 按事件类型分类筛选；新事件 ev-in 动画。
 *
 *  刷新不跳/不闪的关键：
 *   1. Spinner 仅首帧（view 为空）显示，refetch 期间不再闪回 loading；
 *   2. 渲染快照 view 与 resource 解耦，按 id 复用未变行的对象引用 → <For> 不重建其 DOM、不重放动画；
 *   3. 提交新数据的同步窗口内（尚未绘制）量"首个可见行"位移并用 scrollTop 补偿，
 *      停在顶部贴顶看最新、滚下去看历史则锚点不动。 */
export function EventStreamPanel() {
  const [data, { refetch }] = createResource(() => probeTelemetryApi.stream(STREAM_LIMIT));
  const [filter, setFilter] = createSignal('all');
  const [paused, setPaused] = createSignal(false);
  // 已提交渲染的事件快照。与 resource 解耦，以便在数据切换瞬间夹住滚动位置。
  const [view, setView] = createSignal<StreamEvent[]>([]);

  let bodyRef: HTMLDivElement | undefined;

  // 轮询定时器在 onMount 内创建、onCleanup 内清除，保证每实例仅一个且卸载即回收。
  // 暂停时跳过 refetch（真实暂停轮询，非假按钮）；恢复时立即拉一次再继续节奏。
  onMount(() => {
    const timer = setInterval(() => { if (!paused()) void refetch(); }, POLL_MS);
    onCleanup(() => clearInterval(timer));
  });
  const togglePause = () => {
    const next = !paused();
    setPaused(next);
    if (!next) void refetch();
  };

  const incoming = createMemo<StreamEvent[]>(() => data()?.events ?? []);

  // 锚点测量：用视口坐标（getBoundingClientRect）规避 offsetParent 歧义。
  const firstVisibleId = (el: HTMLElement): string | null => {
    const top = el.getBoundingClientRect().top;
    for (const r of el.querySelectorAll<HTMLElement>('[data-eid]')) {
      if (r.getBoundingClientRect().bottom > top + 1) return r.dataset.eid ?? null;
    }
    return null;
  };
  const rowTop = (el: HTMLElement, id: string): number | null => {
    const r = el.querySelector<HTMLElement>(`[data-eid="${CSS.escape(id)}"]`);
    return r ? r.getBoundingClientRect().top : null;
  };

  // incoming 变化即提交：复用未变行引用，再锚定滚动。on() 的回调在 untrack 内执行，
  // 故读/写 view 不会自激成环；仅 incoming（resource 解析出新数组）才触发。
  createEffect(on(incoming, (next) => {
    const prev = view();
    const merged = prev.length
      ? (() => { const byId = new Map(prev.map((e) => [e.id, e])); return next.map((e) => byId.get(e.id) ?? e); })()
      : next;

    const el = bodyRef;
    if (!el || !prev.length) { setView(merged); return; } // 首次填充无需锚定

    const atTop = el.scrollTop <= 8;
    const anchorId = atTop ? null : firstVisibleId(el);
    const top0 = anchorId ? rowTop(el, anchorId) : null;

    setView(merged); // 同步重渲染，此刻 DOM 已更新但尚未绘制

    if (atTop) {
      el.scrollTop = 0; // 贴顶：始终展示最新
    } else if (anchorId && top0 != null) {
      const top1 = rowTop(el, anchorId);
      if (top1 != null) el.scrollTop += top1 - top0; // 把锚点行拉回原视口位置
    }
  }));

  const rendered = createMemo(() => {
    const f = filter();
    const list = view();
    return f === 'all' ? list : list.filter((e) => category(e.type) === f);
  });
  const typeCount = createMemo(() => new Set(view().map((e) => e.type)).size);
  const firstLoading = createMemo(() => data.loading && view().length === 0);

  return (
    <aside class="pb-stream animate-fade-in-up" style={{ 'animation-delay': '80ms' }}>
      <div class="pb-stream-head">
        <span class={`live-dot${paused() ? ' is-paused' : ''}`} />
        <h3>实时事件流</h3>
        <span class="rate">{paused() ? '已暂停' : '每 5 秒刷新'} · <b>{typeCount()}</b> 类型</span>
      </div>
      <div class="pb-stream-tools">
        <For each={CHIPS}>
          {(chip) => (
            <span
              class={`filter-chip${filter() === chip.id ? ' is-on' : ''}`}
              role="button"
              tabindex="0"
              onClick={() => setFilter(chip.id)}
              onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') setFilter(chip.id); }}
            >
              {chip.label}
            </span>
          )}
        </For>
        <button type="button" class="pause" aria-pressed={paused()} onClick={togglePause}>
          <Show
            when={paused()}
            fallback={<><svg width="11" height="11" viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="4" width="4" height="16" /><rect x="14" y="4" width="4" height="16" /></svg> Pause</>}
          >
            <svg width="11" height="11" viewBox="0 0 24 24" fill="currentColor"><path d="M8 5v14l11-7z" /></svg> Resume
          </Show>
        </button>
      </div>
      <div class="pb-stream-body" ref={bodyRef}>
        <Show
          when={!firstLoading()}
          fallback={<div class="min-h-[120px] grid place-items-center"><Spinner /></div>}
        >
          <Show
            when={rendered().length > 0}
            fallback={<Empty title="暂无遥测事件" description="telemetry_events 表当前为空或无匹配筛选" />}
          >
            <For each={rendered()}>
              {(ev) => {
                const cat = category(ev.type);
                const evCls = cat === 'learn' ? ' is-learn' : cat === 'perf' ? ' is-perf' : cat === 'err' ? ' is-err' : '';
                return (
                  <div class={`pb-event${evCls}`} data-eid={ev.id}>
                    <span class="ts">{hms(ev.ts)}</span>
                    <span class="type">{ev.type}</span>
                    <span class="payload">
                      <span class="dev">{ev.deviceId.slice(0, 10)}…</span> {ev.payloadPreview}
                    </span>
                  </div>
                );
              }}
            </For>
          </Show>
        </Show>
      </div>
    </aside>
  );
}
