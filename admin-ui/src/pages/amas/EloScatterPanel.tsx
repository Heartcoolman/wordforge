import { createResource, For, Show } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { adminApi } from '@/api/admin';

/** 用户 ELO 散点：x=ELO(rating)，y=答题数(games, log 轴)，颜色=7d Δ ELO。
 * 真实数据来自 /api/admin/amas/metrics/elo-scatter（user_elo + user_elo_history）。 */
export function EloScatterPanel() {
  const [d] = createResource(() => adminApi.amasEloScatter(400));

  const dotColor = (delta: number) =>
    delta > 20 ? 'var(--success)' : delta < -20 ? 'var(--error)' : 'var(--accent)';

  return (
    <Card variant="elevated" class="amas-panel">
      <div class="amas-panel-head">
        <h3>用户 ELO 散点</h3>
        <span class="amas-sub">x: ELO · y: 答题数 · 颜色 = 7d Δ ELO</span>
      </div>
      <Show when={!d.loading} fallback={<div class="min-h-[320px] flex items-center justify-center"><Spinner /></div>}>
        <Show when={(d()?.total ?? 0) > 0} fallback={<Empty title="暂无 ELO 数据" description="user_elo 表当前为空（决策产生 ELO 更新后累积）" />}>
          <div class="elo-scatter">
            <div class="axis" />
            <div class="axis-y" />
            <div class="lbl" style="bottom:2px;left:36px">800</div>
            <div class="lbl" style="bottom:2px;left:33%">1200</div>
            <div class="lbl" style="bottom:2px;left:62%">1600</div>
            <div class="lbl" style="bottom:2px;right:0">2000+</div>
            <div class="lbl" style="left:6px;bottom:20%">0</div>
            <div class="lbl" style="left:6px;bottom:50%">5K</div>
            <div class="lbl" style="left:6px;bottom:80%">10K</div>
            <For each={d()?.points ?? []}>
              {(p) => {
                const xr = Math.min(1, Math.max(0, (p.elo - 800) / 1300));
                const yr = Math.min(1, Math.log(p.decisions + 1) / Math.log(11000));
                const c = dotColor(p.deltaElo);
                return (
                  <div
                    class="dot"
                    style={{
                      left: `${8 + xr * 88}%`,
                      bottom: `${8 + yr * 80}%`,
                      background: c,
                      color: c,
                    }}
                  />
                );
              }}
            </For>
          </div>
          <div class="flex gap-4 mt-3 text-xs text-content-secondary">
            <span class="flex items-center gap-1"><span style="width:10px;height:10px;border-radius:50%;background:var(--success)" /> Δ ELO &gt; +20</span>
            <span class="flex items-center gap-1"><span style="width:10px;height:10px;border-radius:50%;background:var(--accent)" /> ±20</span>
            <span class="flex items-center gap-1"><span style="width:10px;height:10px;border-radius:50%;background:var(--error)" /> &lt; -20</span>
            <span class="ml-auto tabular-nums text-content-tertiary">{(d()?.total ?? 0).toLocaleString()} 用户 · 均值 ELO {Math.round(d()?.meanElo ?? 0)}</span>
          </div>
        </Show>
      </Show>
    </Card>
  );
}
