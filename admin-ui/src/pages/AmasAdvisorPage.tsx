/* ============================================================
   AMAS — LLM 调参顾问 (advisor)
   成本/配额 + 每日成本趋势 + 建议队列(批准/拒绝/进灰度/解释/沙箱)
   + 批准全部 + 灰度管理 + 白名单 + 顾问配置 + 导出 CSV
   ============================================================ */
import { createEffect, createMemo, createResource, createSignal, For, Show, type Accessor } from 'solid-js';
import {
  Badge, Btn, Card, Drawer, Empty, Field, IconBtn, Loading, PageHead,
  Panel, Progress, Switch, Tabs, LineChart, fmtNum, fmtAgo, toast, sx,
} from '@/components/wf';
import type { JSX } from 'solid-js';
import {
  adminApi,
  type AmasSuggestion,
  type AmasSuggestionStatus,
  type AdvisorConfig,
  type WhitelistRow,
  type PatchCanaryWithMetrics,
  type AmasExplainResponse,
  type SandboxSuggestionResponse,
  type SandboxMetricImpact,
} from '@/api/admin';

/* 状态 → [中文标签, Badge variant] */
const SUGG_STATUS: Record<AmasSuggestionStatus, [string, 'warning' | 'success' | 'error' | 'accent' | 'default']> = {
  pending: ['待审', 'warning'],
  approved: ['已批准', 'success'],
  rejected: ['已拒绝', 'error'],
  auto_applied: ['自动应用', 'accent'],
  superseded: ['被取代', 'default'],
  expired: ['已过期', 'default'],
};

type TabId = 'pending' | 'canary' | 'history' | 'whitelist' | 'config';

/* 安全读取 evidenceJson 中的数值字段 */
function evNum(ev: Record<string, unknown> | undefined, key: string): number | undefined {
  const v = ev?.[key];
  return typeof v === 'number' ? v : undefined;
}

/* ---------- 小指标块（对齐设计稿 MetricBox） ---------- */
function MetricBox(props: { label: string; value: string | number }) {
  return (
    <div style={sx({ padding: 10, borderRadius: 10, background: 'var(--surface-sunken)' })}>
      <div class="muted-3" style={sx({ fontSize: 10.5 })}>{props.label}</div>
      <div class="mono tnum" style={sx({ fontSize: 16, fontWeight: 700, marginTop: 2 })}>{props.value}</div>
    </div>
  );
}

