import { createSignal, Show } from 'solid-js';
import { Tabs } from '@/components/ui/Tabs';
import { HeroCard } from '@/components/ui/HeroCard';
import { MetricsDashboard } from './amas/MetricsDashboard';
import { AnomaliesPanel } from './amas/AnomaliesPanel';
import { UserStatePanel } from './amas/UserStatePanel';
import { VersionComparePanel } from './amas/VersionComparePanel';

type TabId = 'metrics' | 'compare' | 'anomalies' | 'user-state';

export default function AmasMetricsPage() {
  const [tab, setTab] = createSignal<TabId>('metrics');

  // 强转保护：onChange 透传的 id 是 string，走 switch 校验后再写回
  const handleTabChange = (id: string) => {
    if (id === 'metrics' || id === 'compare' || id === 'anomalies' || id === 'user-state') {
      setTab(id);
    }
  };

  return (
    <div class="space-y-4">
      <HeroCard
        eyebrow="8 算法 × 决策"
        eyebrowVariant="accent"
        title="AMAS 指标"
        desc="算法延迟 / 错误率、版本对比（预测 / 留存）、异常 / 不变量、用户状态分布。监控 ensemble / heuristic / IGE / SWD / MDM / IAD / MTP / SSP 8 个决策算法。"
      />
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
        <div class="animate-fade-in"><MetricsDashboard /></div>
      </Show>
      <Show when={tab() === 'compare'}>
        <div class="animate-fade-in"><VersionComparePanel /></div>
      </Show>
      <Show when={tab() === 'anomalies'}>
        <div class="animate-fade-in"><AnomaliesPanel /></div>
      </Show>
      <Show when={tab() === 'user-state'}>
        <div class="animate-fade-in"><UserStatePanel /></div>
      </Show>
    </div>
  );
}
