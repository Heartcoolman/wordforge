import { createResource, createSignal, createMemo, For, Show } from 'solid-js';
import { PageHead, Panel, Field, Btn, Badge, Empty, Loading, Confirm, sx, toast } from '@/components/wf';
import { adminApi } from '@/api/admin';
import type {
  AmasExperiment,
  AmasMetricComparison,
  AmasMetricPoint,
  AmasExperimentPlan,
  RegisterExperimentPayload,
} from '@/api/admin';

/* ============================================================
   AMAS — 真实留存 A/B 实验（T1.3 验证闭环）
   注册实验 / 样本量规划 / 两桶指标对比 + CI + 采纳门
   纪律：离线赢 ≠ 真实赢（墨墨 MMX-6 铁律）
   ============================================================ */

const PRIMARY_OPTS = [
  { value: 'day7_retention', label: 'Day-7 留存' },
  { value: 'day30_retention', label: 'Day-30 留存' },
  { value: 'session_completion', label: '会话完成率' },
  { value: 'mastered_hold', label: '长期掌握保持' },
  { value: 'reviews_per_day', label: '复习量/活跃日' },
];
const METRIC_LABELS: Record<string, string> = {
  day7_retention: 'Day-7 留存',
  day30_retention: 'Day-30 留存',
  session_completion: '会话完成率',
  mastered_hold: '长期掌握保持(Day-30)',
  reviews_per_day: '复习量/活跃日',
};
const STATUS_META: Record<string, { label: string; variant: 'success' | 'info' | 'error' | 'warning' }> = {
  running: { label: '进行中', variant: 'info' },
  concluded_adopt: { label: '已采纳', variant: 'success' },
  concluded_reject: { label: '已否决', variant: 'error' },
};

const pct = (v: number) => `${(v * 100).toFixed(1)}%`;
const fmtPoint = (p: AmasMetricPoint, kind: string) =>
  kind === 'proportion'
    ? `${pct(p.value)}  [${pct(p.ciLow)}, ${pct(p.ciHigh)}]`
    : `${p.value.toFixed(2)}  [${p.ciLow.toFixed(2)}, ${p.ciHigh.toFixed(2)}]`;
const fmtDiff = (m: AmasMetricComparison) =>
  m.kind === 'proportion'
    ? `${m.diff >= 0 ? '+' : ''}${(m.diff * 100).toFixed(1)} pt`
    : `${m.diff >= 0 ? '+' : ''}${m.diff.toFixed(2)}`;
const fmtTime = (iso: string | null) => {
  if (!iso) return '—';
  try {
    return new Date(iso).toLocaleString('zh-CN', { hour12: false });
  } catch {
    return iso;
  }
};

