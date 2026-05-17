import { createResource, createSignal, For, Show } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { Badge } from '@/components/ui/Badge';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { Tabs } from '@/components/ui/Tabs';
import { StatCard } from '@/components/ui/StatCard';
import { uiStore } from '@/stores/ui';
import { adminApi, type AmasSuggestion, type AmasSuggestionStatus } from '@/api/admin';

const STATUS_LABEL: Record<AmasSuggestionStatus, string> = {
  pending: '待审批',
  approved: '已批准',
  rejected: '已拒绝',
  superseded: '已被覆盖',
  expired: '已过期',
  auto_applied: '自动应用',
};
const STATUS_VARIANT: Record<AmasSuggestionStatus, 'default' | 'success' | 'error' | 'warning' | 'info' | 'accent'> = {
  pending: 'warning',
  approved: 'success',
  rejected: 'error',
  superseded: 'default',
  expired: 'default',
  auto_applied: 'info',
};

type TabId = 'pending' | 'history';

export default function AmasAdvisorPage() {
  const [tab, setTab] = createSignal<TabId>('pending');
  const [decidingId, setDecidingId] = createSignal<number | null>(null);

  const [pending, { refetch: refetchPending }] = createResource(
    () => tab() === 'pending',
    async (on) => (on ? adminApi.amasListSuggestions('pending', 50) : []),
  );
  const [history, { refetch: refetchHistory }] = createResource(
    () => tab() === 'history',
    async (on) => (on ? adminApi.amasListSuggestions(undefined, 100) : []),
  );

  const [spend, { refetch: refetchSpend }] = createResource(async () => adminApi.amasSuggestionSpend());

  async function approve(s: AmasSuggestion) {
    if (!confirm(`将立即应用 patch 并新建版本：\n${formatPatch(s.patchJson)}\n确认？`)) return;
    setDecidingId(s.id);
    try {
      const r = await adminApi.amasApproveSuggestion(s.id);
      uiStore.toast.success('已批准并应用', `新版本 ${r.versionHash.slice(0, 10)}`);
      void refetchPending();
      void refetchHistory();
      void refetchSpend();
    } catch (err) {
      uiStore.toast.error('应用失败', err instanceof Error ? err.message : '');
    } finally {
      setDecidingId(null);
    }
  }

  async function reject(s: AmasSuggestion) {
    const note = prompt('拒绝原因（可选）：') ?? undefined;
    setDecidingId(s.id);
    try {
      await adminApi.amasRejectSuggestion(s.id, note);
      uiStore.toast.success('已拒绝');
      void refetchPending();
      void refetchHistory();
    } catch (err) {
      uiStore.toast.error('拒绝失败', err instanceof Error ? err.message : '');
    } finally {
      setDecidingId(null);
    }
  }

  return (
    <div class="space-y-4 animate-fade-in-up">
      <Show when={spend()} fallback={<Card variant="elevated"><Spinner size="sm" /></Card>}>
        {(s) => (
          <div class="grid grid-cols-2 md:grid-cols-4 gap-3">
            <StatCard title="今日花费 USD" value={`$${s().todayCostUsd.toFixed(4)}`} icon="" color="info" />
            <StatCard title="日额度上限" value={`$${s().dailyCapUsd.toFixed(2)}`} icon="" color="accent" />
            <StatCard title="剩余额度" value={`$${s().remainingUsd.toFixed(4)}`} icon="" color={s().remainingUsd < 0.1 ? 'error' : 'success'} />
            <StatCard title="今日 token (in/out)" value={`${s().todayTokensInput}/${s().todayTokensOutput}`} icon="" color="info" />
          </div>
        )}
      </Show>

      <Tabs
        tabs={[
          { id: 'pending', label: '待审批' },
          { id: 'history', label: '历史' },
        ]}
        active={tab()}
        onChange={(id) => setTab(id as TabId)}
      />

      <Show when={tab() === 'pending'}>
        <Show when={!pending.loading} fallback={<div class="flex justify-center py-12"><Spinner /></div>}>
          <Show when={(pending() ?? []).length > 0} fallback={<Card variant="elevated"><Empty title="暂无待审批建议" description="LLM advisor worker 每 20 分钟产出一次；mock 模式启用后下一次会有 pending" /></Card>}>
            <For each={pending() ?? []}>
              {(s) => (
                <SuggestionCard
                  s={s}
                  busy={decidingId() === s.id}
                  onApprove={() => approve(s)}
                  onReject={() => reject(s)}
                />
              )}
            </For>
          </Show>
        </Show>
      </Show>

      <Show when={tab() === 'history'}>
        <Show when={!history.loading} fallback={<div class="flex justify-center py-12"><Spinner /></div>}>
          <Show when={(history() ?? []).length > 0} fallback={<Card variant="elevated"><Empty title="尚无历史" description="" /></Card>}>
            <Card variant="elevated">
              <table class="w-full text-sm">
                <thead>
                  <tr class="bg-surface-secondary border-b border-border">
                    <th scope="col" class="px-3 py-2 text-left font-medium text-content-secondary">时间</th>
                    <th scope="col" class="px-3 py-2 text-left font-medium text-content-secondary">状态</th>
                    <th scope="col" class="px-3 py-2 text-left font-medium text-content-secondary">基础版本</th>
                    <th scope="col" class="px-3 py-2 text-left font-medium text-content-secondary">理由（摘要）</th>
                    <th scope="col" class="px-3 py-2 text-right font-medium text-content-secondary">cost</th>
                    <th scope="col" class="px-3 py-2 text-left font-medium text-content-secondary">决策人</th>
                  </tr>
                </thead>
                <tbody>
                  <For each={history() ?? []}>
                    {(s) => (
                      <tr class="border-b border-border/40">
                        <td class="px-3 py-2 text-xs text-content-tertiary">{formatTime(s.createdAt)}</td>
                        <td class="px-3 py-2"><Badge variant={STATUS_VARIANT[s.status]} size="sm">{STATUS_LABEL[s.status]}</Badge></td>
                        <td class="px-3 py-2 font-mono text-xs">{s.basedOnVersionHash.slice(0, 10)}</td>
                        <td class="px-3 py-2 text-xs text-content max-w-md truncate" title={s.rationale}>{s.rationale}</td>
                        <td class="px-3 py-2 text-right font-mono text-xs">{s.costUsd != null ? `$${s.costUsd.toFixed(4)}` : '—'}</td>
                        <td class="px-3 py-2 text-xs text-content-tertiary">{s.decidedBy ?? '—'}</td>
                      </tr>
                    )}
                  </For>
                </tbody>
              </table>
            </Card>
          </Show>
        </Show>
      </Show>
    </div>
  );
}

