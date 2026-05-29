import { For, Show, createMemo } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Empty } from '@/components/ui/Empty';
import type { AdvisorCostDaily } from '@/api/admin';

const W = 300;
const H = 140;
const PAD = 4;

export function CostChart(props: {
  data: AdvisorCostDaily[];
  avg7dYuan: number;
  capYuan: number;
  refLineYuan: number;
}) {
  const max = createMemo(() => Math.max(props.refLineYuan, ...props.data.map((d) => d.costYuan), 0.01));
  const barW = createMemo(() => (W - PAD * 2) / Math.max(props.data.length, 1));
  const refY = createMemo(() => H - PAD - (props.refLineYuan / max()) * (H - PAD * 2));
  return (
    <Card variant="elevated">
      <h4 class="text-sm font-medium text-content-secondary mb-2">调用成本 · 30 天</h4>
      <Show when={props.data.length > 0} fallback={<Empty title="暂无成本数据" description="" />}>
        <svg viewBox={`0 0 ${W} ${H}`} class="w-full" role="img" aria-label="30 天调用成本柱状图">
          <line x1={PAD} x2={W - PAD} y1={refY()} y2={refY()}
            stroke="var(--border-hairline)" stroke-dasharray="3 3" />
          <For each={props.data}>
            {(d, i) => {
              const h = () => (d.costYuan / max()) * (H - PAD * 2);
              return (
                <rect
                  data-bar
                  x={PAD + i() * barW() + 0.5}
                  y={H - PAD - h()}
                  width={Math.max(barW() - 1, 0.5)}
                  height={h()}
                  fill="var(--accent)"
                  rx="0.5"
                />
              );
            }}
          </For>
        </svg>
        <div class="flex justify-between text-[11px] text-content-tertiary tabular-nums mt-1">
          <span>7 天平均 ¥{props.avg7dYuan.toFixed(2)}</span>
          <span>月度上限 ¥{props.capYuan.toFixed(0)}</span>
        </div>
      </Show>
    </Card>
  );
}
