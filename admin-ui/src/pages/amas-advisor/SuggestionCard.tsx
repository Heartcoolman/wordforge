import { createMemo, createSignal, For, Show } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { Badge } from '@/components/ui/Badge';
import type { AmasSuggestion, AmasSuggestionStatus, WhitelistRow } from '@/api/admin';

const STATUS_LABEL: Record<AmasSuggestionStatus, string> = {
  pending: '待审批', approved: '已批准', rejected: '已拒绝',
  superseded: '已被覆盖', expired: '已过期', auto_applied: '自动应用',
};
const STATUS_VARIANT: Record<AmasSuggestionStatus, 'default' | 'success' | 'error' | 'warning' | 'info'> = {
  pending: 'warning', approved: 'success', rejected: 'error',
  superseded: 'default', expired: 'default', auto_applied: 'info',
};

/** evidence_json 三联影响字段（缺失显 "—"，不编造）。 */
const IMPACT_FIELDS: Array<{ key: string; label: string; goodWhenNegative?: boolean }> = [
  { key: 'fatigueDelta', label: '疲劳率', goodWhenNegative: true },
  { key: 'accuracyDelta', label: '正确率' },
  { key: 'retentionDelta', label: '留存' },
];

function fmtTime(iso: string): string {
  try { return new Date(iso).toLocaleString('zh-CN', { hour12: false }); } catch { return iso; }
}

function fmtPatchValue(value: unknown): string {
  if (typeof value !== 'number' || !Number.isFinite(value)) return String(value);
  if (value === 0) return '0';
  if (Math.abs(value) < 1e-4) return value.toExponential(2);
  return value.toPrecision(6).replace(/\.?0+$/, '');
}

type Risk = { kind: 'ok' | 'outside' | 'breach'; label: string };

function riskFor(path: string, value: unknown, whitelist: WhitelistRow[]): Risk {
  const row = whitelist.find((w) => w.path === path);
  if (!row) return { kind: 'outside', label: '白名单外' };
  if (typeof value === 'number' && Number.isFinite(value) && (value < row.minSafe || value > row.maxSafe)) {
    return { kind: 'breach', label: `越界 [${row.minSafe}, ${row.maxSafe}]` };
  }
  return { kind: 'ok', label: '白名单内' };
}

