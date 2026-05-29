import { Card } from '@/components/ui/Card';
import { StatCard } from '@/components/ui/StatCard';
import type { AdvisorCostStats } from '@/api/admin';

function yuan(v: number, d = 2): string {
  return `¥${v.toFixed(d)}`;
}

export function CostRow(props: { stats: AdvisorCostStats }) {
  const s = () => props.stats;
  const decided = () => s().acceptedCount + s().rejectedCount;
  return (
    <div class="grid grid-cols-2 lg:grid-cols-4 gap-3">
      {/* 首卡：本月成本 + 配额条 + 预测（自定义，StatCard 无配额条槽） */}
      <Card variant="elevated">
        <div class="flex flex-col gap-2">
          <span class="text-sm text-content-secondary">本月调用成本</span>
          <span class="text-2xl font-semibold tabular-nums text-accent">
            {yuan(s().monthYuan)}<span class="text-sm text-content-tertiary"> / {yuan(s().monthCapYuan)}</span>
          </span>
          <div class="h-1.5 rounded-full bg-surface-secondary overflow-hidden">
            <div
              data-testid="quota-bar-fill"
              class="h-full bg-accent transition-[width]"
              style={{ width: `${Math.min(s().quotaPct, 100)}%` }}
            />
          </div>
          <span class="text-[11.5px] text-content-tertiary tabular-nums">
            {s().quotaPct.toFixed(1)}% · 预计月底 {yuan(s().forecastYuan)}
          </span>
        </div>
      </Card>

      <StatCard
        title="7 天平均单次成本"
        value={yuan(s().avg7dCostYuan)}
        color="info"
        icon="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z"
      />
      <StatCard
        title="本月调用次数"
        value={`${s().monthCalls}`}
        color="accent"
        icon="M13 10V3L4 14h7v7l9-11h-7z"
      />
      <StatCard
        title="累计 patch · 接受率"
        value={`${s().acceptedCount}/${decided()}`}
        color="success"
        icon="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"
        trend={{ value: Math.round(s().acceptanceRate * 100), label: '接受率', showZero: true }}
      />
    </div>
  );
}
