import type { AdvisorCostStats } from '@/api/admin';

function yuan(v: number, d = 2): string {
  return `¥${v.toFixed(d)}`;
}

/** 成本行：4 张 .cost-card，对齐设计图密集布局 + 紫色 quota-bar。 */
export function CostRow(props: { stats: AdvisorCostStats }) {
  const s = () => props.stats;
  const decided = () => s().acceptedCount + s().rejectedCount;
  return (
    <div class="cost-row">
      {/* 首卡：本月成本 + 配额条 + 预测 */}
      <div class="cost-card is-llm">
        <div class="label">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <circle cx="12" cy="12" r="10" /><line x1="12" y1="8" x2="12" y2="12" /><line x1="12" y1="16" x2="12.01" y2="16" />
          </svg>
          本月调用成本
        </div>
        <div class="value">{yuan(s().monthYuan)}<span class="unit">/ {yuan(s().monthCapYuan)}</span></div>
        <div class="quota-bar">
          <span data-testid="quota-bar-fill" style={{ width: `${Math.min(s().quotaPct, 100)}%` }} />
        </div>
        <div class="sub">{s().quotaPct.toFixed(1)}% · 预计月底 {yuan(s().forecastYuan)}</div>
      </div>

      {/* 7 天平均单次成本 */}
      <div class="cost-card">
        <div class="label">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <circle cx="12" cy="12" r="10" /><polyline points="12 6 12 12 16 14" />
          </svg>
          7 天平均单次成本
        </div>
        <div class="value">{yuan(s().avg7dCostYuan)}</div>
        <div class="sub">DeepSeek · 单次调用均价</div>
      </div>

      {/* 本月调用次数 */}
      <div class="cost-card">
        <div class="label">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M18 6V4a2 2 0 0 0-2-2H8a2 2 0 0 0-2 2v2" /><rect x="3" y="6" width="18" height="14" rx="2" />
          </svg>
          本月调用次数
        </div>
        <div class="value">{s().monthCalls}<span class="unit">次</span></div>
        <div class="sub">含产出 patch 与无建议巡查</div>
      </div>

      {/* 累计 patch · 接受率 */}
      <div class="cost-card">
        <div class="label">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <polyline points="20 6 9 17 4 12" />
          </svg>
          累计 patch · 接受率
        </div>
        <div class="value">
          {s().acceptedCount}/{decided()}<span class="unit">· {(s().acceptanceRate * 100).toFixed(1)}%</span>
        </div>
        <div class="sub">{s().rejectedCount} 拒绝 · 累计决策 {decided()} 条</div>
      </div>
    </div>
  );
}
