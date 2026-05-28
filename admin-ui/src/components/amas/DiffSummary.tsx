import { createMemo, For, Show } from 'solid-js';
import { Badge } from '@/components/ui/Badge';
import { Card } from '@/components/ui/Card';
import { diffKnown, PARAM_INDEX, type FieldError } from '@/pages/amas/schema';
import { formatVal } from '@/pages/amas/PresetSelector';

interface DiffSummaryProps {
  baseline: Record<string, unknown>;
  config: Record<string, unknown>;
  errors: FieldError[];
}

/** sensitivity → 影响文案 */
const AFFECT_HINT: Record<string, string> = {
  ultra: '核心指标变动 ±5pt+',
  high: '核心指标变动 ±1-3pt',
  med: '次级指标变动',
  low: '可忽略',
};

/**
 * 修改摘要面板(对齐 amas-config.html .panel "修改摘要 (vs HEAD)" 设计图):
 * - 表头: 参数 / 原值 / 新值 / 影响
 * - 每行: path mono + before chip-error + after chip-success + sensitivity 文案
 * - 顶部: "{N} 处改动" + validate 状态 chip
 *
 * 数据来自 diffKnown(baseline, config),仅对 PARAM_DICT 已知字段做 diff。
 */
export function DiffSummary(props: DiffSummaryProps) {
  const diff = createMemo(() => diffKnown(props.baseline, props.config));
  const validateOk = createMemo(() => props.errors.length === 0);

  return (
    <Card variant="outlined" padding="none">
      <div class="flex items-baseline justify-between px-4 py-3 border-b border-border-hairline">
        <h3 class="text-sm font-semibold text-content">修改摘要 (vs baseline)</h3>
        <div class="flex items-center gap-2">
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
              const affectText = meta?.sensitivity ? AFFECT_HINT[meta.sensitivity] ?? '未估算' : '未估算';
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
                  <span class="text-content-secondary truncate">{affectText}</span>
                </div>
              );
            }}
          </For>
        </div>
      </Show>
    </Card>
  );
}
