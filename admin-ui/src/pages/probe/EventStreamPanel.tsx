import { createResource, createSignal, createMemo, For, Show, onMount, onCleanup } from 'solid-js';
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
 *  filter chip 按事件类型分类筛选；新事件 ev-in 动画。 */
export function EventStreamPanel() {
  const [data, { refetch }] = createResource(() => probeTelemetryApi.stream(STREAM_LIMIT));
  const [filter, setFilter] = createSignal('all');
  const [paused, setPaused] = createSignal(false);

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

  const events = createMemo<StreamEvent[]>(() => data()?.events ?? []);
  const filtered = createMemo(() => {
    const f = filter();
    const list = events();
    if (f === 'all') return list;
    return list.filter((e) => category(e.type) === f);
  });
  const typeCount = createMemo(() => new Set(events().map((e) => e.type)).size);

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
      <div class="pb-stream-body">
        <Show
          when={!data.loading}
          fallback={<div class="min-h-[120px] grid place-items-center"><Spinner /></div>}
        >
          <Show
            when={filtered().length > 0}
            fallback={<Empty title="暂无遥测事件" description="telemetry_events 表当前为空或无匹配筛选" />}
          >
            <For each={filtered()}>
              {(ev) => {
                const cat = category(ev.type);
                const evCls = cat === 'learn' ? ' is-learn' : cat === 'perf' ? ' is-perf' : cat === 'err' ? ' is-err' : '';
                return (
                  <div class={`pb-event${evCls}`}>
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
