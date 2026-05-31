import { createResource, For, Show } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { adminApi } from '@/api/admin';
import { algoMeta } from './algoMeta';

const C = 2 * Math.PI * 14; // 甜甜圈周长 ≈ 87.965

function fmtShort(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1).replace(/\.0$/, '')}M`;
  if (n >= 1000) return `${(n / 1000).toFixed(1).replace(/\.0$/, '')}K`;
  return String(n);
}

/** 算法决策分布甜甜圈 —— 移植设计稿 SVG donut + 双列 legend，真实数据来自
 * /api/admin/amas/metrics/algorithm-distribution，按 routing_algo 聚合 N 天决策占比。
 * routing_algo 取值即 AlgorithmId 6 个路由算法(heuristic/ige/swd/ensemble/mdm/mastery);
 * IAD/MTP/SSP 是记忆模型不参与路由,不会出现在此分布。 */
export function AlgorithmDonut(props: { days: () => number }) {
  const [dist] = createResource(props.days, (d) => adminApi.amasAlgorithmDistribution(d));

  const segments = () => {
    let acc = 0;
    return (dist() ?? []).map((s) => {
      const len = s.pct * C;
      const seg = { color: algoMeta(s.algorithm).color, len, offset: -acc };
      acc += len;
      return seg;
    });
  };
  const total = () => (dist() ?? []).reduce((a, s) => a + s.count, 0);
  const topName = () => {
    const top = (dist() ?? [])[0];
    return top ? algoMeta(top.algorithm).label : '—';
  };

  return (
    <Card variant="elevated" class="amas-panel">
      <div class="amas-panel-head">
        <h3>算法决策分布 · 6 路由算法</h3>
        <span class="amas-sub">{fmtShort(total())} 次决策 · {topName()} 主导</span>
      </div>
      <Show
        when={!dist.loading}
        fallback={<div class="min-h-[260px] flex items-center justify-center"><Spinner /></div>}
      >
        <Show
          when={(dist() ?? []).length > 0}
          fallback={
            <Empty
              title="暂无决策路由数据"
              description="engine_monitoring_events.routing_algo 当前为空（决策埋点生效后随流量累积）"
            />
          }
        >
          <div style="display:grid;grid-template-columns:220px 1fr;gap:30px;align-items:center" class="max-md:!grid-cols-1">
            <div class="donut">
              <svg viewBox="0 0 36 36">
                <circle cx="18" cy="18" r="14" fill="none" stroke="var(--surface-tertiary)" stroke-width="6" />
                <For each={segments()}>
                  {(seg) => (
                    <circle
                      cx="18" cy="18" r="14" fill="none"
                      stroke={seg.color} stroke-width="6"
                      stroke-dasharray={`${seg.len.toFixed(2)} ${C.toFixed(2)}`}
                      stroke-dashoffset={seg.offset.toFixed(2)}
                    />
                  )}
                </For>
              </svg>
              <div class="center">
                <div class="v">{fmtShort(total())}</div>
                <div class="l">{props.days()}d decisions</div>
              </div>
            </div>
            <div class="legend">
              <For each={dist() ?? []}>
                {(s) => {
                  const m = algoMeta(s.algorithm);
                  return (
                    <div class="row-l">
                      <span class="sw" style={{ background: m.color }} />
                      <span class="nm">{m.label}</span>
                      <span class="v">{(s.pct * 100).toFixed(0)}%</span>
                    </div>
                  );
                }}
              </For>
            </div>
          </div>
        </Show>
      </Show>
    </Card>
  );
}
