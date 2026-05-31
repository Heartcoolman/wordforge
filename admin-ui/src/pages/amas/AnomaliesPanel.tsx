import { createResource, createSignal, For, Show } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { Drawer } from '@/components/ui/Drawer';
import { adminApi, type AmasAnomalyItem } from '@/api/admin';

function ago(iso: string): string {
  const t = new Date(iso).getTime();
  if (Number.isNaN(t)) return '';
  const s = Math.max(0, Math.floor((Date.now() - t) / 1000));
  if (s < 60) return `${s} 秒前`;
  if (s < 3600) return `${Math.floor(s / 60)} 分钟前`;
  if (s < 86400) return `${Math.floor(s / 3600)} 小时前`;
  return `${Math.floor(s / 86400)} 天前`;
}
function hms(iso: string): string {
  try {
    return new Date(iso).toLocaleTimeString('zh-CN', { hour12: false });
  } catch {
    return '';
  }
}
const SEV_LABEL: Record<string, string> = { error: 'error', warn: 'warn', info: 'info' };
const SEV_DESC: Record<string, string> = {
  error: '需立即处理 —— 强不变量被违反',
  warn: '24h 内可观察 —— 软不变量异常',
  info: '趋势观察 —— 弱不变量提示',
};
const sevColor = (s: string) =>
  s === 'error' ? 'text-error' : s === 'warn' ? 'text-warning' : 'text-info';

/** 异常详情抽屉:仅消费 /anomalies/feed 已有字段(id/code/value/expectedRange/...),无额外端点。 */
function DetailBody(props: { item: AmasAnomalyItem }) {
  const it = props.item;
  const Row = (p: { k: string; v: string; cls?: string }) => (
    <div class="flex items-baseline justify-between gap-4 py-2.5 border-b border-border-hairline last:border-0">
      <dt class="text-xs text-content-tertiary shrink-0">{p.k}</dt>
      <dd class={`text-[13px] text-content text-right break-all ${p.cls ?? ''}`}>{p.v}</dd>
    </div>
  );
  return (
    <dl>
      <div class="mb-3">
        <div class="text-[10.5px] font-mono uppercase tracking-wide text-content-tertiary">不变量违反</div>
        <code class="block mt-1 text-[13px] font-mono text-accent bg-surface-secondary rounded px-2 py-1 break-all">{it.code}</code>
      </div>
      <Row k="严重度" v={`${SEV_LABEL[it.severity]} · ${SEV_DESC[it.severity]}`} cls={sevColor(it.severity)} />
      <Row k="观测值" v={Number.isFinite(it.value) ? it.value.toFixed(4) : '—'} cls="font-mono" />
      <Row k="期望区间" v={it.expectedRange || '合法区间'} cls="font-mono" />
      <Row k="影响面" v={`${it.impactedUsers} users · ~${(it.impactPct * 100).toFixed(1)}%`} cls="font-mono" />
      <Row k="首例用户" v={it.userId.slice(0, 12)} cls="font-mono" />
      <Row k="事件 ID" v={it.id} cls="font-mono" />
      <Row k="发生时间" v={new Date(it.timestamp).toLocaleString('zh-CN', { hour12: false })} cls="font-mono" />
    </dl>
  );
}

/** 异常 / 不变量违反：严重度分级汇总(error/warn/info/不变量通过) + 逐条异常列表。
 * 真实数据来自 /api/admin/amas/anomalies/feed（解析 invariant_violations 首条定级）。
 * 行内操作:error/warn → 详情抽屉(读 feed 已有字段);info → 忽略(本地会话态,无后端 ack 端点)。 */