function SuggestionCard(props: {
  s: AmasSuggestion;
  busy: boolean;
  onApprove: () => void;
  onReject: () => void;
}) {
  const [showEvidence, setShowEvidence] = createSignal(false);
  return (
    <Card variant="elevated">
      <div class="flex items-start justify-between gap-3 mb-2">
        <div class="flex items-center gap-2 flex-wrap">
          <Badge variant={STATUS_VARIANT[props.s.status]} size="sm">{STATUS_LABEL[props.s.status]}</Badge>
          <span class="text-xs font-mono text-content-tertiary">基于 {props.s.basedOnVersionHash.slice(0, 10)}</span>
          <span class="text-xs text-content-tertiary">{formatTime(props.s.createdAt)}</span>
          <Show when={props.s.confidence != null}>
            <Badge variant="info" size="sm">置信 {(props.s.confidence! * 100).toFixed(0)}%</Badge>
          </Show>
          <Show when={props.s.costUsd != null}>
            <span class="text-xs text-content-tertiary">${props.s.costUsd!.toFixed(4)}</span>
          </Show>
        </div>
        <div class="flex gap-2 shrink-0">
          <Button size="sm" variant="outline" loading={props.busy} onClick={props.onReject}>拒绝</Button>
          <Button size="sm" loading={props.busy} onClick={props.onApprove}>批准并应用</Button>
        </div>
      </div>

      <div class="text-sm text-content leading-relaxed mb-2">{props.s.rationale}</div>

      <div class="space-y-1.5">
        <h4 class="text-xs font-medium text-content-secondary">Patch（{Object.keys(props.s.patchJson).length} 项）</h4>
        <table class="w-full text-xs font-mono">
          <thead>
            <tr class="text-content-tertiary border-b border-border/40">
              <th class="text-left py-1 pr-2">字段</th>
              <th class="text-right py-1">建议值</th>
            </tr>
          </thead>
          <tbody>
            <For each={Object.entries(props.s.patchJson)}>
              {([path, value]) => (
                <tr class="border-b border-border/30">
                  <td class="py-1 pr-2 text-content">{path}</td>
                  <td class="py-1 text-right text-success">{typeof value === 'number' ? value.toFixed(6).replace(/0+$/, '').replace(/\.$/, '') : String(value)}</td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </div>

      <div class="mt-2">
        <button
          type="button"
          class="text-xs text-content-tertiary hover:text-accent"
          onClick={() => setShowEvidence(!showEvidence())}
        >
          {showEvidence() ? '隐藏' : '查看'} evidence
        </button>
        <Show when={showEvidence()}>
          <pre class="mt-2 p-2 bg-surface-secondary rounded text-[10px] overflow-x-auto font-mono max-h-64">
            {JSON.stringify(props.s.evidenceJson, null, 2)}
          </pre>
        </Show>
      </div>
    </Card>
  );
}

function formatTime(iso: string): string {
  try {
    return new Date(iso).toLocaleString('zh-CN', { hour12: false });
  } catch {
    return iso;
  }
}

function formatPatch(p: Record<string, number>): string {
  return Object.entries(p).map(([k, v]) => `  ${k} → ${v}`).join('\n');
}