export default function AmasExperimentsPage() {
  const [experiments, { refetch: refetchList }] = createResource(() => adminApi.amasListExperiments());
  const [selected, setSelected] = createSignal<string>('');
  const [metrics, { refetch: refetchMetrics }] = createResource(
    selected,
    (id) => (id ? adminApi.amasExperimentMetrics(id) : null),
  );

  // ── 注册表单 ──
  const [form, setForm] = createSignal<RegisterExperimentPayload>({
    canaryVersionHash: '',
    baselineVersionHash: '',
    canaryCohortLo: 0,
    canaryCohortHi: 50,
    primaryMetric: 'day7_retention',
    minSample: 1000,
    alpha: 0.05,
    power: 0.8,
    mde: 0.05,
    notes: '',
  });
  const upd = <K extends keyof RegisterExperimentPayload>(k: K, v: RegisterExperimentPayload[K]) =>
    setForm({ ...form(), [k]: v });
  const [registering, setRegistering] = createSignal(false);

  const submit = async () => {
    const f = form();
    if (!f.canaryVersionHash || !f.baselineVersionHash) {
      toast.warning('请填写 canary / baseline 版本哈希');
      return;
    }
    setRegistering(true);
    try {
      const exp = await adminApi.amasRegisterExperiment(f);
      toast.success('实验已注册', exp.experimentId.slice(0, 12));
      await refetchList();
      setSelected(exp.experimentId);
    } catch (e) {
      toast.error('注册失败', e instanceof Error ? e.message : '');
    } finally {
      setRegistering(false);
    }
  };

  // ── 样本量规划（决策4：后端按 power/alpha/MDE 反推） ──
  const [planP0, setPlanP0] = createSignal(0.4);
  const [planMde, setPlanMde] = createSignal(0.05);
  const [planDaily, setPlanDaily] = createSignal(200);
  const [plan, setPlan] = createSignal<AmasExperimentPlan | null>(null);
  const [planning, setPlanning] = createSignal(false);
  const computePlan = async () => {
    setPlanning(true);
    try {
      const p = await adminApi.amasPlanExperiment({
        kind: 'proportion',
        p0: planP0(),
        mdeRel: planMde(),
        alpha: form().alpha,
        power: form().power,
        dailySignups: planDaily(),
      });
      setPlan(p);
    } catch (e) {
      toast.error('规划失败', e instanceof Error ? e.message : '');
    } finally {
      setPlanning(false);
    }
  };
  const applyPlan = () => {
    const p = plan();
    if (p) {
      setForm({ ...form(), minSample: p.minSamplePerArm, mde: planMde() });
      toast.success('已填入 minSample', `每桶 ${p.minSamplePerArm}`);
    }
  };

  // ── 结束实验 ──
  const [confirmAdopt, setConfirmAdopt] = createSignal<boolean | null>(null);
  const [concluding, setConcluding] = createSignal(false);
  const doConclude = async () => {
    const adopt = confirmAdopt();
    const id = selected();
    if (adopt === null || !id) return;
    setConcluding(true);
    try {
      await adminApi.amasConcludeExperiment(id, adopt);
      toast.success(adopt ? '已采纳实验' : '已否决实验');
      setConfirmAdopt(null);
      await refetchList();
      await refetchMetrics();
    } catch (e) {
      toast.error('操作失败', e instanceof Error ? e.message : '');
    } finally {
      setConcluding(false);
    }
  };

  const running = createMemo(() => (experiments() ?? []).some((e) => e.status === 'running'));

  return (
    <div class="fade-up">
      <PageHead
        title="AMAS A/B 实验"
        eyebrow="T1.3 · 真实留存验证闭环"
        desc="按 canary 桶 × baseline 桶对比真实留存/参与度。纪律：离线赢 ≠ 真实赢——primary 显著为正且达 MDE、无 guardrail 恶化才建议采纳。"
      />

      <div style={sx({ display: 'grid', gridTemplateColumns: 'minmax(0,360px) minmax(0,1fr)', gap: 16, alignItems: 'start' })}>
        {/* ── 左栏：注册 + 规划 + 列表 ── */}
        <div style={sx({ display: 'grid', gap: 16 })}>
          <Panel title="注册新实验" sub="单 active 实验模型">
            <Show
              when={!running()}
              fallback={
                <div class="muted" style={sx({ fontSize: 13 })}>
                  已有进行中实验，先结束它再注册新实验。
                </div>
              }
            >
              <div style={sx({ display: 'grid', gap: 10 })}>
                <Field label="canary 版本哈希">
                  <input class="input mono" value={form().canaryVersionHash}
                    onInput={(e) => upd('canaryVersionHash', e.currentTarget.value.trim())}
                    placeholder="16-hex；与 baseline 相同 = A/A 自检" />
                </Field>
                <Field label="baseline 版本哈希">
                  <input class="input mono" value={form().baselineVersionHash}
                    onInput={(e) => upd('baselineVersionHash', e.currentTarget.value.trim())}
                    placeholder="stable 版本哈希" />
                </Field>
                <div style={sx({ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 10 })}>
                  <Field label="canary cohort 下界">
                    <input class="input" type="number" min="0" max="100" value={form().canaryCohortLo}
                      onInput={(e) => upd('canaryCohortLo', Number(e.currentTarget.value))} />
                  </Field>
                  <Field label="canary cohort 上界">
                    <input class="input" type="number" min="0" max="100" value={form().canaryCohortHi}
                      onInput={(e) => upd('canaryCohortHi', Number(e.currentTarget.value))} />
                  </Field>
                </div>
                <Field label="primary 指标" hint="主指标显著为正才算赢；其余为 guardrail">
                  <select class="input" value={form().primaryMetric}
                    onChange={(e) => upd('primaryMetric', e.currentTarget.value)}>
                    <For each={PRIMARY_OPTS}>{(o) => <option value={o.value}>{o.label}</option>}</For>
                  </select>
                </Field>
                <div style={sx({ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 10 })}>
                  <Field label="最小样本/桶">
                    <input class="input" type="number" min="1" value={form().minSample}
                      onInput={(e) => upd('minSample', Number(e.currentTarget.value))} />
                  </Field>
                  <Field label="MDE(相对)">
                    <input class="input" type="number" step="0.01" min="0" value={form().mde}
                      onInput={(e) => upd('mde', Number(e.currentTarget.value))} />
                  </Field>
                  <Field label="alpha">
                    <input class="input" type="number" step="0.01" min="0" max="1" value={form().alpha}
                      onInput={(e) => upd('alpha', Number(e.currentTarget.value))} />
                  </Field>
                  <Field label="power">
                    <input class="input" type="number" step="0.05" min="0" max="1" value={form().power}
                      onInput={(e) => upd('power', Number(e.currentTarget.value))} />
                  </Field>
                </div>
                <Field label="备注">
                  <input class="input" value={form().notes ?? ''} onInput={(e) => upd('notes', e.currentTarget.value)} />
                </Field>
                <Btn variant="primary" disabled={registering()} onClick={submit}>
                  {registering() ? '注册中…' : '注册实验'}
                </Btn>
              </div>
            </Show>
          </Panel>

          <Panel title="样本量规划" sub="power/alpha/MDE 反推每桶最小样本">
            <div style={sx({ display: 'grid', gap: 10 })}>
              <div style={sx({ display: 'grid', gridTemplateColumns: '1fr 1fr 1fr', gap: 10 })}>
                <Field label="基线 p0">
                  <input class="input" type="number" step="0.01" min="0" max="1" value={planP0()}
                    onInput={(e) => setPlanP0(Number(e.currentTarget.value))} />
                </Field>
                <Field label="相对 MDE">
                  <input class="input" type="number" step="0.01" min="0" value={planMde()}
                    onInput={(e) => setPlanMde(Number(e.currentTarget.value))} />
                </Field>
                <Field label="日进桶量">
                  <input class="input" type="number" min="0" value={planDaily()}
                    onInput={(e) => setPlanDaily(Number(e.currentTarget.value))} />
                </Field>
              </div>
              <div style={sx({ display: 'flex', gap: 8 })}>
                <Btn size="sm" disabled={planning()} onClick={computePlan}>{planning() ? '计算中…' : '计算样本量'}</Btn>
                <Show when={plan()}><Btn size="sm" variant="ghost" onClick={applyPlan}>填入 minSample</Btn></Show>
              </div>
              <Show when={plan()}>
                {(p) => (
                  <div class="muted" style={sx({ fontSize: 12.5, lineHeight: 1.7 })}>
                    每桶最小样本：<b class="tnum">{p().minSamplePerArm.toLocaleString()}</b>
                    合计：<b class="tnum">{p().totalRequired.toLocaleString()}</b><br />
                    预计运行：<b class="tnum">{p().estimatedDays ?? '—'}</b> 天　推荐 canary 占比：<b>{p().recommendedPercent}%</b>
                    <div class="muted-3" style={sx({ fontSize: 11.5, marginTop: 4 })}>提示：5% 对 Day-30 低频留存大概率样本不足。</div>
                  </div>
                )}
              </Show>
            </div>
          </Panel>

          <Panel title="实验列表">
            <Show when={!experiments.loading} fallback={<Loading />}>
              <Show when={(experiments() ?? []).length > 0} fallback={<Empty title="暂无实验" desc="注册一个真实留存 A/B 实验" />}>
                <div style={sx({ display: 'grid', gap: 8 })}>
                  <For each={experiments()}>
                    {(exp: AmasExperiment) => (
                      <button
                        class="card card-pad"
                        style={sx({
                          textAlign: 'left', cursor: 'pointer', border: selected() === exp.experimentId ? '1px solid var(--accent)' : '1px solid var(--hairline)',
                          display: 'grid', gap: 4,
                        })}
                        onClick={() => setSelected(exp.experimentId)}
                      >
                        <div style={sx({ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 8 })}>
                          <span class="mono" style={sx({ fontSize: 12.5, fontWeight: 600 })}>{exp.experimentId.slice(0, 12)}</span>
                          <Badge variant={STATUS_META[exp.status]?.variant ?? 'default'}>{STATUS_META[exp.status]?.label ?? exp.status}</Badge>
                        </div>
                        <div class="muted-3" style={sx({ fontSize: 11.5 })}>
                          {METRIC_LABELS[exp.primaryMetric] ?? exp.primaryMetric} · cohort [{exp.canaryCohortLo},{exp.canaryCohortHi}) · {fmtTime(exp.registeredAt)}
                        </div>
                      </button>
                    )}
                  </For>
                </div>
              </Show>
            </Show>
          </Panel>
        </div>

        {/* ── 右栏：两桶指标对比 + 采纳门 ── */}
        <Panel
          title="两桶指标对比"
          sub={selected() ? selected().slice(0, 16) : '选择左侧实验'}
          right={
            <Show when={metrics()?.experiment.status === 'running'}>
              <div style={sx({ display: 'flex', gap: 8 })}>
                <Btn size="sm" variant="success" onClick={() => setConfirmAdopt(true)}>采纳</Btn>
                <Btn size="sm" variant="danger" onClick={() => setConfirmAdopt(false)}>否决</Btn>
              </div>
            </Show>
          }
        >
          <Show when={selected()} fallback={<Empty title="未选择实验" desc="从左侧列表选择一个实验查看真实指标" />}>
            <Show when={!metrics.loading && metrics()} fallback={<Loading h={240} />}>
              {(m) => {
                const v = m().verdict;
                return (
                  <div style={sx({ display: 'grid', gap: 14 })}>
                    {/* 采纳门横幅 */}
                    <div
                      class="card card-pad"
                      style={sx({
                        display: 'grid', gap: 6,
                        borderLeft: `3px solid ${v.adoptRecommended ? 'var(--success)' : 'var(--warning)'}`,
                      })}
                    >
                      <div style={sx({ display: 'flex', alignItems: 'center', gap: 8 })}>
                        <Badge variant={v.adoptRecommended ? 'success' : 'warning'}>
                          {v.adoptRecommended ? '建议采纳' : '暂不采纳'}
                        </Badge>
                        <span class="muted" style={sx({ fontSize: 12.5 })}>primary：{METRIC_LABELS[v.primaryMetric] ?? v.primaryMetric}</span>
                      </div>
                      <div class="muted" style={sx({ fontSize: 12.5 })}>{v.reason}</div>
                      <div style={sx({ display: 'flex', flexWrap: 'wrap', gap: 6, marginTop: 2 })}>
                        <GateChip ok={v.sampleOk} label={`样本量(≥${v.minSample})`} />
                        <GateChip ok={v.primarySignificant} label={`显著(α=${v.alpha})`} />
                        <GateChip ok={v.primaryPositive} label="方向为正" />
                        <GateChip ok={v.primaryMeetsMde} label={`达 MDE(${v.mde})`} />
                        <GateChip ok={v.guardrailRegressions.length === 0} label="无 guardrail 恶化" />
                      </div>
                      <Show when={v.guardrailRegressions.length > 0}>
                        <div style={sx({ fontSize: 12, color: 'var(--error)' })}>恶化 guardrail：{v.guardrailRegressions.map((g) => METRIC_LABELS[g] ?? g).join('、')}</div>
                      </Show>
                    </div>

                    {/* 入组规模 */}
                    <div class="muted-3" style={sx({ fontSize: 11.5 })}>
                      入组：canary <b class="tnum">{m().raw.canary.enrolled.toLocaleString()}</b> · baseline <b class="tnum">{m().raw.baseline.enrolled.toLocaleString()}</b>
                    </div>

                    {/* 对比表 */}
                    <div style={sx({ overflowX: 'auto' })}>
                      <table class="wf-table" style={sx({ width: '100%', fontSize: 12.5, borderCollapse: 'collapse' })}>
                        <thead>
                          <tr style={sx({ textAlign: 'left', color: 'var(--text-2)' })}>
                            <th style={cellHead}>指标</th>
                            <th style={cellHead}>canary [95% CI]</th>
                            <th style={cellHead}>baseline [95% CI]</th>
                            <th style={cellHead}>Δ</th>
                            <th style={cellHead}>p</th>
                            <th style={cellHead}>显著</th>
                          </tr>
                        </thead>
                        <tbody>
                          <For each={v.metrics}>
                            {(mc) => {
                              const good = mc.significant && !mc.regressed;
                              const isPrimary = mc.metric === v.primaryMetric;
                              return (
                                <tr style={sx({ borderTop: '1px solid var(--hairline)', background: isPrimary ? 'color-mix(in oklch, var(--accent) 6%, transparent)' : 'transparent' })}>
                                  <td style={cell}>
                                    {METRIC_LABELS[mc.metric] ?? mc.metric}
                                    <Show when={isPrimary}><span class="muted-3" style={sx({ marginLeft: 6, fontSize: 10.5 })}>primary</span></Show>
                                    <Show when={!mc.higherIsBetter}><span class="muted-3" style={sx({ marginLeft: 6, fontSize: 10.5 })}>越小越好</span></Show>
                                  </td>
                                  <td style={{ ...cell, ...mono }}>{fmtPoint(mc.canary, mc.kind)} <span class="muted-3">(n={mc.canary.n})</span></td>
                                  <td style={{ ...cell, ...mono }}>{fmtPoint(mc.baseline, mc.kind)} <span class="muted-3">(n={mc.baseline.n})</span></td>
                                  <td style={{ ...cell, ...mono, color: mc.regressed ? 'var(--error)' : good ? 'var(--success)' : 'var(--text-1)' }}>{fmtDiff(mc)}</td>
                                  <td style={{ ...cell, ...mono }}>{mc.pValue == null ? '—' : mc.pValue.toFixed(3)}</td>
                                  <td style={cell}>
                                    <Show when={mc.significant} fallback={<span class="muted-3">—</span>}>
                                      <Badge variant={mc.regressed ? 'error' : 'success'}>{mc.regressed ? '恶化' : '显著'}</Badge>
                                    </Show>
                                  </td>
                                </tr>
                              );
                            }}
                          </For>
                        </tbody>
                      </table>
                    </div>
                    <div class="muted-3" style={sx({ fontSize: 11 })}>
                      留存/完成率/掌握保持为比例(Wilson CI + 两比例 z 检验)；复习量为均值(Welch t)。留存走全量业务表，不受 5% 采样影响。
                    </div>
                  </div>
                );
              }}
            </Show>
          </Show>
        </Panel>
      </div>

      <Confirm
        open={confirmAdopt() !== null}
        onClose={() => setConfirmAdopt(null)}
        onConfirm={doConclude}
        loading={concluding()}
        danger={confirmAdopt() === false}
        title={confirmAdopt() ? '采纳实验' : '否决实验'}
        confirmText={confirmAdopt() ? '采纳' : '否决'}
        body={
          <div class="muted" style={sx({ fontSize: 13 })}>
            {confirmAdopt()
              ? '记录为采纳并停止入组。注意：配置全量上线仍需走 canary promote 流程。'
              : '记录为否决并停止入组。'}
          </div>
        }
      />
    </div>
  );
}

const cellHead = sx({ padding: '6px 10px', fontWeight: 600, whiteSpace: 'nowrap' });
const cell = sx({ padding: '7px 10px', verticalAlign: 'top' });
const mono = { 'font-variant-numeric': 'tabular-nums' } as const;

function GateChip(props: { ok: boolean; label: string }) {
  return (
    <span
      style={sx({
        display: 'inline-flex', alignItems: 'center', gap: 4, fontSize: 11, padding: '2px 8px', borderRadius: 999,
        background: props.ok ? 'color-mix(in oklch, var(--success) 14%, transparent)' : 'color-mix(in oklch, var(--warning) 14%, transparent)',
        color: props.ok ? 'var(--success)' : 'var(--warning)',
      })}
    >
      {props.ok ? '✓' : '✕'} {props.label}
    </span>
  );
}
