import { createSignal, For, Show } from 'solid-js';
import { Tabs } from '@/components/ui/Tabs';
import { adminApi } from '@/api/admin';
import { MetricsDashboard } from './amas/MetricsDashboard';
import { AnomaliesPanel } from './amas/AnomaliesPanel';
import { UserStatePanel } from './amas/UserStatePanel';
import { VersionComparePanel } from './amas/VersionComparePanel';
import { PanelBoundary } from './amas/PanelBoundary';
import './amas/metrics.css';

type TabId = 'metrics' | 'compare' | 'anomalies' | 'user-state';

// 对齐设计稿头部分段（1h/24h/7d/30d）。后端按天聚合，1h/24h 取最近 1 天窗口。
const WINDOWS = [
  { label: '1h', days: 1 },
  { label: '24h', days: 1 },
  { label: '7d', days: 7 },
  { label: '30d', days: 30 },
] as const;

export default function AmasMetricsPage() {
  const [tab, setTab] = createSignal<TabId>('metrics');
  const [winIdx, setWinIdx] = createSignal(2); // 默认 7d
  const days = () => WINDOWS[winIdx()].days;

  const handleTabChange = (id: string) => {
    if (id === 'metrics' || id === 'compare' || id === 'anomalies' || id === 'user-state') {
      setTab(id);
    }
  };

  // 导出当前窗口算法时序为 CSV（对齐设计稿头部"导出"）
  const exportCsv = async () => {
    const rows = await adminApi.amasMetricsTimeseries(days());
    const head = 'date,algorithm,callCount,avgLatencyUs,errorCount';
    const body = rows
      .map((p) => [p.date, p.algorithm, p.callCount, p.avgLatencyUs, p.errorCount].join(','))
      .join('\n');
    const blob = new Blob([`${head}\n${body}`], { type: 'text/csv;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `amas-metrics-${days()}d.csv`;
    a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <div class="amas-metrics space-y-4">
      {/* 紧凑页头：标题 + 副标题 左；分段时间窗 + 导出 右（对齐设计稿，非 HeroCard） */}
      <div class="page-head">
        <div>
          <h1 class="page-title">AMAS 指标</h1>
          <p class="page-desc">
            决策算法 · ELO · MDM · 疲劳信号 · 冷启动 vs 稳定阶段。所有指标来自{' '}
            <code>/admin/amas/metrics</code>。
          </p>
        </div>
        <div class="head-actions">
          <div class="seg" role="group" aria-label="时间窗口">
            <For each={WINDOWS}>
              {(w, i) => (
                <button
                  type="button"
                  class={winIdx() === i() ? 'is-active' : ''}
                  aria-pressed={winIdx() === i()}
                  onClick={() => setWinIdx(i())}
                >
                  {w.label}
                </button>
              )}
            </For>
          </div>
          <button type="button" class="btn-export" onClick={exportCsv}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" /><polyline points="7 10 12 15 17 10" /><line x1="12" y1="15" x2="12" y2="3" /></svg>
            导出
          </button>
        </div>
      </div>

      <Tabs
        tabs={[
          { id: 'metrics', label: '算法延迟 / 错误率' },
          { id: 'compare', label: '版本对比（预测 / 留存）' },
          { id: 'anomalies', label: '异常 / 不变量' },
          { id: 'user-state', label: '用户状态分布' },
        ]}
        active={tab()}
        onChange={handleTabChange}
      />

      <Show when={tab() === 'metrics'}>
        <div class="animate-fade-in"><MetricsDashboard days={days} /></div>
      </Show>
      <Show when={tab() === 'compare'}>
        <div class="animate-fade-in"><PanelBoundary><VersionComparePanel /></PanelBoundary></div>
      </Show>
      <Show when={tab() === 'anomalies'}>
        <div class="animate-fade-in"><PanelBoundary><AnomaliesPanel /></PanelBoundary></div>
      </Show>
      <Show when={tab() === 'user-state'}>
        <div class="animate-fade-in"><PanelBoundary><UserStatePanel /></PanelBoundary></div>
      </Show>
    </div>
  );
}
