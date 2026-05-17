import { createSignal, Show } from 'solid-js';
import { Tabs } from '@/components/ui/Tabs';
import { MetricsDashboard } from './amas/MetricsDashboard';
import { AnomaliesPanel } from './amas/AnomaliesPanel';
import { UserStatePanel } from './amas/UserStatePanel';
import { VersionComparePanel } from './amas/VersionComparePanel';

type TabId = 'metrics' | 'compare' | 'anomalies' | 'user-state';

export default function AmasMetricsPage() {
  const [tab, setTab] = createSignal<TabId>('metrics');

  return (
    <div class="space-y-4 animate-fade-in-up">
      <Tabs
        tabs={[
          { id: 'metrics', label: '算法延迟 / 错误率' },
          { id: 'compare', label: '版本对比（预测 / 留存）' },
          { id: 'anomalies', label: '异常 / 不变量' },
          { id: 'user-state', label: '用户状态分布' },
        ]}
        active={tab()}
        onChange={(id) => setTab(id as TabId)}
      />

      <Show when={tab() === 'metrics'}>
        <MetricsDashboard />
      </Show>
      <Show when={tab() === 'compare'}>
        <VersionComparePanel />
      </Show>
      <Show when={tab() === 'anomalies'}>
        <AnomaliesPanel />
      </Show>
      <Show when={tab() === 'user-state'}>
        <UserStatePanel />
      </Show>
    </div>
  );
}
