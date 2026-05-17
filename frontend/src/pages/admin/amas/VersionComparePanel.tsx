import { createResource, createSignal, createMemo, For, Show } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { EChart } from '@/components/ui/EChart';
import { Button } from '@/components/ui/Button';
import { Select } from '@/components/ui/Select';
import { Badge } from '@/components/ui/Badge';
import { adminApi, type AmasConfigVersionRow, type AmasVersionSlice } from '@/api/admin';

const METRICS: { key: keyof AmasVersionSlice; label: string; format: (v: number) => string; lowerIsBetter: boolean }[] = [
  { key: 'eventCount', label: '事件数', format: (v) => v.toFixed(0), lowerIsBetter: false },
  { key: 'anomalyRate', label: '异常率', format: (v) => `${(v * 100).toFixed(2)}%`, lowerIsBetter: true },
  { key: 'meanLatencyMs', label: '平均延迟 (ms)', format: (v) => v.toFixed(2), lowerIsBetter: true },
  { key: 'meanReward', label: '平均 Reward', format: (v) => v.toFixed(4), lowerIsBetter: false },
  { key: 'meanAttention', label: '平均注意力', format: (v) => v.toFixed(3), lowerIsBetter: false },
  { key: 'meanFatigue', label: '平均疲劳', format: (v) => v.toFixed(3), lowerIsBetter: true },
  { key: 'meanMotivation', label: '平均动机', format: (v) => v.toFixed(3), lowerIsBetter: false },
  { key: 'meanConfidence', label: '平均信心', format: (v) => v.toFixed(3), lowerIsBetter: false },
];