/* ---------- 主页面 ---------- */
export default function AmasAdvisorPage() {
  const [tab, setTab] = createSignal<TabId>('pending');
  const [running, setRunning] = createSignal(false);
  const [detail, setDetail] = createSignal<AmasSuggestion | null>(null);

  const [cost, { refetch: refetchCost }] = createResource(() => adminApi.amasAdvisorCost());
  const [costDaily] = createResource(() => adminApi.amasAdvisorCostDaily(30));
  const [config, { refetch: refetchConfig }] = createResource(() => adminApi.amasAdvisorConfig());
  // 拉取全量(不带 status,默认 limit 50)；前端按 status 分桶，复刻设计稿单列表逻辑
  const [sugg, { refetch: refetchSugg }] = createResource(() => adminApi.amasListSuggestions(undefined, 200));
  const [canaries, { refetch: refetchCanaries }] = createResource(() => adminApi.amasListCanaries());
  const [whitelist, { refetch: refetchWhitelist }] = createResource(() => adminApi.amasListWhitelist());

  // 任一资源 error 时安全降级，避免整页崩
  const costSafe = createMemo(() => (cost.error ? undefined : cost()));
  const costDailySafe = createMemo(() => (costDaily.error ? [] : costDaily() ?? []));
  const configSafe = createMemo(() => (config.error ? undefined : config()));
  const list = createMemo<AmasSuggestion[]>(() => (sugg.error ? [] : sugg() ?? []));
  const canariesSafe = createMemo<PatchCanaryWithMetrics[]>(() => (canaries.error ? [] : canaries() ?? []));
  const whitelistSafe = createMemo<WhitelistRow[]>(() => (whitelist.error ? [] : whitelist() ?? []));

  const byStatus = (s: AmasSuggestionStatus) => list().filter((x) => x.status === s);
  const pendingList = createMemo(() => byStatus('pending'));
  const historyList = createMemo(() => list().filter((x) => x.status !== 'pending'));
  const activeCanaries = createMemo(() => canariesSafe().filter((c) => c.status === 'active'));

  const reloadSuggAndCanary = () => { void refetchSugg(); void refetchCanaries(); void refetchCost(); };

  async function runNow() {
    setRunning(true);
    try {
      const r = await adminApi.amasAdvisorRun();
      toast.success('已触发顾问巡查', r.produced ? '产出新建议' : '无新建议');
      void refetchSugg();
      void refetchCost();
    } catch (e) {
      toast.error('巡查失败', e instanceof Error ? e.message : '');
    } finally {
      setRunning(false);
    }
  }

  async function approveAll() {
    try {
      const r = await adminApi.amasApproveAllSuggestions();
      const ok = r.results.filter((x) => x.ok).length;
      toast.success('已批准全部待审建议', `${ok}/${r.results.length} 条`);
      reloadSuggAndCanary();
    } catch (e) {
      toast.error('批量批准失败', e instanceof Error ? e.message : '');
    }
  }

  async function exportCsv() {
    try {
      const csv = await adminApi.amasExportSuggestionsCsv();
      const blob = new Blob([csv], { type: 'text/csv;charset=utf-8' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `amas-suggestions-${new Date().toISOString().slice(0, 10)}.csv`;
      document.body.appendChild(a);
      a.click();
      a.remove();
      URL.revokeObjectURL(url);
      toast.success('已导出 CSV');
    } catch (e) {
      toast.error('导出失败', e instanceof Error ? e.message : '');
    }
  }

  return (
    <div class="fade-up">
      <PageHead
        title="LLM 调参顾问"
        desc="LLM 周期性分析遥测并提出参数 patch；管理员审批后进入灰度，可沙箱试运行、设白名单与成本上限。"
        right={
          <>
            <Btn variant="ghost" icon="download" onClick={() => void exportCsv()}>导出 CSV</Btn>
            <Btn variant="secondary" icon="play" onClick={() => void runNow()} disabled={running()}>
              {running() ? '巡查中…' : '立即巡查'}
            </Btn>
            <Show when={pendingList().length > 0}>
              <Btn variant="primary" icon="check" onClick={() => void approveAll()}>批准全部</Btn>
            </Show>
          </>
        }
      />

      {/* cost + 每日趋势 */}
      <div style={sx({ display: 'grid', gridTemplateColumns: 'minmax(0,1.2fr) minmax(0,1fr)', gap: 16, marginBottom: 16 })} class="grid-collapse">
        <Panel title="本月成本" sub={costSafe() ? `配额 ¥${costSafe()!.monthCapYuan} · 汇率 ${costSafe()!.usdToCny}` : ''}>
          <Show when={!cost.error} fallback={<Empty title="成本信息加载失败" desc={cost.error instanceof Error ? cost.error.message : '请稍后重试'} icon="alert" />}>
            <Show when={costSafe()} fallback={<Loading h={150} />}>
              {(c) => (
                <div>
                  <div style={sx({ display: 'flex', alignItems: 'baseline', gap: 8, marginBottom: 10 })}>
                    <span class="tnum" style={sx({ fontSize: 32, fontWeight: 800 })}>¥{c().monthYuan.toFixed(1)}</span>
                    <span class="muted">/ ¥{c().monthCapYuan} · {(c().quotaPct * 100).toFixed(0)}%</span>
                  </div>
                  <Progress value={c().quotaPct * 100} tone={c().quotaPct > 0.8 ? 'warning' : 'accent'} height={8} />
                  <div style={sx({ display: 'grid', gridTemplateColumns: 'repeat(4,1fr)', gap: 10, marginTop: 14 })}>
                    <MetricBox label="预测月底" value={`¥${c().forecastYuan.toFixed(0)}`} />
                    <MetricBox label="7d 日均" value={`¥${c().avg7dCostYuan.toFixed(1)}`} />
                    <MetricBox label="本月调用" value={fmtNum(c().monthCalls)} />
                    <MetricBox label="采纳率" value={`${(c().acceptanceRate * 100).toFixed(0)}%`} />
                  </div>
                </div>
              )}
            </Show>
          </Show>
        </Panel>
        <Panel title="每日成本趋势" sub="近 30 天">
          <Show when={costDaily.state !== 'pending'} fallback={<Loading h={150} />}>
            <LineChart
              height={170}
              fmtY={(v) => `¥${v.toFixed(0)}`}
              labels={costDailySafe().map((d) => d.date.slice(5))}
              series={[{ name: '日成本', data: costDailySafe().map((d) => d.costYuan), color: 'var(--accent)' }]}
            />
          </Show>
        </Panel>
      </div>

      <Tabs
        value={tab()}
        onChange={(v) => setTab(v as TabId)}
        tabs={[
          { value: 'pending', label: '待审建议', count: pendingList().length },
          { value: 'canary', label: '灰度中', count: activeCanaries().length },
          { value: 'history', label: '历史', count: historyList().length },
          { value: 'whitelist', label: '白名单', count: whitelistSafe().length },
          { value: 'config', label: '顾问配置' },
        ]}
      />

      <div style={sx({ marginTop: 16 })}>
        {/* 待审建议 */}
        <Show when={tab() === 'pending'}>
          <Show when={sugg.state !== 'pending'} fallback={<Loading />}>
            <Show
              when={pendingList().length > 0}
              fallback={<Card><Empty title="暂无待审建议" desc="LLM advisor 每 20 分钟巡查一次。" icon="bulb" /></Card>}
            >
              <div style={sx({ display: 'flex', flexDirection: 'column', gap: 12 })}>
                <For each={pendingList()}>
                  {(s) => <SuggestionCard s={s} onOpen={() => setDetail(s)} onReload={reloadSuggAndCanary} />}
                </For>
              </div>
            </Show>
          </Show>
        </Show>

        {/* 灰度中 */}
        <Show when={tab() === 'canary'}>
          <Show when={canaries.state !== 'pending'} fallback={<Loading />}>
            <CanaryList canaries={canariesSafe()} onReload={() => { void refetchCanaries(); void refetchCost(); }} />
          </Show>
        </Show>

        {/* 历史 */}
        <Show when={tab() === 'history'}>
          <Show when={sugg.state !== 'pending'} fallback={<Loading />}>
            <HistoryTable rows={historyList()} onOpen={setDetail} onReload={reloadSuggAndCanary} />
          </Show>
        </Show>

        {/* 白名单 */}
        <Show when={tab() === 'whitelist'}>
          <Show when={whitelist.state !== 'pending'} fallback={<Loading />}>
            <WhitelistPanel rows={whitelistSafe()} onReload={() => void refetchWhitelist()} />
          </Show>
        </Show>

        {/* 顾问配置 */}
        <Show when={tab() === 'config'}>
          <Show when={config.state !== 'pending'} fallback={<Loading />}>
            <Show when={configSafe()} fallback={<Card><Empty title="配置加载失败" desc="请稍后重试" icon="alert" /></Card>}>
              {(c) => <AdvisorConfigPanel cfg={c} onReload={() => void refetchConfig()} />}
            </Show>
          </Show>
        </Show>
      </div>

      <SuggestionDrawer s={detail()} onClose={() => setDetail(null)} onReload={reloadSuggAndCanary} />
    </div>
  );
}

/* ---------- 建议卡片 ---------- */
function SuggestionCard(props: { s: AmasSuggestion; onOpen: () => void; onReload: () => void }) {
  const s = () => props.s;
  const [busy, setBusy] = createSignal('');
  const approve = async (e: MouseEvent) => {
    e.stopPropagation();
    setBusy('approve');
    try {
      const r = await adminApi.amasApproveSuggestion(s().id);
      toast.success(`已批准 #${s().id}`, `新版本 ${r.versionHash.slice(0, 10)}`);
      props.onReload();
    } catch (err) {
      toast.error('批准失败', err instanceof Error ? err.message : '');
    } finally {
      setBusy('');
    }
  };
  const reject = async (e: MouseEvent) => {
    e.stopPropagation();
    setBusy('reject');
    try {
      await adminApi.amasRejectSuggestion(s().id, '人工拒绝');
      toast.info(`已拒绝 #${s().id}`);
      props.onReload();
    } catch (err) {
      toast.error('拒绝失败', err instanceof Error ? err.message : '');
    } finally {
      setBusy('');
    }
  };
  const sampleSize = () => evNum(s().evidenceJson, 'sampleSize');
  return (
    <Card hover style={sx({ cursor: 'pointer' })} onClick={() => props.onOpen()}>
      <div style={sx({ display: 'flex', justifyContent: 'space-between', gap: 12, marginBottom: 9 })}>
        <div style={sx({ display: 'flex', gap: 9, alignItems: 'center' })}>
          <Badge variant="warning">待审 #{s().id}</Badge>
          <span class="muted-3" style={sx({ fontSize: 11.5 })}>{fmtAgo(s().createdAt)}</span>
        </div>
        <div style={sx({ display: 'flex', gap: 7 })}>
          <Btn size="sm" variant="secondary" disabled={busy() !== ''} onClick={reject}>拒绝</Btn>
          <Btn size="sm" variant="primary" disabled={busy() !== ''} onClick={approve}>批准并应用</Btn>
        </div>
      </div>
      <p style={sx({ margin: '0 0 11px', fontSize: 13.5, lineHeight: 1.55 })}>{s().rationale}</p>
      <div style={sx({ display: 'flex', gap: 8, flexWrap: 'wrap', marginBottom: 10 })}>
        <For each={Object.entries(s().patchJson)}>
          {([k, v]) => (
            <span class="mono" style={sx({ fontSize: 11.5, padding: '4px 9px', borderRadius: 8, background: 'var(--surface-sunken)', border: '1px solid var(--hairline)' })}>
              {k}: <b style={sx({ color: 'var(--text-3)', textDecoration: 'line-through' })}>{s().baseValuesJson?.[k] ?? '?'}</b> → <b style={sx({ color: 'var(--accent)' })}>{v}</b>
            </span>
          )}
        </For>
      </div>
      <div class="muted-3" style={sx({ fontSize: 11.5, display: 'flex', gap: 14, flexWrap: 'wrap' })}>
        <span>置信度 <b class="mono">{s().confidence != null ? `${(s().confidence! * 100).toFixed(0)}%` : '—'}</b></span>
        <span>成本 <b class="mono">{s().costUsd != null ? `$${s().costUsd!.toFixed(4)}` : '—'}</b></span>
        <span>样本 <b class="mono">{sampleSize() != null ? fmtNum(sampleSize()!) : '—'}</b></span>
      </div>
    </Card>
  );
}

/* ---------- 建议详情抽屉（含 LLM 解释 / 沙箱试运行） ---------- */
function SuggestionDrawer(props: { s: AmasSuggestion | null; onClose: () => void; onReload: () => void }) {
  const [explain, setExplain] = createSignal<AmasExplainResponse | null>(null);
  const [sandbox, setSandbox] = createSignal<SandboxSuggestionResponse | null>(null);
  const [busy, setBusy] = createSignal('');
  const s = () => props.s;

  // 切换建议时清空上一条结果
  let lastId: number | null = null;
  createEffect(() => {
    const id = props.s?.id ?? null;
    if (id !== lastId) {
      lastId = id;
      setExplain(null);
      setSandbox(null);
      setBusy('');
    }
  });

  const doExplain = async () => {
    const cur = s();
    if (!cur) return;
    setBusy('explain');
    try {
      const path = Object.keys(cur.patchJson)[0];
      const r = await adminApi.amasExplainParam(path, cur.patchJson[path]);
      setExplain(r);
    } catch (e) {
      toast.error('解释失败', e instanceof Error ? e.message : '');
    } finally {
      setBusy('');
    }
  };
  const doSandbox = async () => {
    const cur = s();
    if (!cur) return;
    setBusy('sandbox');
    try {
      const r = await adminApi.amasSandboxSuggestion(cur.id);
      setSandbox(r);
    } catch (e) {
      toast.error('沙箱试运行失败', e instanceof Error ? e.message : '');
    } finally {
      setBusy('');
    }
  };
  const approve = async () => {
    const cur = s();
    if (!cur) return;
    try {
      await adminApi.amasApproveSuggestion(cur.id);
      toast.success(`已批准 #${cur.id}`);
      props.onReload();
      props.onClose();
    } catch (e) {
      toast.error('批准失败', e instanceof Error ? e.message : '');
    }
  };
  const reject = async () => {
    const cur = s();
    if (!cur) return;
    try {
      await adminApi.amasRejectSuggestion(cur.id, '人工拒绝');
      toast.info(`已拒绝 #${cur.id}`);
      props.onReload();
      props.onClose();
    } catch (e) {
      toast.error('拒绝失败', e instanceof Error ? e.message : '');
    }
  };

  return (
    <Drawer
      open={!!props.s}
      onClose={() => props.onClose()}
      title={props.s ? `建议 #${props.s.id}` : ''}
      subtitle={props.s ? `based on ${props.s.basedOnVersionHash}` : ''}
      width={560}
      footer={
        <Show
          when={props.s?.status === 'pending'}
          fallback={props.s ? <Badge variant={SUGG_STATUS[props.s.status][1]}>{SUGG_STATUS[props.s.status][0]}</Badge> : undefined}
        >
          <Btn variant="secondary" onClick={() => void reject()}>拒绝</Btn>
          <Btn variant="primary" onClick={() => void approve()}>批准并应用</Btn>
        </Show>
      }
    >
      <Show when={props.s}>
        {(cur) => (
          <div>
            <p style={sx({ marginTop: 0, fontSize: 14, lineHeight: 1.6 })}>{cur().rationale}</p>
            <div class="eyebrow" style={sx({ margin: '14px 0 8px' })}>参数 patch</div>
            <For each={Object.entries(cur().patchJson)}>
              {([k, v]) => (
                <div style={sx({ display: 'flex', justifyContent: 'space-between', padding: '8px 0', borderBottom: '1px solid var(--hairline)', fontSize: 12.5 })}>
                  <span class="mono">{k}</span>
                  <span class="mono">
                    <span style={sx({ color: 'var(--text-3)', textDecoration: 'line-through' })}>{cur().baseValuesJson?.[k] ?? '?'}</span>
                    {' → '}<b style={sx({ color: 'var(--accent)' })}>{v}</b>
                  </span>
                </div>
              )}
            </For>
            <div style={sx({ display: 'flex', gap: 9, margin: '16px 0' })}>
              <Btn size="sm" variant="secondary" icon="bulb" onClick={() => void doExplain()} disabled={busy() === 'explain'}>
                {busy() === 'explain' ? '解释中…' : 'LLM 解释'}
              </Btn>
              <Btn size="sm" variant="secondary" icon="play" onClick={() => void doSandbox()} disabled={busy() === 'sandbox'}>
                {busy() === 'sandbox' ? '试运行…' : '沙箱试运行'}
              </Btn>
            </div>

            <Show when={explain()}>
              {(ex) => (
                <div class="fade-in" style={sx({ padding: 13, borderRadius: 11, background: 'var(--accent-soft)', marginBottom: 14 })}>
                  <div class="eyebrow" style={sx({ marginBottom: 6 })}>LLM 解释 · {ex().model} · ${ex().costUsd.toFixed(4)}</div>
                  <p style={sx({ margin: 0, fontSize: 12.5, lineHeight: 1.6 })}>{ex().explanation}</p>
                </div>
              )}
            </Show>

            <Show when={sandbox()}>
              {(sb) => (
                <div class="fade-in" style={sx({ padding: 13, borderRadius: 11, background: 'var(--surface-sunken)' })}>
                  <div class="eyebrow" style={sx({ marginBottom: 8 })}>沙箱结果 · 样本 {fmtNum(sb().telemetrySampleSize)} · 置信度 {sb().confidence}</div>
                  <div style={sx({ display: 'grid', gridTemplateColumns: '1fr 1fr 1fr', gap: 9 })}>
                    <SandboxMetric label="正确率" m={sb().accuracy} />
                    <SandboxMetric label="疲劳" m={sb().fatigue} invert />
                    <SandboxMetric label="7d 留存" m={sb().d7Retention} />
                  </div>
                  <div style={sx({ marginTop: 10, fontSize: 11.5 })} class={sb().whitelistOk ? 'muted' : ''}>
                    {sb().whitelistOk ? '✓ 所有改动在白名单区间内' : '⚠ 部分改动超出白名单'}
                  </div>
                </div>
              )}
            </Show>

            <div class="eyebrow" style={sx({ margin: '16px 0 8px' })}>证据</div>
            <div class="muted" style={sx({ fontSize: 12.5 })}>
              样本量 {evNum(cur().evidenceJson, 'sampleSize') != null ? fmtNum(evNum(cur().evidenceJson, 'sampleSize')!) : '—'}
              {' · '}窗口 {evNum(cur().evidenceJson, 'windowDays') ?? '—'} 天
              {' · '}tokens {cur().tokensInput ?? '—'}/{cur().tokensOutput ?? '—'}
            </div>
          </div>
        )}
      </Show>
    </Drawer>
  );
}

function SandboxMetric(props: { label: string; m: SandboxMetricImpact; invert?: boolean }) {
  const m = () => props.m;
  const good = () => (props.invert ? m().deltaPt < 0 : m().deltaPt > 0);
  return (
    <div style={sx({ padding: 10, borderRadius: 9, background: 'var(--surface-solid)' })}>
      <div class="muted-3" style={sx({ fontSize: 10.5 })}>{props.label}</div>
      <div class="mono" style={sx({ fontSize: 16, fontWeight: 700, color: m().deltaPt === 0 ? 'var(--text)' : good() ? 'var(--success)' : 'var(--error)' })}>
        {m().deltaPt > 0 ? '+' : ''}{m().deltaPt}pt
      </div>
      <Show when={m().baseline != null && m().predicted != null}>
        <div class="muted-3 mono" style={sx({ fontSize: 10 })}>{(m().baseline! * 100).toFixed(0)}→{(m().predicted! * 100).toFixed(0)}</div>
      </Show>
    </div>
  );
}

/* ---------- 历史表 ---------- */
function HistoryTable(props: { rows: AmasSuggestion[]; onOpen: (s: AmasSuggestion) => void; onReload: () => void }) {
  const rollback = async (s: AmasSuggestion) => {
    try {
      const r = await adminApi.amasRollbackSuggestion(s.id);
      toast.success(`已回滚 #${s.id}`, `版本 ${r.versionHash.slice(0, 10)}`);
      props.onReload();
    } catch (e) {
      toast.error('回滚失败', e instanceof Error ? e.message : '');
    }
  };
  return (
    <Show when={props.rows.length > 0} fallback={<Card><Empty title="暂无历史记录" desc="被决策的建议会在此列出。" icon="clock" /></Card>}>
      <Card pad={false} style={sx({ overflow: 'hidden' })}>
        <div style={sx({ overflowX: 'auto' })}>
          <table class="tbl">
            <thead>
              <tr>
                <th>ID</th><th>状态</th><th>理由</th><th>patch</th><th>置信度</th><th>成本</th><th>决策人</th>
                <th style={sx({ textAlign: 'right' })}>操作</th>
              </tr>
            </thead>
            <tbody>
              <For each={props.rows}>
                {(s) => (
                  <tr>
                    <td class="mono">#{s.id}</td>
                    <td><Badge variant={SUGG_STATUS[s.status][1]}>{SUGG_STATUS[s.status][0]}</Badge></td>
                    <td style={sx({ fontSize: 12.5, maxWidth: 280, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' })}>{s.rationale}</td>
                    <td class="mono">{Object.keys(s.patchJson).length}</td>
                    <td class="mono">{s.confidence != null ? `${(s.confidence * 100).toFixed(0)}%` : '—'}</td>
                    <td class="mono">{s.costUsd != null ? `$${s.costUsd.toFixed(4)}` : '—'}</td>
                    <td class="mono muted" style={sx({ fontSize: 11.5 })}>{s.decidedBy || '—'}</td>
                    <td style={sx({ textAlign: 'right', whiteSpace: 'nowrap' })}>
                      <Btn size="xs" variant="ghost" onClick={() => props.onOpen(s)}>详情</Btn>
                      <Show when={s.status === 'approved' || s.status === 'auto_applied'}>
                        <Btn size="xs" variant="outline" icon="rotate" onClick={() => void rollback(s)}>回滚</Btn>
                      </Show>
                    </td>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
        </div>
      </Card>
    </Show>
  );
}

/* ---------- 灰度列表 ---------- */
function CanaryList(props: { canaries: PatchCanaryWithMetrics[]; onReload: () => void }) {
  const [busyId, setBusyId] = createSignal<number | null>(null);
  const scale = async (id: number, percent: number) => {
    setBusyId(id);
    try {
      await adminApi.amasScaleCanary(id, percent);
      toast.success(`已调整灰度比例至 ${percent}%`);
      props.onReload();
    } catch (e) {
      toast.error('扩量失败', e instanceof Error ? e.message : '');
    } finally {
      setBusyId(null);
    }
  };
  const promote = async (c: PatchCanaryWithMetrics) => {
    setBusyId(c.id);
    try {
      const r = await adminApi.amasPromoteCanary(c.id);
      toast.success(`已全量发布 #${c.suggestionId}`, `版本 ${r.versionHash.slice(0, 10)}`);
      props.onReload();
    } catch (e) {
      toast.error('全量发布失败', e instanceof Error ? e.message : '');
    } finally {
      setBusyId(null);
    }
  };
  const rollback = async (c: PatchCanaryWithMetrics) => {
    setBusyId(c.id);
    try {
      await adminApi.amasRollbackCanary(c.id);
      toast.success(`已回滚 #${c.suggestionId}`);
      props.onReload();
    } catch (e) {
      toast.error('回滚失败', e instanceof Error ? e.message : '');
    } finally {
      setBusyId(null);
    }
  };

  return (
    <Show
      when={props.canaries.length > 0}
      fallback={<Card><Empty title="暂无灰度中 patch" desc="批准建议时选择「进灰度」即在此监测。" icon="layers" /></Card>}
    >
      <div style={sx({ display: 'flex', flexDirection: 'column', gap: 12 })}>
        <For each={props.canaries}>
          {(c) => (
            <Card>
              <div style={sx({ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 12, flexWrap: 'wrap', gap: 10 })}>
                <div style={sx({ display: 'flex', gap: 9, alignItems: 'center' })}>
                  <Badge variant={c.status === 'active' ? 'accent' : c.status === 'rolled_back' ? 'error' : 'success'} dot>{c.status}</Badge>
                  <span class="mono" style={sx({ fontSize: 12.5 })}>建议 #{c.suggestionId} · {c.versionHash.slice(0, 12)}</span>
                </div>
                <div style={sx({ display: 'flex', gap: 7, alignItems: 'center' })}>
                  <select
                    class="select"
                    style={sx({ width: 'auto', padding: '5px 9px', fontSize: 12 })}
                    value={c.percent}
                    disabled={busyId() === c.id || c.status !== 'active'}
                    onChange={(e) => void scale(c.id, +e.currentTarget.value)}
                  >
                    <For each={[5, 10, 25, 50, 100]}>{(p) => <option value={p}>{p}%</option>}</For>
                  </select>
                  <Btn size="sm" variant="success" disabled={busyId() === c.id || c.status !== 'active'} onClick={() => void promote(c)}>全量发布</Btn>
                  <Btn size="sm" variant="danger" disabled={busyId() === c.id || c.status !== 'active'} onClick={() => void rollback(c)}>回滚</Btn>
                </div>
              </div>
              <div style={sx({ display: 'grid', gridTemplateColumns: 'repeat(4,1fr)', gap: 10 })}>
                <MetricBox label="灰度比例" value={`${c.percent}%`} />
                <MetricBox label="实测奖励" value={c.liveReward.toFixed(3)} />
                <MetricBox label="基线奖励" value={c.baselineReward.toFixed(3)} />
                <MetricBox label="异常率" value={`${(c.liveAnomalyRate * 100).toFixed(2)}%`} />
              </div>
              <div style={sx({ marginTop: 10 })}><Progress value={c.percent} tone="accent" height={6} showLabel /></div>
            </Card>
          )}
        </For>
      </div>
    </Show>
  );
}

/* ---------- 白名单 ---------- */
function WhitelistPanel(props: { rows: WhitelistRow[]; onReload: () => void }) {
  const [path, setPath] = createSignal('');
  const [minSafe, setMinSafe] = createSignal(0);
  const [maxSafe, setMaxSafe] = createSignal(1);
  const [busy, setBusy] = createSignal(false);

  const add = async () => {
    if (!path()) {
      toast.warning('请填写参数路径');
      return;
    }
    setBusy(true);
    try {
      await adminApi.amasAddWhitelist({ path: path(), minSafe: minSafe(), maxSafe: maxSafe() });
      toast.success('已添加白名单', path());
      setPath('');
      setMinSafe(0);
      setMaxSafe(1);
      props.onReload();
    } catch (e) {
      toast.error('添加失败', e instanceof Error ? e.message : '');
    } finally {
      setBusy(false);
    }
  };
  const del = async (p: string) => {
    try {
      await adminApi.amasDeleteWhitelist(p);
      toast.info(`已移除 ${p}`);
      props.onReload();
    } catch (e) {
      toast.error('移除失败', e instanceof Error ? e.message : '');
    }
  };

  return (
    <div style={sx({ display: 'grid', gridTemplateColumns: 'minmax(0,1.4fr) minmax(0,1fr)', gap: 16 })} class="grid-collapse">
      <Card pad={false} style={sx({ overflow: 'hidden' })}>
        <Show when={props.rows.length > 0} fallback={<div style={sx({ padding: 28 })}><Empty title="白名单为空" desc="右侧添加参数路径与安全区间。" icon="shield" /></div>}>
          <div style={sx({ overflowX: 'auto' })}>
            <table class="tbl">
              <thead>
                <tr><th>参数路径</th><th>安全下限</th><th>安全上限</th><th style={sx({ textAlign: 'right' })}>操作</th></tr>
              </thead>
              <tbody>
                <For each={props.rows}>
                  {(w) => (
                    <tr>
                      <td class="mono" style={sx({ fontSize: 12 })}>{w.path}</td>
                      <td class="mono">{w.minSafe}</td>
                      <td class="mono">{w.maxSafe}</td>
                      <td style={sx({ textAlign: 'right' })}><IconBtn name="trash" title="移除" onClick={() => void del(w.path)} /></td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          </div>
        </Show>
      </Card>
      <Panel title="添加白名单" sub="LLM 仅可在区间内调参">
        <div style={sx({ display: 'flex', flexDirection: 'column', gap: 12 })}>
          <Field label="参数路径">
            <input class="input mono" value={path()} onInput={(e) => setPath(e.currentTarget.value)} placeholder="cold_start.epsilon" />
          </Field>
          <div style={sx({ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 9 })}>
            <Field label="安全下限">
              <input class="input" type="number" step="0.01" value={minSafe()} onInput={(e) => setMinSafe(+e.currentTarget.value)} />
            </Field>
            <Field label="安全上限">
              <input class="input" type="number" step="0.01" value={maxSafe()} onInput={(e) => setMaxSafe(+e.currentTarget.value)} />
            </Field>
          </div>
          <Btn variant="primary" icon="plus" disabled={busy()} onClick={() => void add()}>添加</Btn>
        </div>
      </Panel>
    </div>
  );
}

/* ---------- 顾问配置 ---------- */
function ConfigRow(props: { label: string; desc?: string; children: JSX.Element }) {
  return (
    <div style={sx({ display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: 12 })}>
      <div>
        <div style={sx({ fontWeight: 600, fontSize: 13 })}>{props.label}</div>
        <Show when={props.desc}><div class="muted-3" style={sx({ fontSize: 11.5, marginTop: 2 })}>{props.desc}</div></Show>
      </div>
      {props.children}
    </div>
  );
}

function AdvisorConfigPanel(props: { cfg: Accessor<AdvisorConfig>; onReload: () => void }) {
  const initial = props.cfg();
  const [advisorEnabled, setAdvisorEnabled] = createSignal(initial.advisorEnabled);
  const [monthCapYuan, setMonthCapYuan] = createSignal(initial.monthCapYuan);
  const [autoApplyEnabled, setAutoApplyEnabled] = createSignal(initial.autoApplyEnabled);
  const [autoApplyMaxPerDay, setAutoApplyMaxPerDay] = createSignal(initial.autoApplyMaxPerDay);
  const [autoApplyMinConfidence, setAutoApplyMinConfidence] = createSignal(initial.autoApplyMinConfidence);
  const [busy, setBusy] = createSignal(false);

  const dirty = createMemo(() => {
    const c = props.cfg();
    return (
      advisorEnabled() !== c.advisorEnabled ||
      monthCapYuan() !== c.monthCapYuan ||
      autoApplyEnabled() !== c.autoApplyEnabled ||
      autoApplyMaxPerDay() !== c.autoApplyMaxPerDay ||
      autoApplyMinConfidence() !== c.autoApplyMinConfidence
    );
  });

  const save = async () => {
    setBusy(true);
    try {
      await adminApi.amasUpdateAdvisorConfig({
        monthCapYuan: monthCapYuan(),
        autoApplyEnabled: autoApplyEnabled(),
        autoApplyMaxPerDay: autoApplyMaxPerDay(),
        autoApplyMinConfidence: autoApplyMinConfidence(),
        advisorEnabled: advisorEnabled(),
      });
      toast.success('顾问配置已保存');
      props.onReload();
    } catch (e) {
      toast.error('保存失败', e instanceof Error ? e.message : '');
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card style={sx({ maxWidth: 640 })}>
      <div style={sx({ display: 'flex', flexDirection: 'column', gap: 16 })}>
        <ConfigRow label="启用顾问" desc="关闭后不再周期性巡查">
          <Switch checked={advisorEnabled()} onChange={setAdvisorEnabled} />
        </ConfigRow>
        <ConfigRow label="模型" desc="只读"><span class="mono muted">{props.cfg().model}</span></ConfigRow>
        <ConfigRow label="巡查周期" desc="cron"><span class="mono muted">{props.cfg().pollCron}</span></ConfigRow>
        <ConfigRow label="API Key" desc="掩码"><span class="mono muted">{props.cfg().apiKeyTail}</span></ConfigRow>
        <Field label={`月度成本上限 ¥${monthCapYuan()}`}>
          <input type="range" min="100" max="2000" step="50" value={monthCapYuan()} onInput={(e) => setMonthCapYuan(+e.currentTarget.value)} style={sx({ width: '100%', accentColor: 'var(--accent)' })} />
        </Field>
        <div class="hr" />
        <ConfigRow label="自动应用" desc="高置信度建议自动进灰度">
          <Switch checked={autoApplyEnabled()} onChange={setAutoApplyEnabled} />
        </ConfigRow>
        <Show when={autoApplyEnabled()}>
          <div class="fade-in" style={sx({ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12 })}>
            <Field label="每日上限">
              <input class="input" type="number" value={autoApplyMaxPerDay()} onInput={(e) => setAutoApplyMaxPerDay(+e.currentTarget.value)} />
            </Field>
            <Field label={`最低置信度 ${(autoApplyMinConfidence() * 100).toFixed(0)}%`}>
              <input type="range" min="0.5" max="1" step="0.01" value={autoApplyMinConfidence()} onInput={(e) => setAutoApplyMinConfidence(+e.currentTarget.value)} style={sx({ width: '100%', accentColor: 'var(--accent)' })} />
            </Field>
          </div>
        </Show>
        <Btn variant="primary" disabled={!dirty() || busy()} onClick={() => void save()}>{busy() ? '保存中…' : '保存配置'}</Btn>
      </div>
    </Card>
  );
}
