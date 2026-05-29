import { createMemo, createResource, createSignal, For, Show } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { HeroCard } from '@/components/ui/HeroCard';
import { uiStore } from '@/stores/ui';
import { adminApi, type AmasSuggestion, type PatchCanary } from '@/api/admin';
import { PageHeaderOps } from '@/pages/amas-advisor/PageHeaderOps';
import { CostRow } from '@/pages/amas-advisor/CostRow';
import { PatchTabs, type PatchTabId } from '@/pages/amas-advisor/PatchTabs';
import { CostChart } from '@/pages/amas-advisor/CostChart';
import { SuggestionCard } from '@/pages/amas-advisor/SuggestionCard';
import { PatchCanaryCard } from '@/pages/amas-advisor/PatchCanaryCard';
import { AdvisorConfigPanel } from '@/pages/amas-advisor/AdvisorConfigPanel';
import { WhitelistPanel } from '@/pages/amas-advisor/WhitelistPanel';
import { HistoryTable } from '@/pages/amas-advisor/HistoryTable';

export default function AmasAdvisorPage() {
  const [tab, setTab] = createSignal<PatchTabId>('pending');
  const [running, setRunning] = createSignal(false);
  // SuggestionCard / PatchCanaryCard 为纯展示组件，busy 与决策回调由本页托管
  const [decidingId, setDecidingId] = createSignal<number | null>(null);
  const [canaryBusyId, setCanaryBusyId] = createSignal<number | null>(null);

  const [cost, { refetch: refetchCost }] = createResource(() => adminApi.amasAdvisorCost());
  const [costDaily] = createResource(() => adminApi.amasAdvisorCostDaily(30));
  const [config, { refetch: refetchConfig }] = createResource(() => adminApi.amasAdvisorConfig());
  const [pending, { refetch: refetchPending }] = createResource(() => adminApi.amasListSuggestions('pending', 50));
  const [canaries, { refetch: refetchCanaries }] = createResource(() => adminApi.amasListCanaries());
  const [whitelist] = createResource(() => adminApi.amasListWhitelist());

  // cost 处于 error 态时直接 cost() 会 throw，统一走安全读取避免整页崩
  const costSafe = createMemo(() => (cost.error ? undefined : cost()));

  const counts = createMemo(() => ({
    pending: (pending() ?? []).length,
    canary: (canaries() ?? []).length,
    effective: costSafe()?.acceptedCount ?? 0,
    rejected: costSafe()?.rejectedCount ?? 0,
  }));

  const steps = createMemo<[number, number, number]>(() => config()?.grayscaleSteps ?? [20, 60, 100]);

  async function onRunNow() {
    setRunning(true);
    try {
      const r = await adminApi.amasAdvisorRun();
      uiStore.toast.success(r.produced ? '巡查完成，产出新建议' : '巡查完成，无新建议');
      void refetchPending();
      void refetchCost();
    } catch (e) {
      uiStore.toast.error('触发失败', e instanceof Error ? e.message : '');
    } finally {
      setRunning(false);
    }
  }

  async function onToggleAutoScan(next: boolean) {
    try {
      await adminApi.amasUpdateAdvisorConfig({ advisorEnabled: next });
      uiStore.toast.success(next ? '已启用自动巡查' : '已关闭自动巡查');
      void refetchConfig();
    } catch (e) {
      uiStore.toast.error('设置失败', e instanceof Error ? e.message : '');
    }
  }

  async function onApproveAll() {
    try {
      const r = await adminApi.amasApproveAllSuggestions();
      const ok = r.results.filter((x) => x.ok).length;
      uiStore.toast.success(`已批准 ${ok}/${r.results.length} 条`);
      void refetchPending();
      void refetchCost();
      void refetchCanaries();
    } catch (e) {
      uiStore.toast.error('批量批准失败', e instanceof Error ? e.message : '');
    }
  }

  // ── 单条建议决策（SuggestionCard 回调）──
  async function approveSuggestion(s: AmasSuggestion) {
    setDecidingId(s.id);
    try {
      const r = await adminApi.amasApproveSuggestion(s.id);
      uiStore.toast.success('已批准并应用', `新版本 ${r.versionHash.slice(0, 10)}`);
      void refetchPending();
      void refetchCost();
    } catch (e) {
      uiStore.toast.error('应用失败', e instanceof Error ? e.message : '');
    } finally {
      setDecidingId(null);
    }
  }

  async function rejectSuggestion(s: AmasSuggestion) {
    setDecidingId(s.id);
    try {
      await adminApi.amasRejectSuggestion(s.id);
      uiStore.toast.success('已拒绝');
      void refetchPending();
      void refetchCost();
    } catch (e) {
      uiStore.toast.error('拒绝失败', e instanceof Error ? e.message : '');
    } finally {
      setDecidingId(null);
    }
  }

  async function canarySuggestion(s: AmasSuggestion) {
    setDecidingId(s.id);
    try {
      await adminApi.amasCreateCanary({ suggestionId: s.id, percent: steps()[0] });
      uiStore.toast.success('已进灰度', `初始 ${steps()[0]}%`);
      void refetchPending();
      void refetchCanaries();
    } catch (e) {
      uiStore.toast.error('进灰度失败', e instanceof Error ? e.message : '');
    } finally {
      setDecidingId(null);
    }
  }

  // ── canary 操作（PatchCanaryCard 回调）──
  async function scaleCanary(c: PatchCanary, percent: number) {
    setCanaryBusyId(c.id);
    try {
      await adminApi.amasScaleCanary(c.id, percent);
      uiStore.toast.success(`已扩量到 ${percent}%`);
      void refetchCanaries();
      void refetchCost();
    } catch (e) {
      uiStore.toast.error('扩量失败', e instanceof Error ? e.message : '');
    } finally {
      setCanaryBusyId(null);
    }
  }

  async function rollbackCanary(c: PatchCanary) {
    setCanaryBusyId(c.id);
    try {
      await adminApi.amasRollbackCanary(c.id);
      uiStore.toast.success('已回滚灰度');
      void refetchCanaries();
      void refetchCost();
    } catch (e) {
      uiStore.toast.error('回滚失败', e instanceof Error ? e.message : '');
    } finally {
      setCanaryBusyId(null);
    }
  }

  async function promoteCanary(c: PatchCanary) {
    setCanaryBusyId(c.id);
    try {
      const r = await adminApi.amasPromoteCanary(c.id);
      uiStore.toast.success('已提升为 stable', `版本 ${r.versionHash.slice(0, 10)}`);
      void refetchCanaries();
      void refetchCost();
    } catch (e) {
      uiStore.toast.error('提升失败', e instanceof Error ? e.message : '');
    } finally {
      setCanaryBusyId(null);
    }
  }

  return (
    <div class="space-y-4">
      <div class="flex flex-wrap items-start justify-between gap-3">
        <HeroCard
          eyebrow="每 20 分钟 · 白名单"
          eyebrowVariant="info"
          title="LLM 调参顾问"
          desc="每 20 分钟跑一次 DeepSeek，对照 7 日运营指标输出参数 patch。白名单内自动灰度，超出白名单待人工审核。所有 patch 可一键回滚，写入审计日志。"
        />
        <Show when={config()}>
          {(c) => (
            <PageHeaderOps
              advisorEnabled={c().advisorEnabled}
              running={running()}
              pendingCount={counts().pending}
              onToggleAutoScan={onToggleAutoScan}
              onRunNow={onRunNow}
              onApproveAll={onApproveAll}
            />
          )}
        </Show>
      </div>

      {/* 成本行（全宽，失败降级不崩页） */}
      <Show
        when={!cost.error}
        fallback={<Card variant="elevated"><Empty title="成本信息加载失败" description={cost.error instanceof Error ? cost.error.message : '请稍后重试'} /></Card>}
      >
        <Show when={cost()} fallback={<Card variant="elevated"><div class="flex justify-center py-8"><Spinner size="sm" /></div></Card>}>
          {(c) => <CostRow stats={c()} />}
        </Show>
      </Show>

      <PatchTabs active={tab()} counts={counts()} onChange={setTab} />

      {/* 主体 12 栅格双栏 */}
      <div class="grid grid-cols-1 lg:grid-cols-12 gap-4">
        <div class="lg:col-span-8 space-y-3">
          <Show when={tab() === 'pending'}>
            <Show when={(pending() ?? []).length > 0} fallback={<Card variant="elevated"><Empty title="暂无待审批建议" description="LLM advisor worker 每 20 分钟产出一次" /></Card>}>
              <For each={pending() ?? []}>
                {(s) => (
                  <SuggestionCard
                    s={s}
                    whitelist={whitelist() ?? []}
                    busy={decidingId() === s.id}
                    onApprove={() => void approveSuggestion(s)}
                    onReject={() => void rejectSuggestion(s)}
                    onCanary={() => void canarySuggestion(s)}
                  />
                )}
              </For>
            </Show>
          </Show>
          <Show when={tab() === 'canary'}>
            <Show when={(canaries() ?? []).length > 0} fallback={<Card variant="elevated"><Empty title="暂无灰度中 patch" description="批准建议时选择「进灰度」即在此监测" /></Card>}>
              <For each={canaries() ?? []}>
                {(c) => (
                  <PatchCanaryCard
                    c={c}
                    steps={steps()}
                    busy={canaryBusyId() === c.id}
                    onScale={(percent) => void scaleCanary(c, percent)}
                    onRollback={() => void rollbackCanary(c)}
                    onPromote={() => void promoteCanary(c)}
                  />
                )}
              </For>
            </Show>
          </Show>
          <Show when={tab() === 'effective' || tab() === 'rejected'}>
            <Card variant="elevated">
              <Empty
                title={tab() === 'effective' ? '已生效 patch' : '已拒绝 patch'}
                description="已决策记录见下方「已生效 Patch 历史」表，支持按状态/关键字搜索、分页与导出 CSV。"
              />
            </Card>
          </Show>
        </div>
        <div class="lg:col-span-4 space-y-3">
          <Show when={costDaily()}>
            {(d) => <CostChart data={d()} avg7dYuan={costSafe()?.avg7dCostYuan ?? 0} capYuan={costSafe()?.monthCapYuan ?? 0} refLineYuan={0.3} />}
          </Show>
          <AdvisorConfigPanel />
          <WhitelistPanel />
        </div>
      </div>

      {/* 历史表（全宽）：始终展示已决策历史，自带搜索/分页/导出/回滚 */}
      <HistoryTable />
    </div>
  );
}
