import { createResource, createSignal, For, Show } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { Badge } from '@/components/ui/Badge';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { Tabs } from '@/components/ui/Tabs';
import { StatCard } from '@/components/ui/StatCard';
import { Modal } from '@/components/ui/Modal';
import { ConfirmDialog } from '@/components/ui/ConfirmDialog';
import { HeroCard } from '@/components/ui/HeroCard';
import { uiStore } from '@/stores/ui';
import { adminApi, type AmasSuggestion, type AmasSuggestionStatus } from '@/api/admin';
import { formatMoney } from '@/utils/formatters';

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
  // 替代原生 confirm/prompt：批准与拒绝走项目自带 ConfirmDialog / Modal
  const [approveTarget, setApproveTarget] = createSignal<AmasSuggestion | null>(null);
  const [rejectTarget, setRejectTarget] = createSignal<AmasSuggestion | null>(null);
  const [rejectNote, setRejectNote] = createSignal('');

  const [pending, { refetch: refetchPending }] = createResource(
    () => tab() === 'pending',
    async (on) => (on ? adminApi.amasListSuggestions('pending', 50) : []),
  );
  const [history, { refetch: refetchHistory }] = createResource(
    () => tab() === 'history',
    async (on) => (on ? adminApi.amasListSuggestions(undefined, 100) : []),
  );

  const [spend, { refetch: refetchSpend }] = createResource(async () => adminApi.amasSuggestionSpend());

  function approve(s: AmasSuggestion) {
    setApproveTarget(s);
  }

  async function confirmApprove() {
    const s = approveTarget();
    if (!s) return;
    setApproveTarget(null);
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

  function reject(s: AmasSuggestion) {
    setRejectNote('');
    setRejectTarget(s);
  }

  async function confirmReject() {
    const s = rejectTarget();
    if (!s) return;
    const note = rejectNote().trim() || undefined;
    setRejectTarget(null);
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
    <div class="space-y-4">
      <HeroCard
        eyebrow="每 20 分钟 · 白名单"
        eyebrowVariant="info"
        title="LLM 调参顾问"
        desc="DeepSeek patch 时间线、当前建议 diff、成本看板、灰度发布。接受 / 拒绝建议会写回 AMAS 配置。"
      />
      <Show
        when={!spend.error}
        fallback={<Card variant="elevated"><Empty title="额度信息加载失败" description={spend.error instanceof Error ? spend.error.message : '请稍后重试'} /></Card>}
      >
        <Show when={spend()} fallback={<Card variant="elevated"><Spinner size="sm" /></Card>}>
          {(s) => (
            <div class="grid grid-cols-2 lg:grid-cols-4 gap-3">
              <StatCard
                title="今日花费 USD"
                value={formatMoney(s().todayCostUsd, 4)}
                color="info"
                icon="M12 8c-1.657 0-3 .895-3 2s1.343 2 3 2 3 .895 3 2-1.343 2-3 2m0-8c1.11 0 2.08.402 2.599 1M12 8V7m0 1v8m0 0v1m0-1c-1.11 0-2.08-.402-2.599-1M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
              />
              <StatCard
                title="日额度上限"
                value={formatMoney(s().dailyCapUsd, 2)}
                color="accent"
                icon="M11 3.055A9.001 9.001 0 1020.945 13H11V3.055zM20.488 9H15V3.512A9.025 9.025 0 0120.488 9z"
              />
              <StatCard
                title="剩余额度"
                value={formatMoney(s().remainingUsd, 4)}
                color={s().remainingUsd < 0.1 ? 'error' : 'success'}
                icon="M3 10h18M7 15h1m4 0h1m-7 4h12a3 3 0 003-3V8a3 3 0 00-3-3H6a3 3 0 00-3 3v8a3 3 0 003 3z"
              />
              <StatCard
                title="今日 token (in/out)"
                value={`${s().todayTokensInput}/${s().todayTokensOutput}`}
                color="info"
                icon="M8 7h12m0 0l-4-4m4 4l-4 4m0 6H4m0 0l4 4m-4-4l4-4"
              />
            </div>
          )}
        </Show>
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
        <Show
          when={!pending.error}
          fallback={<Card variant="elevated"><Empty title="待审批列表加载失败" description={pending.error instanceof Error ? pending.error.message : '请稍后重试'} /></Card>}
        >
          <Show when={!pending.loading} fallback={<div class="flex justify-center py-12"><Spinner /></div>}>
            <Show when={(pending() ?? []).length > 0} fallback={<Card variant="elevated"><Empty title="暂无待审批建议" description="LLM advisor worker 每 20 分钟产出一次" /></Card>}>
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
      </Show>

      {/* 批准确认：用 ConfirmDialog 替代浏览器原生 confirm()，patch 用 pre 渲染 */}
      <ConfirmDialog
        open={!!approveTarget()}
        title="确认批准并应用 patch"
        message={
          <>
            将立即应用 patch 并新建版本：
            <Show when={approveTarget()}>
              <pre class="mt-2 text-xs max-h-64 overflow-auto p-2 bg-surface-secondary rounded font-mono whitespace-pre">
                {formatPatch(approveTarget()!.patchJson)}
              </pre>
            </Show>
          </>
        }
        confirmText="批准并应用"
        variant="warning"
        onConfirm={confirmApprove}
        onCancel={() => setApproveTarget(null)}
      />

      {/* 拒绝确认：用 Modal + textarea 替代浏览器原生 prompt() */}
      <Modal open={!!rejectTarget()} onClose={() => setRejectTarget(null)} title="拒绝建议" size="sm">
        <div class="space-y-3">
          <p class="text-sm text-content-secondary">填写拒绝原因（可选）：</p>
          <textarea
            value={rejectNote()}
            onInput={(e) => setRejectNote(e.currentTarget.value)}
            rows={4}
            placeholder="如：与当前线上版本冲突 / 风险过高 / 需要更多 evidence…"
            class="w-full px-3 py-2 rounded-lg text-sm bg-surface text-content border border-border-hairline focus-ring-soft focus:border-accent placeholder:text-content-tertiary"
          />
          <div class="flex justify-end gap-2">
            <Button size="sm" variant="ghost" onClick={() => setRejectTarget(null)}>取消</Button>
            <Button size="sm" variant="danger" onClick={confirmReject}>确认拒绝</Button>
          </div>
        </div>
      </Modal>

      <Show when={tab() === 'history'}>
        <Show
          when={!history.error}
          fallback={<Card variant="elevated"><Empty title="历史加载失败" description={history.error instanceof Error ? history.error.message : '请稍后重试'} /></Card>}
        >
          <Show when={!history.loading} fallback={<div class="flex justify-center py-12"><Spinner /></div>}>
            <Show when={(history() ?? []).length > 0} fallback={<Card variant="elevated"><Empty title="尚无历史" description="" /></Card>}>
            <Card variant="elevated">
              <div class="overflow-x-auto">
                <table class="w-full text-sm">
                  <thead>
                    <tr class="bg-surface-secondary/60 backdrop-blur-sm border-b border-border-hairline">
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
                        <tr class="border-b border-border-hairline">
                          <td class="px-3 py-2 text-xs text-content-tertiary tabular-nums whitespace-nowrap">{formatTime(s.createdAt)}</td>
                          <td class="px-3 py-2"><Badge variant={STATUS_VARIANT[s.status]} size="sm">{STATUS_LABEL[s.status]}</Badge></td>
                          <td class="px-3 py-2 font-mono text-xs tabular-nums">{s.basedOnVersionHash.slice(0, 10)}</td>
                          <td class="px-3 py-2 text-xs text-content max-w-md truncate" title={s.rationale}>{s.rationale}</td>
                          <td class="px-3 py-2 text-right font-mono text-xs tabular-nums">{s.costUsd != null ? formatMoney(s.costUsd, 4) : '—'}</td>
                          <td class="px-3 py-2 text-xs text-content-tertiary">{s.decidedBy ?? '—'}</td>
                        </tr>
                      )}
                    </For>
                  </tbody>
                </table>
              </div>
            </Card>
            </Show>
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
            <span class="text-xs text-content-tertiary tabular-nums">{formatMoney(props.s.costUsd!, 4)}</span>
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
            <tr class="text-content-tertiary border-b border-border-hairline">
              <th class="text-left py-1 pr-2">字段</th>
              <th class="text-right py-1">建议值</th>
            </tr>
          </thead>
          <tbody>
            <For each={Object.entries(props.s.patchJson)}>
              {([path, value]) => (
                <tr class="border-b border-border-hairline">
                  <td class="py-1 pr-2 text-content">{path}</td>
                  <td class="py-1 text-right text-success tabular-nums">{formatPatchValue(value)}</td>
                </tr>
              )}
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

function formatTime(iso: string): string {
  try {
    return new Date(iso).toLocaleString('zh-CN', { hour12: false });
  } catch {
    return iso;
  }
}

function formatPatch(p: Record<string, number>): string {
  return Object.entries(p).map(([k, v]) => `  ${k} → ${formatPatchValue(v)}`).join('\n');
}

/**
 * 数值显示策略：
 * - 非有限数（NaN/Infinity）退回字符串；
 * - |v| < 1e-4 用科学计数法（避免 toFixed(6) 截成 0.000000 丢精度）；
 * - 其余用 6 位有效数字，去掉尾随 0。
 */
function formatPatchValue(value: unknown): string {
  if (typeof value !== 'number') return String(value);
  if (!Number.isFinite(value)) return String(value);
  if (value === 0) return '0';
  if (Math.abs(value) < 1e-4) return value.toExponential(2);
  return value.toPrecision(6).replace(/\.?0+$/, '');
}
