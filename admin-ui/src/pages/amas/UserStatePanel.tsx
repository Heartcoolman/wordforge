import { createResource, For, Show } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { adminApi, type AmasStageStat } from '@/api/admin';
import { algoMeta } from './algoMeta';

const STAGE_CLASS: Record<string, string> = { cold: 'stage-cold', transition: 'stage-tran', stable: 'stage-stab' };
const STAGE_TITLE: Record<string, string> = { cold: '冷启动', transition: '过渡期', stable: '稳定阶段' };
const STAGE_RULE: Record<string, string> = {
  cold: 'decision_count < 15',
  transition: '15 ≤ decision_count < 200',
  stable: 'decision_count ≥ 200',
};
const STAGE_ORDER = ['cold', 'transition', 'stable'];

const BAR_COLORS = ['var(--accent)', 'var(--info)', 'var(--info-strong)', 'var(--success)', 'var(--success-strong)', 'var(--warning-strong)', 'var(--error-strong)'];

const FLOW_META: Record<string, { label: string; color: string }> = {
  'cold>transition': { label: '冷 → 过渡', color: 'var(--info)' },
  'transition>stable': { label: '过渡 → 稳定', color: 'var(--success)' },
  'stable>transition': { label: '稳定 → 过渡', color: 'var(--warning)' },
  'new>cold': { label: '新注册 → 冷', color: 'var(--accent)' },
};
const CLUSTER_COLORS = ['oklch(58% 0.20 269)', 'var(--success-strong)', 'var(--warning-strong)', 'var(--info-strong)'];

const find = (stages: AmasStageStat[] | undefined, s: string) => (stages ?? []).find((x) => x.stage === s);

/** 用户状态分布：3 阶段卡(冷/过渡/稳定) + 每用户决策直方图 + 状态流转(24h) + 学习风格聚类(k=4)。
 * 全部真实聚合，对齐设计稿 amas-metrics.html 的 user-state pane。 */
