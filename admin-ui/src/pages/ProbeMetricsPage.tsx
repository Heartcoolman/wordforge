import { createSignal, For } from 'solid-js';
import { PanelBoundary } from './amas/PanelBoundary';
import { KpiStrip } from './probe/KpiStrip';
import { ProbeGroupsPanel } from './probe/ProbeGroupsPanel';
import { EventStreamPanel } from './probe/EventStreamPanel';
import { SamplingRulesPanel } from './probe/SamplingRulesPanel';
import { SinksPanel } from './probe/SinksPanel';
import { SchemaPreview } from './probe/SchemaPreview';
import { AuditTrail } from './probe/AuditTrail';
import { TechTip } from '@/components/ui/TechTip';
import './probe/metrics.css';

// 时间窗：KPI / 探针聚合按 days 联动，后端 overview/probes 接 ?days 真实生效。
// 后端按天聚合（最小窗口 1 天），不提供亚天级窗口，故不放 1h 误导选项。
const WINDOWS = [
  { label: '24h', days: 1 },
  { label: '7d', days: 7 },
  { label: '30d', days: 30 },
] as const;

export default function ProbeMetricsPage() {
  const [winIdx, setWinIdx] = createSignal(0); // 默认 24h
  const days = () => WINDOWS[winIdx()].days;

  return (
    <div class="probe-metrics space-y-4">
      {/* 紧凑页头（对齐设计稿 .page-header） */}
      <div class="page-head">
        <div>
          <h1 class="page-title">数据探针</h1>
          <p class="page-desc">
            给运维 / 客服看的数据健康大盘：四类数据源（点击、学习、答题、错误）实时汇总，一眼看出系统活不活跃、数据质量好不好、采集有没有出错。
            <TechTip label="技术说明" placement="bottom" width="330px">
              <b>技术说明</b>
              <span class="tip-mono">
                指标来自 /admin/probe-telemetry；4 个派生探针对真实源 telemetry_summaries / learning_sessions / learning_records 聚合，采样改动写入审计。
              </span>
            </TechTip>
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
        </div>
      </div>

      {/* KPI strip */}
      <PanelBoundary><KpiStrip days={days} /></PanelBoundary>

      {/* 主区：左 探针组 / 右 实时流 */}
      <div class="pb-layout">
        <div>
          <PanelBoundary><ProbeGroupsPanel days={days} /></PanelBoundary>
        </div>
        <PanelBoundary><EventStreamPanel /></PanelBoundary>
      </div>

      {/* 采样策略：规则 + sinks + schema */}
      <div class="pb-section-title">
        <h2>采样和写入管理</h2>
        <span class="hint">控制每类数据按多大比例留存、最终存到哪里，以及一条数据长什么样</span>
      </div>
      <div class="pb-strategy-grid">
        <PanelBoundary><SamplingRulesPanel /></PanelBoundary>
        <PanelBoundary><SinksPanel /></PanelBoundary>
        <PanelBoundary><SchemaPreview /></PanelBoundary>
      </div>

      {/* 审计 */}
      <div class="pb-section-title">
        <h2>最近改动</h2>
        <span class="hint">谁改过采样设置的历史记录，便于回溯排查</span>
      </div>
      <div class="card-surface" style={{ background: 'var(--surface-elevated)', 'border-radius': 'var(--radius-lg)', 'box-shadow': 'var(--elevation-1)' }}>
        <PanelBoundary><AuditTrail /></PanelBoundary>
      </div>
    </div>
  );
}