export function SuggestionCard(props: {
  s: AmasSuggestion;
  whitelist: WhitelistRow[];
  busy: boolean;
  onApprove: () => void;
  onReject: () => void;
  onCanary: () => void;
}) {
  const [showEvidence, setShowEvidence] = createSignal(false);

  // 整卡风险等级：任一 breach→error、任一 outside→warning、否则 ok
  const cardRisk = createMemo<Risk['kind']>(() => {
    const risks = Object.entries(props.s.patchJson).map(([p, v]) => riskFor(p, v, props.whitelist).kind);
    if (risks.includes('breach')) return 'breach';
    if (risks.includes('outside')) return 'outside';
    return 'ok';
  });

  function impactValue(key: string): { text: string; cls: string } {
    const raw = (props.s.evidenceJson as Record<string, unknown>)[key];
    if (typeof raw !== 'number' || !Number.isFinite(raw)) return { text: '—', cls: 'text-content-tertiary' };
    const pct = raw * 100;
    const field = IMPACT_FIELDS.find((f) => f.key === key)!;
    const good = field.goodWhenNegative ? raw <= 0 : raw >= 0;
    return {
      text: `${raw >= 0 ? '+' : ''}${pct.toFixed(1)}%`,
      cls: good ? 'text-success' : 'text-error',
    };
  }

  return (
    <Card variant="elevated">
      <div class="flex items-start justify-between gap-3 mb-2">
        <div class="flex items-center gap-2 flex-wrap">
          <Badge variant={STATUS_VARIANT[props.s.status]} size="sm">{STATUS_LABEL[props.s.status]}</Badge>
          <span class="text-xs font-mono text-content-tertiary">基于 {props.s.basedOnVersionHash.slice(0, 10)}</span>
          <span class="text-xs text-content-tertiary">{fmtTime(props.s.createdAt)}</span>
          <Show when={props.s.confidence != null}>
            <Badge variant="info" size="sm">置信 {(props.s.confidence! * 100).toFixed(0)}%</Badge>
          </Show>
          <Show when={cardRisk() === 'breach'}>
            <Badge variant="error" size="sm" dot>含越界参数</Badge>
          </Show>
          <Show when={cardRisk() === 'outside'}>
            <Badge variant="warning" size="sm" dot>含白名单外参数</Badge>
          </Show>
        </div>
        <div class="flex gap-2 shrink-0">
          <Button size="sm" variant="outline" loading={props.busy} onClick={props.onReject}>拒绝</Button>
          <Button size="sm" variant="secondary" loading={props.busy} onClick={props.onCanary}>进灰度 20%</Button>
          <Button size="sm" loading={props.busy} onClick={props.onApprove}>批准并应用</Button>
        </div>
      </div>

      <div class="text-sm text-content leading-relaxed mb-3">{props.s.rationale}</div>

      {/* 三联预估影响 */}
      <div class="grid grid-cols-3 gap-2 mb-3">
        <For each={IMPACT_FIELDS}>
          {(f) => {
            const v = impactValue(f.key);
            return (
              <div class="rounded-lg bg-surface-secondary px-3 py-2">
                <p class="text-[11px] text-content-tertiary">{f.label}</p>
                <p class={`text-sm font-medium tabular-nums ${v.cls}`}>{v.text}</p>
              </div>
            );
          }}
        </For>
      </div>

      {/* patch diff + 每行白名单内外 / 越界标记 */}
      <div class="space-y-1.5">
        <h4 class="text-xs font-medium text-content-secondary">
          Patch diff（{Object.keys(props.s.patchJson).length} 项 · 基于 {props.s.basedOnVersionHash.slice(0, 8)}）
        </h4>
        <table class="w-full text-xs font-mono">
          <thead>
            <tr class="text-content-tertiary border-b border-border-hairline">
              <th class="text-left py-1 pr-2">字段</th>
              <th class="text-right py-1 pr-2 w-20">旧值</th>
              <th class="text-center py-1 w-6"></th>
              <th class="text-right py-1 pl-2 w-20">建议值</th>
              <th class="text-left py-1 pl-3">风险</th>
            </tr>
          </thead>
          <tbody>
            <For each={Object.entries(props.s.patchJson)}>
              {([path, value]) => {
                const old = props.s.baseValuesJson?.[path];
                const oldNum = typeof old === 'number' && Number.isFinite(old) ? old : null;
                const risk = riskFor(path, value, props.whitelist);
                const riskCls = risk.kind === 'breach'
                  ? 'text-error' : risk.kind === 'outside' ? 'text-warning' : 'text-success';
                return (
                  <tr class="border-b border-border-hairline">
                    <td class="py-1 pr-2 text-content">{path}</td>
                    <td class="py-1 pr-2 text-right text-content-tertiary tabular-nums">
                      {oldNum != null ? fmtPatchValue(oldNum) : '—'}
                    </td>
                    <td class="py-1 text-center text-content-tertiary">→</td>
                    <td class="py-1 pl-2 text-right text-success tabular-nums">{fmtPatchValue(value)}</td>
                    <td class={`py-1 pl-3 ${riskCls}`}>{risk.label}</td>
                  </tr>
                );
              }}
            </For>
          </tbody>
        </table>
      </div>

      <div class="mt-2">
        <Button size="xs" variant="ghost" onClick={() => setShowEvidence(!showEvidence())}>
          {showEvidence() ? '隐藏' : '查看'} evidence
        </Button>
        <Show when={showEvidence()}>
          <pre class="mt-2 p-2 bg-surface-secondary rounded text-[10px] overflow-auto font-mono max-h-64">
            {JSON.stringify(props.s.evidenceJson, null, 2)}
          </pre>
        </Show>
      </div>
    </Card>
  );
}
