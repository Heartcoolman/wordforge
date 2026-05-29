import { createMemo, createSignal, For, Show } from 'solid-js';
import { Badge } from '@/components/ui/Badge';
import { Button } from '@/components/ui/Button';
import type { AmasSuggestion, AmasSuggestionStatus, WhitelistRow } from '@/api/admin';

const STATUS_LABEL: Record<AmasSuggestionStatus, string> = {
  pending: '待审批', approved: '已批准', rejected: '已拒绝',
  superseded: '已被覆盖', expired: '已过期', auto_applied: '自动应用',
};
const STATUS_VARIANT: Record<AmasSuggestionStatus, 'default' | 'success' | 'error' | 'warning' | 'info'> = {
  pending: 'warning', approved: 'success', rejected: 'error',
  superseded: 'default', expired: 'default', auto_applied: 'info',
};

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
  const entries = () => Object.entries(props.s.patchJson);
  const title = createMemo(() => {
    const ks = Object.keys(props.s.patchJson);
    if (ks.length === 0) return 'AMAS 参数 patch';
    return ks.length === 1 ? `调整 ${ks[0]}` : `调整 ${ks[0]} 等 ${ks.length} 项`;
  });
  const removedCount = createMemo(() => entries().filter(([p]) => props.s.baseValuesJson?.[p] != null).length);

  const cardRisk = createMemo<Risk['kind']>(() => {
    const risks = entries().map(([p, v]) => riskFor(p, v, props.whitelist).kind);
    if (risks.includes('breach')) return 'breach';
    if (risks.includes('outside')) return 'outside';
    return 'ok';
  });

  function impactValue(key: string): { text: string; cls: string } {
    const raw = (props.s.evidenceJson as Record<string, unknown>)[key];
    if (typeof raw !== 'number' || !Number.isFinite(raw)) return { text: '—', cls: '' };
    const field = IMPACT_FIELDS.find((f) => f.key === key)!;
    const good = field.goodWhenNegative ? raw <= 0 : raw >= 0;
    return { text: `${raw >= 0 ? '+' : ''}${(raw * 100).toFixed(1)}%`, cls: good ? 'is-up' : 'is-down' };
  }

  return (
    <div class="patch-card">
      <div class="patch-head">
        <div class="avatar">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 2v4M12 18v4M4.9 4.9l2.8 2.8M16.3 16.3l2.8 2.8M2 12h4M18 12h4M4.9 19.1l2.8-2.8M16.3 7.7l2.8-2.8" /></svg>
        </div>
        <div class="info">
          <div class="title">{title()}</div>
          <div class="meta">
            <span class="font-mono">#{props.s.basedOnVersionHash.slice(0, 8)}</span>
            <span class="dot" />
            <span>{fmtTime(props.s.createdAt)}</span>
            <span class="dot" />
            <span class="stat-line"><span class="pos">+{entries().length}</span> <span class="neg">-{removedCount()}</span></span>
            <Show when={props.s.confidence != null}>
              <span class="dot" /><span>置信 {(props.s.confidence! * 100).toFixed(0)}%</span>
            </Show>
            <Show when={cardRisk() === 'breach'}><Badge variant="error" size="sm" dot>含越界参数</Badge></Show>
            <Show when={cardRisk() === 'outside'}><Badge variant="warning" size="sm" dot>含白名单外参数</Badge></Show>
            <Show when={cardRisk() === 'ok'}><Badge variant="success" size="sm" dot>白名单内 · 自动灰度可用</Badge></Show>
          </div>
        </div>
        <Badge variant={STATUS_VARIANT[props.s.status]} size="sm">{STATUS_LABEL[props.s.status]}</Badge>
      </div>

      <div class="patch-rationale">{props.s.rationale}</div>

      {/* 三联预估影响 */}
      <div class="patch-impact">
        <For each={IMPACT_FIELDS}>
          {(f) => {
            const v = impactValue(f.key);
            return (<div><div class="l">{f.label}</div><div class={`v ${v.cls}`}>{v.text}</div></div>);
          }}
        </For>
      </div>

      {/* unified diff */}
      <div class="diff">
        <div class="hunk">@@ amas_config · {Object.keys(props.s.patchJson).length} 项变更 @@</div>
        <For each={entries()}>
          {([path, value]) => {
            const old = props.s.baseValuesJson?.[path];
            const oldNum = typeof old === 'number' && Number.isFinite(old) ? old : null;
            return (
              <>
                <Show when={oldNum != null}>
                  <div class="ln rem"><span class="num">-</span><span class="num" /><span class="code">{path} = {fmtPatchValue(oldNum)}</span></div>
                </Show>
                <div class="ln add"><span class="num" /><span class="num">+</span><span class="code">{path} = {fmtPatchValue(value)}</span></div>
              </>
            );
          }}
        </For>
      </div>

      <div class="patch-foot">
        <Button size="sm" variant="outline" loading={props.busy} onClick={props.onReject}>拒绝</Button>
        <Button size="sm" variant="secondary" loading={props.busy} onClick={props.onCanary}>进灰度 20%</Button>
        <Button size="sm" loading={props.busy} onClick={props.onApprove}>批准并应用</Button>
        <span class="cost">
          <Show when={props.s.costUsd != null} fallback="—">¥{((props.s.costUsd ?? 0) * 7.3).toFixed(2)}</Show>
          <Show when={props.s.tokensInput != null}> · {props.s.tokensInput}+{props.s.tokensOutput ?? 0} tok</Show>
        </span>
        <Button size="xs" variant="ghost" onClick={() => setShowEvidence(!showEvidence())}>
          {showEvidence() ? '隐藏' : '查看'} evidence
        </Button>
      </div>
      <Show when={showEvidence()}>
        <pre class="m-4 p-2 bg-surface-secondary rounded text-[10px] overflow-auto font-mono max-h-64">{JSON.stringify(props.s.evidenceJson, null, 2)}</pre>
      </Show>
    </div>
  );
}
