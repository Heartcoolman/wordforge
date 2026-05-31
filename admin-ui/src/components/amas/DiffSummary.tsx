import { createMemo, createResource, For, Show } from 'solid-js';
import { Badge } from '@/components/ui/Badge';
import { Card } from '@/components/ui/Card';
import { diffKnown, PARAM_INDEX, type FieldError } from '@/pages/amas/schema';
import { formatVal } from '@/pages/amas/PresetSelector';
import { adminApi, type DiffImpactEntry, type DiffImpactResponse } from '@/api/admin';

interface DiffSummaryProps {
  baseline: Record<string, unknown>;
  config: Record<string, unknown>;
  errors: FieldError[];
}

/** sensitivity → 静态影响文案(diff-impact 无估算时兜底) */
const AFFECT_HINT: Record<string, string> = {
  ultra: '核心指标变动 ±5pt+',
  high: '核心指标变动 ±1-3pt',
  med: '次级指标变动',
  low: '可忽略',
};

const METRIC_LABEL: Record<string, string> = {
  accuracy: '准确率',
  fatigue: '疲劳率',
  d7Retention: 'D7 留存',
};

/** 单条 metric 估算 → "准确率 +0.4~1.2pt" 文案 */
function fmtImpact(e: DiffImpactEntry): string {
  const sign = e.deltaHighPt < 0 ? '−' : '+';
  const lo = Math.abs(e.deltaLowPt).toFixed(1);
  const hi = Math.abs(e.deltaHighPt).toFixed(1);
  const range = lo === hi ? hi : `${lo}~${hi}`;
  return `${METRIC_LABEL[e.metric] ?? e.metric} ${sign}${range}pt`;
}

/**
 * 修改摘要面板(对齐 amas-config.html .panel "修改摘要 (vs HEAD)" 设计图):
 * - 表头: 参数 / 原值 / 新值 / 影响
 * - 每行: path mono + before chip-error + after chip-success + 真实估算影响
 * - 顶部: "{N} 处改动" + validate 状态 chip + 估算样本量
 *
 * diff 集合来自 diffKnown(baseline, config)(仅 PARAM_DICT 已知字段);
 * 影响列优先取 diff-impact 端点的真实遥测估算,按 path 匹配,缺失时回退 sensitivity 文案。
 */
export function DiffSummary(props: DiffSummaryProps) {
  const diff = createMemo(() => diffKnown(props.baseline, props.config));
  const validateOk = createMemo(() => props.errors.length === 0);

  // 把 diff 折叠成 flat patch { path: after },作为 diff-impact 入参;
  // 用 JSON 串作 resource source 保证只在改动真正变化时重新拉取。
  const patchKey = createMemo(() => {
    const patch: Record<string, unknown> = {};
    for (const d of diff()) patch[d.path] = d.after;
    return JSON.stringify(patch);
  });

  const [impact] = createResource<DiffImpactResponse | null, string>(
    () => (diff().length > 0 ? patchKey() : null),
    async (key) => {
      const patch = JSON.parse(key) as Record<string, unknown>;
      if (Object.keys(patch).length === 0) return null;
      return adminApi.amasConfigDiffImpact(patch);
    },
  );

  // path → 该字段的 metric 估算列表
  const impactByPath = createMemo(() => {
    const m = new Map<string, DiffImpactEntry[]>();
    const r = impact();
    if (r) for (const f of r.fields) m.set(f.path, f.impacts);
    return m;
  });

  return (
    <Card variant="outlined" padding="none">
      <div class="flex items-baseline justify-between px-4 py-3 border-b border-border-hairline">
        <h3 class="text-sm font-semibold text-content">修改摘要 (vs baseline)</h3>
        <div class="flex items-center gap-2">
          <Show when={impact()}>
            {(r) => (
              <span class="text-[10.5px] font-mono text-content-tertiary tabular-nums">
                估算样本 {r().telemetrySampleSize.toLocaleString('zh-CN')} · 置信 {r().confidence}
              </span>
            )}
          </Show>
          <span class="text-[11px] font-mono text-content-tertiary tabular-nums">
            {diff().length} 处改动
          </span>
          <Show when={diff().length > 0}>
            <Badge variant={validateOk() ? 'success' : 'error'} size="sm">
              {validateOk() ? '验证通过' : `${props.errors.length} 错`}
            </Badge>
          </Show>
        </div>
      </div>

      <Show
        when={diff().length > 0}
        fallback={
          <div class="px-4 py-6 text-center text-xs text-content-tertiary">
            当前配置与 baseline 完全一致,无修改
          </div>
        }
      >
        <div class="grid grid-cols-[2fr_1fr_1fr_1.4fr] items-center px-4 py-2 bg-surface-secondary border-b border-border-hairline text-[10.5px] uppercase tracking-wide text-content-tertiary font-medium">
          <span>参数</span>
          <span>原值</span>
          <span>新值</span>
          <span>影响</span>
        </div>
        <div class="max-h-[280px] overflow-y-auto">
          <For each={diff()}>
            {(entry) => {
              const meta = PARAM_INDEX.get(entry.path);
              const fallback = meta?.sensitivity ? AFFECT_HINT[meta.sensitivity] ?? '未估算' : '未估算';
              const impacts = () => impactByPath().get(entry.path) ?? [];
              return (
                <div class="grid grid-cols-[2fr_1fr_1fr_1.4fr] items-center px-4 py-2.5 border-b border-border-hairline/50 text-[12px] last:border-b-0 hover:bg-surface-secondary/40 transition-colors">
                  <div class="min-w-0 pr-2">
                    <div class="text-content truncate">{entry.label_zh}</div>
                    <code class="text-[10.5px] font-mono text-content-tertiary truncate block">
                      {entry.path}
                    </code>
                  </div>
                  <span class="inline-flex">
                    <Badge variant="error" size="sm">
                      <span class="font-mono tabular-nums">{formatVal(entry.before)}</span>
                    </Badge>
                  </span>
                  <span class="inline-flex">
                    <Badge variant="success" size="sm">
                      <span class="font-mono tabular-nums">{formatVal(entry.after)}</span>
                    </Badge>
                  </span>
                  <span class="text-content-secondary min-w-0">
                    <Show
                      when={!impact.loading}
                      fallback={<span class="text-content-tertiary">估算中…</span>}
                    >
                      <Show
                        when={impacts().length > 0}
                        fallback={<span class="truncate block">{fallback}</span>}
                      >
                        <span class="flex flex-col gap-0.5">
                          <For each={impacts()}>
                            {(e) => (
                              <span
                                class={
                                  e.deltaHighPt < 0 || e.direction === 'down'
                                    ? 'text-error-strong tabular-nums'
                                    : 'text-success-strong tabular-nums'
                                }
                              >
                                {fmtImpact(e)}
                              </span>
                            )}
                          </For>
                        </span>
                      </Show>
                    </Show>
                  </span>
                </div>
              );
            }}
          </For>
        </div>
      </Show>
    </Card>
  );
}
