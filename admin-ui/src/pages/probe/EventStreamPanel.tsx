import { createResource, createSignal, createMemo, createEffect, on, For, Show, onMount, onCleanup } from 'solid-js';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { probeTelemetryApi } from '@/api/probeTelemetry';
import type { StreamEvent } from '@/types/probeTelemetry';
import { hms } from './util';
import { eventTypeLabel, metricLabel, metricUnit } from './readable';

const STREAM_LIMIT = 30;
const POLL_MS = 5000;

/** 事件 type → 过滤分类（决定颜色 + 中文标签筛选）。 */
function category(type: string): 'learn' | 'perf' | 'err' | 'behavior' {
  const t = type.toLowerCase();
  if (t.includes('error') || t.includes('err')) return 'err';
  // session_start 是会话/行为遥测(非学习记录),归 behavior;真正的学习数据走 learning_* 不进本流。
  if (t.includes('lesson') || t.includes('word') || t.includes('answer') || t.includes('review')) return 'learn';
  if (t.includes('perf') || t.includes('resource') || t.includes('nav')) return 'perf';
  return 'behavior';
}

const CHIPS: Array<{ id: string; label: string }> = [
  { id: 'all', label: '全部' },
  { id: 'behavior', label: '行为' },
  { id: 'learn', label: '学习' },
  { id: 'perf', label: '性能' },
  { id: 'err', label: '错误' },
];

/** 右栏：实时事件流。后端已把每条 payload 解析成人话（设备 / 系统 / 在线 / 关键指标），
 *  客服可直接读懂；点"原始"展开完整 JSON。每 5 秒轮询；新事件 ev-in 动画。
 *  刷新不跳/不闪：Spinner 仅首帧；按 id 复用未变行引用；提交瞬间锚定滚动。 */
export function EventStreamPanel() {
  const [data, { refetch }] = createResource(() => probeTelemetryApi.stream(STREAM_LIMIT));
  const [filter, setFilter] = createSignal('all');
  const [paused, setPaused] = createSignal(false);
  const [view, setView] = createSignal<StreamEvent[]>([]);
  const [expanded, setExpanded] = createSignal<Set<string>>(new Set());

  let bodyRef: HTMLDivElement | undefined;

  const toggleRaw = (id: string) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

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

  createEffect(on(incoming, (next) => {
    const prev = view();
    const merged = prev.length
      ? (() => { const byId = new Map(prev.map((e) => [e.id, e])); return next.map((e) => byId.get(e.id) ?? e); })()
      : next;

    const el = bodyRef;
    if (!el || !prev.length) { setView(merged); return; }

    const atTop = el.scrollTop <= 8;
    const anchorId = atTop ? null : firstVisibleId(el);
    const top0 = anchorId ? rowTop(el, anchorId) : null;

    setView(merged);

    if (atTop) {
      el.scrollTop = 0;
    } else if (anchorId && top0 != null) {
      const top1 = rowTop(el, anchorId);
      if (top1 != null) el.scrollTop += top1 - top0;
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
              aria-pressed={filter() === chip.id}
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
            fallback={<><svg width="11" height="11" viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="4" width="4" height="16" /><rect x="14" y="4" width="4" height="16" /></svg> 暂停</>}
          >
            <svg width="11" height="11" viewBox="0 0 24 24" fill="currentColor"><path d="M8 5v14l11-7z" /></svg> 继续
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
            fallback={<Empty title="暂无遥测事件" description="当前没有设备上报，或无匹配筛选" />}
          >
            <For each={rendered()}>
              {(ev) => <EventCard ev={ev} expanded={expanded().has(ev.id)} onToggle={() => toggleRaw(ev.id)} />}
            </For>
          </Show>
        </Show>
      </div>
    </aside>
  );
}

/** 单条事件人话卡片：时间 + 中文类型 + 设备摘要 + 关键指标 + 原始 JSON 折叠。 */
function EventCard(props: { ev: StreamEvent; expanded: boolean; onToggle: () => void }) {
  const ev = () => props.ev;
  const cat = () => category(ev().type);
  const evCls = () => {
    const c = cat();
    return c === 'learn' ? ' is-learn' : c === 'perf' ? ' is-perf' : c === 'err' ? ' is-err' : '';
  };
  const dev = () => ev().device;
  const hasBody = () => !!dev() || ev().metrics.length > 0;

  return (
    <div class={`pb-event${evCls()}`} data-eid={ev().id}>
      <div class="pb-ev-head">
        <span class="ts">{hms(ev().ts)}</span>
        <span class="type">{eventTypeLabel(ev().type)}</span>
        <span class="dev">设备 {ev().deviceId.slice(0, 8)}…</span>
        <button type="button" class={`pb-ev-raw${props.expanded ? ' is-on' : ''}`} onClick={props.onToggle}>
          {props.expanded ? '收起' : '原始'}
        </button>
      </div>

      <Show when={dev()}>
        {(d) => (
          <div class="pb-ev-device">
            <span class="ic">📱</span>
            <span class="txt">
              {[d().os, d().model].filter(Boolean).join(' · ') || '未知设备'}
              <Show when={d().language}>{(l) => <span class="muted"> · {l()}</span>}</Show>
            </span>
            <Show when={d().online != null}>
              <span class={`pb-ev-online${d().online ? ' is-on' : ' is-off'}`}>{d().online ? '在线' : '离线'}</span>
            </Show>
          </div>
        )}
      </Show>

      <Show when={ev().metrics.length > 0}>
        <div class="pb-ev-metrics">
          <For each={ev().metrics}>
            {(m) => (
              <span class="m">
                <span class="mk">{metricLabel(m.key)}</span>
                <span class="mv">{m.value}{metricUnit(m.key)}</span>
              </span>
            )}
          </For>
        </div>
      </Show>

      <Show when={!hasBody()}>
        <div class="pb-ev-empty">无可解析的结构化字段</div>
      </Show>

      <Show when={props.expanded}>
        <pre class="pb-ev-rawjson">{prettyJson(ev().payloadRaw)}</pre>
      </Show>
    </div>
  );
}

/** 原始 payload 美化输出：能 parse 就缩进两格，否则原样返回。 */
function prettyJson(raw: string): string {
  try {
    return JSON.stringify(JSON.parse(raw), null, 2);
  } catch {
    return raw;
  }
}
