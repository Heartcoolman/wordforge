import { For, createMemo } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { ParamField } from './ParamField';
import { TIER_A_META, getByPath, setByPath, type FieldError } from './schema';

interface TierAPanelProps {
  config: Record<string, unknown>;
  errors: FieldError[];
  onChange: (next: Record<string, unknown>) => void;
}

/** 重点参数页：Tier-A 11 维，2026-05-15 验证收益最大的核心调参点。 */
export function TierAPanel(props: TierAPanelProps) {
  const errorMap = createMemo(() => new Map(props.errors.map((e) => [e.path, e.message])));

  function setValue(path: string, v: unknown) {
    const next = structuredClone(props.config);
    setByPath(next, path, v);
    props.onChange(next);
  }

  return (
    <div class="space-y-3">
      <div class="p-3 rounded-lg bg-info-light/30 text-sm text-content-secondary leading-relaxed">
        <strong class="text-content">重点参数（Tier-A 11 维）</strong>：经 2026-05-15 调参验证，调整这 11 个参数即可拿到核心收益（预测 +10.6% / 记忆量 +14%）。
        其它参数对核心指标贡献较小，建议保持默认或在「分节配置」中谨慎调整。
      </div>

      <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-3">
        <For each={TIER_A_META}>
          {(meta) => (
            <Card variant="outlined" padding="sm">
              <ParamField
                meta={meta}
                value={getByPath(props.config, meta.path)}
                error={errorMap().get(meta.path)}
                onChange={(v) => setValue(meta.path, v)}
              />
            </Card>
          )}
        </For>
      </div>
    </div>
  );
}