export function AnomaliesPanel() {
  const [days, setDays] = createSignal<7 | 14 | 30>(7);
  const [feed] = createResource(days, (d) => adminApi.amasAnomalyFeed(d, 50));
  const [detail, setDetail] = createSignal<AmasAnomalyItem | null>(null);
  // 忽略为本地会话态(无后端 ack 端点):按事件 id 隐藏,切换窗口/刷新后复现。
  const [dismissed, setDismissed] = createSignal<Set<string>>(new Set());
  const ignore = (id: string) => setDismissed((s) => new Set(s).add(id));

  return (
    <div class="space-y-3">
      <div class="flex items-center justify-between flex-wrap gap-2">
        <h2 class="text-lg font-semibold text-content">异常 / 不变量违反</h2>
        <div class="flex items-center gap-1.5" role="group" aria-label="选择时间窗口">
          <For each={[7, 14, 30] as const}>
            {(n) => (
              <button
                type="button"
                onClick={() => setDays(n)}
                aria-pressed={days() === n}
                aria-label={`使用 ${n} 天窗口`}
                class={`focus-ring-soft px-2.5 py-1 text-xs rounded-md transition-colors ${
                  days() === n ? 'bg-accent text-accent-content' : 'bg-surface-secondary text-content-secondary hover:text-content'
                }`}
              >
                {n} 天
              </button>
            )}
          </For>
        </div>
      </div>

      <Show when={!feed.loading && feed()} keyed fallback={<div class="min-h-[440px] flex items-center justify-center"><Spinner /></div>}>
        {(f) => (
          <>
            <div class="anom-summary">
              <div class="anom-stat is-bad"><div class="l">error 级</div><div class="v">{f.summary.error}</div><div class="meta">需立即处理</div></div>
              <div class="anom-stat is-warn"><div class="l">warn 级</div><div class="v">{f.summary.warn}</div><div class="meta">24h 内可观察</div></div>
              <div class="anom-stat"><div class="l">info 级</div><div class="v">{f.summary.info}</div><div class="meta">趋势观察</div></div>
              <div class="anom-stat is-ok">
                <div class="l">不变量通过</div>
                <div class="v">{f.summary.invariantPass} / {f.summary.invariantTotal}</div>
                <div class="meta">SLO {(f.summary.passRate * 100).toFixed(2)}%</div>
              </div>
            </div>

            <Show
              when={f.items.filter((it) => !dismissed().has(it.id)).length > 0}
              fallback={<Card variant="elevated"><Empty title="窗口内无异常事件" description="所有不变量校验通过（is_anomaly = 0）" /></Card>}
            >
              <For each={f.items.filter((it) => !dismissed().has(it.id))}>
                {(it) => (
                  <div class={`anom-item sev-${it.severity}`}>
                    <div class="sev" />
                    <div class="ts">{ago(it.timestamp)}<span>{hms(it.timestamp)}</span></div>
                    <div class="nm">
                      不变量违反：<code>{it.code}</code>{Number.isFinite(it.value) ? ` = ${it.value.toFixed(3)}` : ''}
                      <div class="d">期望 {it.expectedRange || '合法区间'} · 事件 {it.id.slice(0, 8)}</div>
                    </div>
                    <div class="impacted" title={`uid ${it.userId.slice(0, 8)}`}>{it.impactedUsers} users<span>~{(it.impactPct * 100).toFixed(1)}% 影响面</span></div>
                    <div class="ops">
                      <Show
                        when={it.severity === 'info'}
                        fallback={
                          <button type="button" class="focus-ring-soft" onClick={() => setDetail(it)} aria-label={`查看 ${it.code} 详情`}>
                            详情 →
                          </button>
                        }
                      >
                        <button type="button" class="focus-ring-soft" onClick={() => ignore(it.id)} aria-label={`忽略 ${it.code}`}>
                          忽略
                        </button>
                      </Show>
                    </div>
                  </div>
                )}
              </For>
            </Show>
          </>
        )}
      </Show>

      <Drawer open={detail() !== null} onClose={() => setDetail(null)} title="异常详情" width={400}>
        <Show when={detail()} keyed>
          {(it) => <DetailBody item={it} />}
        </Show>
      </Drawer>
    </div>
  );
}