export function UserStatePanel() {
  const [stage] = createResource(() => adminApi.amasStageDistribution());
  const [hist] = createResource(() => adminApi.amasDecisionHistogram(7));
  const [flow] = createResource(() => adminApi.amasStateTransitions(24));
  const [clusters] = createResource(() => adminApi.amasLearningClusters());

  return (
    <div class="space-y-4">
      {/* 3 阶段卡 */}
      <Show when={!stage.loading} fallback={<div class="min-h-[180px] flex items-center justify-center"><Spinner /></div>}>
        <Show when={(stage()?.totalUsers ?? 0) > 0} fallback={<Card variant="elevated"><Empty title="暂无用户状态" description="engine_user_states 当前为空" /></Card>}>
          <div class="us-stages">
            <For each={STAGE_ORDER}>
              {(key) => {
                const s = find(stage()?.stages, key);
                return (
                  <div class={`us-stage ${STAGE_CLASS[key]}`}>
                    <div class="l">{STAGE_TITLE[key]}</div>
                    <div class="snm">{STAGE_RULE[key]}</div>
                    <div class="v">{(s?.users ?? 0).toLocaleString()}<small>用户</small></div>
                    <div class="meta">
                      <span class="k">占比</span><span class="vv">{((s?.pct ?? 0) * 100).toFixed(1)}%</span>
                      <span class="k">平均答题</span><span class="vv">{(s?.avgDecisions ?? 0).toFixed(1)}</span>
                      <span class="k">7d 留存</span><span class="vv">{((s?.retention7d ?? 0) * 100).toFixed(0)}%</span>
                      <span class="k">主路由</span><span class="vv">{s?.mainRoute ? algoMeta(s.mainRoute).label : '—'}</span>
                    </div>
                  </div>
                );
              }}
            </For>
          </div>
        </Show>
      </Show>

      {/* 每用户决策数分布直方图 */}
      <Card variant="elevated" class="amas-panel">
        <div class="amas-panel-head">
          <h3>每用户决策数分布</h3>
          <span class="amas-sub">7d · {(hist()?.totalUsers ?? 0).toLocaleString()} 用户</span>
        </div>
        <Show when={!hist.loading} fallback={<div class="min-h-[220px] flex items-center justify-center"><Spinner /></div>}>
          <Show when={(hist()?.totalUsers ?? 0) > 0} fallback={<Empty title="暂无答题记录" description="learning_records 当前窗口无数据" />}>
            {(() => {
              const buckets = hist()?.buckets ?? [];
              const max = Math.max(1, ...buckets.map((b) => b.count));
              return (
                <div class="us-hist">
                  <For each={buckets}>
                    {(b, i) => (
                      <div class="col">
                        <span class="vv">{b.count}</span>
                        <div class="bar" style={{ height: `${(b.count / max) * 100}%`, background: BAR_COLORS[i() % BAR_COLORS.length] }} />
                        <span class="lab">{b.label}</span>
                      </div>
                    )}
                  </For>
                </div>
              );
            })()}
          </Show>
        </Show>
      </Card>

      {/* 状态流转 + 学习风格聚类 */}
      <div class="grid grid-cols-1 lg:grid-cols-2 gap-4">
        <Card variant="elevated" class="amas-panel">
          <div class="amas-panel-head"><h3>状态切换流向 (24h)</h3><span class="amas-sub">decision_count 阈值穿越</span></div>
          <Show when={!flow.loading} fallback={<div class="min-h-[140px] flex items-center justify-center"><Spinner /></div>}>
            {(() => {
              const tr = (flow()?.transitions ?? []).filter((t) => t.count > 0 || ['new>cold', 'cold>transition', 'transition>stable', 'stable>transition'].includes(`${t.from}>${t.to}`));
              const max = Math.max(1, ...tr.map((t) => t.count));
              return (
                <Show when={tr.length > 0} fallback={<Empty title="24h 内无状态切换" description="learning_records 近 24h 无活动" />}>
                  <div>
                    <For each={tr}>
                      {(t) => {
                        const key = `${t.from}>${t.to}`;
                        const meta = FLOW_META[key] ?? { label: `${t.from} → ${t.to}`, color: 'var(--content-tertiary)' };
                        return (
                          <div class="flow-row">
                            <span>{meta.label}</span>
                            <div class="flow-track"><div class="flow-fill" style={{ width: `${(t.count / max) * 100}%`, background: meta.color }} /></div>
                            <strong style={{ color: meta.color }}>{t.count} user</strong>
                          </div>
                        );
                      }}
                    </For>
                  </div>
                </Show>
              );
            })()}
          </Show>
        </Card>

        <Card variant="elevated" class="amas-panel">
          <div class="amas-panel-head"><h3>学习风格聚类 (k=4)</h3><span class="amas-sub">反应时 + 错误率 + 频次 三维 K-Means</span></div>
          <Show when={!clusters.loading} fallback={<div class="min-h-[140px] flex items-center justify-center"><Spinner /></div>}>
            <Show when={(clusters()?.clusters?.length ?? 0) > 0} fallback={<Empty title="样本不足" description="需 ≥ 4 个有 ≥5 条答题记录的用户才能聚类" />}>
              <div class="flex flex-col gap-2">
                <For each={clusters()?.clusters ?? []}>
                  {(c, i) => {
                    const color = CLUSTER_COLORS[i() % CLUSTER_COLORS.length];
                    return (
                      <div class="cluster-row" style={{ background: `color-mix(in oklab, ${color} 12%, transparent)` }}>
                        <span title={`平均反应时 ${Math.round(c.avgResponseMs)}ms · 错误率 ${(c.errorRate * 100).toFixed(0)}% · ${c.recordsPerActiveDay.toFixed(1)} 题/活跃日`}>
                          {c.label}
                        </span>
                        <strong style={{ color }}>{c.count.toLocaleString()} ({(c.pct * 100).toFixed(0)}%)</strong>
                      </div>
                    );
                  }}
                </For>
              </div>
            </Show>
          </Show>
        </Card>
      </div>
    </div>
  );
}