export function VersionComparePanel() {
  const [versionA, setVersionA] = createSignal<string>('');
  const [versionB, setVersionB] = createSignal<string>('');

  const [versions] = createResource(async () => adminApi.amasListVersions(50));

  // 自动选择最近两个不同 version
  const initialPick = createMemo(() => {
    const list = versions() ?? [];
    return { a: list[1]?.versionHash ?? '', b: list[0]?.versionHash ?? '' };
  });

  // 当版本列表到达且尚未选择，使用 initialPick
  const effectiveA = () => versionA() || initialPick().a;
  const effectiveB = () => versionB() || initialPick().b;

  const [compare] = createResource(
    () => [effectiveA(), effectiveB()] as const,
    async ([a, b]) => {
      if (!a || !b) return null;
      return adminApi.amasCompareVersions(a, b);
    },
  );

  const options = (): { value: string; label: string }[] => {
    return (versions() ?? []).map((v: AmasConfigVersionRow) => ({
      value: v.versionHash,
      label: `${v.versionHash.slice(0, 10)} · ${v.source} · ${formatTime(v.createdAt)}${v.note ? ` · ${v.note}` : ''}`,
    }));
  };

  const barOption = (): import('echarts').EChartsOption => {
    const c = compare();
    if (!c) return {};
    const labels = METRICS.map((m) => m.label);
    const aVals = METRICS.map((m) => Number(c.a[m.key]) || 0);
    const bVals = METRICS.map((m) => Number(c.b[m.key]) || 0);
    return {
      grid: { left: 80, right: 30, top: 50, bottom: 30 },
      legend: { top: 0, data: ['A', 'B'] },
      tooltip: { trigger: 'axis', axisPointer: { type: 'shadow' } },
      xAxis: { type: 'value' },
      yAxis: { type: 'category', data: labels, inverse: true },
      series: [
        { name: 'A', type: 'bar', data: aVals, itemStyle: { color: '#6366f1' }, barGap: '10%' },
        { name: 'B', type: 'bar', data: bVals, itemStyle: { color: '#10b981' }, barGap: '10%' },
      ],
    };
  };

  return (
    <div class="space-y-3">
      <Card variant="elevated">
        <h2 class="text-lg font-semibold text-content mb-1">版本对比</h2>
        <p class="text-xs text-content-tertiary mb-4">
          对比两个 config_version 在其生效期间产生的 monitoring 事件聚合指标。
          ICI / logLoss / expectedMemory 需要离线 benchmark；当前面板基于 monitoring 实测可观察量（reward / 延迟 / 用户状态）。
        </p>

        <Show when={!versions.loading} fallback={<div class="flex justify-center py-12"><Spinner /></div>}>
          <Show when={(versions() ?? []).length >= 2} fallback={<Empty title="至少需要两个版本" description="版本表中条目不足，先在 AMAS 配置页保存几次" />}>
            <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
              <div>
                <label class="text-xs text-content-tertiary mb-1 block">版本 A（基线）</label>
                <Select
                  options={options()}
                  value={effectiveA()}
                  onChange={(e) => setVersionA(e.currentTarget.value)}
                />
              </div>
              <div>
                <label class="text-xs text-content-tertiary mb-1 block">版本 B（对比）</label>
                <Select
                  options={options()}
                  value={effectiveB()}
                  onChange={(e) => setVersionB(e.currentTarget.value)}
                />
              </div>
            </div>

            <div class="flex items-center gap-2 mt-3">
              <Button size="sm" variant="ghost" onClick={() => { setVersionA(initialPick().a); setVersionB(initialPick().b); }}>
                还原为最近两个版本
              </Button>
              <Button size="sm" variant="ghost" onClick={() => { setVersionA(versionB()); setVersionB(versionA()); }}>
                交换 A / B
              </Button>
            </div>
          </Show>
        </Show>
      </Card>

      <Show when={!compare.loading && compare()} fallback={<Show when={compare.loading}><div class="flex justify-center py-12"><Spinner /></div></Show>}>
        {(_) => {
          const c = () => compare()!;
          const noData = () => c().a.eventCount === 0 && c().b.eventCount === 0;
          return (
            <Show when={!noData()} fallback={<Card variant="elevated"><Empty title="两个版本均无 monitoring 事件" description="可能版本尚未上线或落库延迟" /></Card>}>
              <Card variant="elevated">
                <h3 class="text-sm font-semibold text-content mb-3">指标对比柱状图</h3>
                <EChart option={barOption} height="380px" />
              </Card>

              <Card variant="elevated">
                <h3 class="text-sm font-semibold text-content mb-3">差异表</h3>
                <table class="w-full text-sm">
                  <thead>
                    <tr class="bg-surface-secondary border-b border-border">
                      <th scope="col" class="px-3 py-2 text-left font-medium text-content-secondary">指标</th>
                      <th scope="col" class="px-3 py-2 text-right font-medium text-content-secondary">A</th>
                      <th scope="col" class="px-3 py-2 text-right font-medium text-content-secondary">B</th>
                      <th scope="col" class="px-3 py-2 text-right font-medium text-content-secondary">Δ</th>
                    </tr>
                  </thead>
                  <tbody>
                    <For each={METRICS}>
                      {(m) => {
                        const a = Number(c().a[m.key]) || 0;
                        const b = Number(c().b[m.key]) || 0;
                        const delta = b - a;
                        const pct = a !== 0 ? (delta / Math.abs(a)) * 100 : 0;
                        const better = m.lowerIsBetter ? delta < 0 : delta > 0;
                        const worse = m.lowerIsBetter ? delta > 0 : delta < 0;
                        return (
                          <tr class="border-b border-border/40">
                            <td class="px-3 py-2 text-content">{m.label}</td>
                            <td class="px-3 py-2 text-right font-mono">{m.format(a)}</td>
                            <td class="px-3 py-2 text-right font-mono">{m.format(b)}</td>
                            <td class="px-3 py-2 text-right font-mono">
                              <Show when={a !== 0 || b !== 0} fallback={<span class="text-content-tertiary">—</span>}>
                                <Badge variant={better ? 'success' : worse ? 'error' : 'default'} size="sm">
                                  {delta > 0 ? '+' : ''}{m.format(delta)}{a !== 0 && ` (${pct >= 0 ? '+' : ''}${pct.toFixed(1)}%)`}
                                </Badge>
                              </Show>
                            </td>
                          </tr>
                        );
                      }}
                    </For>
                  </tbody>
                </table>
                <p class="text-xs text-content-tertiary mt-3">
                  绿色 = 朝预期方向变化；红色 = 反向变化。延迟、疲劳、异常率以低为佳；其余以高为佳。
                </p>
              </Card>
            </Show>
          );
        }}
      </Show>
    </div>
  );
}

function formatTime(iso: string): string {
  try {
    return new Date(iso).toLocaleString('zh-CN', { hour12: false });
  } catch {
    return iso;
  }
}
