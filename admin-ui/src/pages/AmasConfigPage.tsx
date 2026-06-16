import {
  createSignal,
  createMemo,
  createResource,
  Show,
  For,
  onMount,
} from 'solid-js';
import {
  Btn,
  IconBtn,
  Badge,
  Card,
  Field,
  Switch,
  Tabs,
  Empty,
  Loading,
  Drawer,
  Confirm,
  Panel,
  Progress,
  PageHead,
  Icon,
  sx,
  toast,
} from '@/components/wf';
import { amasApi } from '@/api/amas';
import { adminApi } from '@/api/admin';
import type {
  AmasConfigVersionRow,
  AmasCanaryConfig,
  DiffImpactResponse,
  AmasExplainResponse,
  SetCanaryExtRequest,
  CanaryAudience,
} from '@/api/admin';
import type { AmasConfig } from '@/types/amas';

/* ============================================================
   AMAS 调参 — 分节参数编辑 + TOML 视图 + diff-impact + 灰度 + 版本历史
   字段名对齐后端事实源（src/amas/config.rs → AMASConfig）。
   ============================================================ */

const ALGO_COLORS = ['#4f7dfb', '#18a558', '#d99411', '#8b5cf6', '#ec4899', '#06b6d4', '#f97316', '#64748b'];

/* ---------------------------- 参数元数据 ---------------------------- */
type ParamMeta = {
  key: string;
  label: string;
  min: number;
  max: number;
  step: number;
  fmt: (v: number) => string;
  what: string;
  high: string;
  low: string;
};
type GroupMeta = {
  id: keyof AmasConfig;
  label: string;
  icon: string;
  tone: string;
  desc: string;
  params: ParamMeta[];
};

const CFG_GROUPS: GroupMeta[] = [
  {
    id: 'coldStart',
    label: '新手探索',
    icon: 'zap',
    tone: 'var(--accent)',
    desc: '新用户/新词初期，系统从"摸清画像"到"探索"再到"稳定推荐"的节奏。',
    params: [
      { key: 'classifyToExploreEvents', label: '画像所需事件数', min: 1, max: 20, step: 1, fmt: (v) => v + ' 次', what: '答多少题后完成初步学习者画像、进入探索期。', high: '画像更准，但起步更慢', low: '更快进入探索，画像可能不够准' },
      { key: 'exploreToExploitEvents', label: '稳定所需事件数', min: 5, max: 60, step: 1, fmt: (v) => v + ' 次', what: '再答多少题后退出探索、进入稳定个性化推荐。', high: '更久才定型，更稳但慢', low: '更快个性化，可能波动' },
      { key: 'classifyToExploreConfidence', label: '画像置信门槛', min: 0.3, max: 0.9, step: 0.05, fmt: (v) => v.toFixed(2), what: '画像置信度达到多少才进入探索期。', high: '更保守，要求更高把握', low: '更早进入探索' },
    ],
  },
  {
    id: 'classifier',
    label: '学习者分型',
    icon: 'users',
    tone: 'var(--info)',
    desc: '判定"快学型 / 稳健型"用户的阈值，影响后续策略。',
    params: [
      { key: 'fastLearnerThreshold', label: '快学型门槛', min: 0.5, max: 0.95, step: 0.01, fmt: (v) => v.toFixed(2), what: '综合表现高于此值判为快学型。', high: '更难被判为快学型', low: '更多人被判为快学型' },
      { key: 'stableLearnerThreshold', label: '稳健型门槛', min: 0.2, max: 0.7, step: 0.01, fmt: (v) => v.toFixed(2), what: '稳定度高于此值判为稳健型。', high: '判定更严格', low: '更多人判为稳健型' },
    ],
  },
  {
    id: 'constraints',
    label: '疲劳保护',
    icon: 'shield',
    tone: 'var(--warning)',
    desc: '识别疲劳并给学习量"封顶减负"，避免硬刷导致放弃。',
    params: [
      { key: 'highFatigueThreshold', label: '高疲劳阈值', min: 0.4, max: 0.9, step: 0.01, fmt: (v) => v.toFixed(2), what: '疲劳值超过此线触发减负（越低越敏感）。', high: '更晚减负，学习量更大但更累', low: '更早减负，体验更轻松' },
      { key: 'maxBatchSizeWhenFatigued', label: '疲劳时每批题数', min: 3, max: 30, step: 1, fmt: (v) => v + ' 题', what: '疲劳时单批最多出多少题。', high: '疲劳时仍出较多题', low: '疲劳时大幅减量' },
      { key: 'maxDifficultyWhenFatigued', label: '疲劳时最高难度', min: 1, max: 10, step: 1, fmt: (v) => String(v), what: '疲劳时允许的最高题目难度（1-10）。', high: '疲劳时仍可出难题', low: '疲劳时只出简单题' },
      { key: 'maxNewRatioWhenFatigued', label: '疲劳时新词占比', min: 0, max: 0.8, step: 0.05, fmt: (v) => Math.round(v * 100) + '%', what: '疲劳时新词最多占多少（其余为复习）。', high: '疲劳时仍学较多新词', low: '疲劳时以复习为主' },
    ],
  },
  {
    id: 'intervention',
    label: '状态预警',
    icon: 'alert',
    tone: 'var(--error)',
    desc: '注意力 / 疲劳 / 动机越界时触发干预提醒的阈值。',
    params: [
      { key: 'fatigueAlertThreshold', label: '疲劳预警阈值', min: 0.4, max: 0.95, step: 0.01, fmt: (v) => v.toFixed(2), what: '疲劳超过此值触发预警。', high: '更晚预警', low: '更早预警' },
      { key: 'attentionAlertThreshold', label: '注意力预警阈值', min: 0.1, max: 0.6, step: 0.01, fmt: (v) => v.toFixed(2), what: '注意力低于此值触发预警。', high: '更容易预警', low: '更少预警' },
      { key: 'motivationAlertThreshold', label: '动机预警阈值', min: -0.6, max: 0, step: 0.05, fmt: (v) => v.toFixed(2), what: '动机低于此值（负向）触发预警。', high: '更容易预警', low: '更少预警' },
    ],
  },
  {
    id: 'elo',
    label: '水平评分 (ELO)',
    icon: 'chart',
    tone: 'var(--accent)',
    desc: '像棋类 ELO 一样给用户和单词打分，用于 ZPD 难度匹配。',
    params: [
      { key: 'defaultElo', label: '初始评分', min: 800, max: 1600, step: 50, fmt: (v) => String(v), what: '新用户/新词起始分。', high: '从更难处起步', low: '从更简单处起步' },
      { key: 'kFactor', label: '评分灵敏度 K', min: 8, max: 48, step: 2, fmt: (v) => String(v), what: '每次答题后评分变化幅度。', high: '适配灵敏但易波动', low: '更稳但更慢' },
      { key: 'noviceKMultiplier', label: '新手 K 倍率', min: 1, max: 4, step: 0.5, fmt: (v) => v.toFixed(1) + '×', what: '新手期 K 值放大倍数，加速早期收敛。', high: '新手收敛更快', low: '新手更平稳' },
      { key: 'zpdOptimalOffset', label: 'ZPD 最优偏移', min: -200, max: 200, step: 10, fmt: (v) => String(v), what: '相对用户水平的最优挑战偏移（正=略难）。', high: '倾向更有挑战', low: '倾向更易' },
      { key: 'zpdGaussianSigma', label: 'ZPD 容差', min: 50, max: 400, step: 10, fmt: (v) => String(v), what: '难度匹配的高斯容差（越大越宽松）。', high: '难度跨度更宽', low: '难度更聚焦' },
    ],
  },
];

// 真实 ensemble：启发式 / IGE / SWD 三策略加权混合
const ENSEMBLE_ALGOS = [
  { key: 'baseWeightHeuristic', label: '启发式', desc: '规则启发式策略' },
  { key: 'baseWeightIge', label: 'IGE', desc: '信息增益探索' },
  { key: 'baseWeightSwd', label: 'SWD', desc: '间隔加权衰减' },
] as const;
const ENSEMBLE_EXTRA: ParamMeta[] = [
  { key: 'minWeight', label: '最小权重下限', min: 0, max: 0.3, step: 0.01, fmt: (v) => v.toFixed(2), what: '任一策略权重不低于此值，避免被完全压制。', high: '各策略更均衡', low: '允许某策略占绝对主导' },
  { key: 'warmupSamples', label: '预热样本数', min: 0, max: 200, step: 10, fmt: (v) => v + ' 条', what: '集成生效前的预热样本量。', high: '预热更久更稳', low: '更快启用集成' },
];
const FLAG_META = [
  { key: 'ensembleEnabled', label: '策略集成', desc: '多策略加权混合（关闭则单策略）' },
  { key: 'heuristicEnabled', label: '启发式策略', desc: '规则启发式' },
  { key: 'igeEnabled', label: 'IGE 探索', desc: '信息增益探索' },
  { key: 'swdEnabled', label: 'SWD 调度', desc: '间隔加权衰减' },
  { key: 'mdmEnabled', label: 'MDM 记忆模型', desc: 'FSRS 记忆/遗忘建模' },
  { key: 'iadEnabled', label: 'IAD 干扰衰减', desc: '混淆词对干扰（B38）' },
  { key: 'mtpEnabled', label: 'MTP 词素迁移', desc: '词素迁移预测（B37）' },
  { key: 'sspEnabled', label: 'SSP-MMC 调度', desc: '最优间隔 DP 调度' },
] as const;

/* ---------------------------- 一键预设 ----------------------------
   patch 为 'section.key' → 目标值；null 表示「就用服务端当前 baseline」。
   预设基线 = baseline()（服务端当前配置），与本页「baseline = 线上配置」语义一致。 */
type PresetMeta = { id: string; label: string; desc: string; patch: Record<string, number> | null };
const CFG_PRESETS: PresetMeta[] = [
  { id: 'conservative', label: '稳妥', desc: '体验平滑、波动小，适合新上线或低龄用户。', patch: { 'coldStart.exploreToExploitEvents': 30, 'elo.kFactor': 16, 'constraints.highFatigueThreshold': 0.55 } },
  { id: 'balanced', label: '平衡（推荐）', desc: '保持服务端当前配置，兼顾适配速度与稳定性。', patch: null },
  { id: 'aggressive', label: '激进', desc: '快速摸清水平、追求学习效率，前期略有波动。', patch: { 'coldStart.exploreToExploitEvents': 12, 'elo.kFactor': 32, 'constraints.highFatigueThreshold': 0.75 } },
];

/** 基于 baseline() 构造预设目标 config（不可变写入 patch 路径） */
function buildPresetConfig(base: AmasConfig, patch: PresetMeta['patch']): AmasConfig {
  const next = clone(base) as unknown as Record<string, Record<string, unknown>>;
  if (patch) {
    for (const [path, v] of Object.entries(patch)) {
      const [s, k] = path.split('.');
      next[s] = { ...(next[s] ?? {}), [k]: v };
    }
  }
  return next as unknown as AmasConfig;
}

/* metric 中文名映射（diff-impact） */
const METRIC_LABEL: Record<string, string> = { accuracy: '正确率', fatigue: '疲劳', d7Retention: '7日留存' };

const clone = <T,>(o: T): T => structuredClone(o);

/** 安全读取 cfg[section][key] 为 number；缺失返回 fallback */
function readNum(cfg: AmasConfig, section: string, key: string, fallback = 0): number {
  const sec = (cfg as unknown as Record<string, unknown>)[section];
  if (sec && typeof sec === 'object') {
    const v = (sec as Record<string, unknown>)[key];
    if (typeof v === 'number') return v;
  }
  return fallback;
}
function readBool(cfg: AmasConfig, key: string): boolean {
  const flags = (cfg as unknown as Record<string, unknown>).featureFlags;
  if (flags && typeof flags === 'object') {
    return Boolean((flags as Record<string, unknown>)[key]);
  }
  return false;
}

export default function AmasConfigPage() {
  const [config, setConfig] = createSignal<AmasConfig>({} as AmasConfig);
  const [baseline, setBaseline] = createSignal<AmasConfig>({} as AmasConfig);
  const [loading, setLoading] = createSignal(true);
  const [loadError, setLoadError] = createSignal('');
  const [tab, setTab] = createSignal<'visual' | 'editor' | 'versions' | 'canary'>('visual');
  const [note, setNote] = createSignal('');
  const [confirmKind, setConfirmKind] = createSignal<'save' | 'reload' | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [diffImpact, setDiffImpact] = createSignal<DiffImpactResponse | null>(null);
  const [diffLoading, setDiffLoading] = createSignal(false);
  const [drawerOpen, setDrawerOpen] = createSignal(false);

  // 版本历史 / 灰度 用 resource，方便 refetch
  const [versions, { refetch: refetchVersions }] = createResource<AmasConfigVersionRow[]>(() => adminApi.amasListVersions());
  const [canary, { refetch: refetchCanary }] = createResource<AmasCanaryConfig | null>(async () => {
    const r = await adminApi.amasGetCanary();
    return r.canary;
  });

  onMount(async () => {
    try {
      const cfg = await amasApi.getConfig();
      setBaseline(clone(cfg));
      setConfig(clone(cfg));
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : '无法获取 AMAS 配置';
      setLoadError(msg);
      toast.error('加载失败', msg);
    } finally {
      setLoading(false);
    }
  });

  const dirty = createMemo(() => JSON.stringify(config()) !== JSON.stringify(baseline()));

  // ── 参数写入（不可变更新指定 section.key）──
  function setParam(section: string, key: string, val: number | boolean) {
    setConfig((prev) => {
      const next = clone(prev) as unknown as Record<string, Record<string, unknown>>;
      next[section] = { ...(next[section] ?? {}), [key]: val };
      return next as unknown as AmasConfig;
    });
  }

  function resetDefaults() {
    setConfig(clone(baseline()));
    setDiffImpact(null);
    toast.info('已回滚到 baseline', '恢复服务端当前配置');
  }

  // ── 一键预设（基线 = 服务端当前 baseline）──
  function applyPreset(p: PresetMeta) {
    setConfig(buildPresetConfig(baseline(), p.patch));
    setDiffImpact(null);
    toast.info('已套用「' + p.label + '」预设', '记得保存或热重载生效');
  }
  function presetActive(p: PresetMeta): boolean {
    return JSON.stringify(buildPresetConfig(baseline(), p.patch)) === JSON.stringify(config());
  }

  // ── 保存为新版本 ──
  async function doSave() {
    setBusy(true);
    try {
      await adminApi.amasUpdateConfigWithNote(config(), note() || undefined);
      setBaseline(clone(config()));
      toast.success('配置已保存为新版本', note() || '无备注');
      setConfirmKind(null);
      setNote('');
      void refetchVersions();
    } catch (err: unknown) {
      toast.error('保存失败', err instanceof Error ? err.message : '未知错误');
    } finally {
      setBusy(false);
    }
  }

  // ── 热重载（绕过版本化）──
  async function doReload() {
    setBusy(true);
    try {
      const latest = await adminApi.reloadAmas(config());
      setBaseline(clone(latest));
      setConfig(clone(latest));
      toast.success('已热重载', '运行时立即生效');
      setConfirmKind(null);
    } catch (err: unknown) {
      toast.error('热重载失败', err instanceof Error ? err.message : '未知错误');
    } finally {
      setBusy(false);
    }
  }

  // ── diff-impact 评估 ──
  async function checkImpact() {
    setDiffLoading(true);
    try {
      const patch: Record<string, unknown> = {};
      // 仅取已知改动字段（与 baseline 不同）
      for (const g of CFG_GROUPS) {
        for (const p of g.params) {
          const cur = readNum(config(), g.id as string, p.key);
          const base = readNum(baseline(), g.id as string, p.key);
          if (cur !== base) patch[`${g.id}.${p.key}`] = cur;
        }
      }
      // 若无改动，至少送两项关键参数让后端给出区间
      if (Object.keys(patch).length === 0) {
        patch['elo.kFactor'] = readNum(config(), 'elo', 'kFactor', 24);
        patch['constraints.highFatigueThreshold'] = readNum(config(), 'constraints', 'highFatigueThreshold', 0.6);
      }
      const r = await adminApi.amasConfigDiffImpact(patch);
      setDiffImpact(r);
    } catch (err: unknown) {
      toast.error('评估失败', err instanceof Error ? err.message : '未知错误');
    } finally {
      setDiffLoading(false);
    }
  }

  // ── 版本恢复 ──
  async function restoreVersion(hash: string) {
    try {
      await adminApi.amasRestoreVersion(hash, `回滚到 ${hash}`);
      const cfg = await amasApi.getConfig();
      setBaseline(clone(cfg));
      setConfig(clone(cfg));
      setDiffImpact(null);
      toast.success('已回滚到版本 ' + hash);
      void refetchVersions();
    } catch (err: unknown) {
      toast.error('回滚失败', err instanceof Error ? err.message : '未知错误');
    }
  }

  // ensemble 归一化（用于活动配比条）
  const eSum = createMemo(() => {
    const cfg = config();
    const s = ENSEMBLE_ALGOS.reduce((acc, a) => acc + readNum(cfg, 'ensemble', a.key), 0);
    return s || 1;
  });

  // 改动预览列表
  type DiffRow = { group: string; label: string; before: string; after: string };
  const changedRows = createMemo<DiffRow[]>(() => {
    const rows: DiffRow[] = [];
    const cfg = config();
    const base = baseline();
    for (const g of CFG_GROUPS) {
      for (const p of g.params) {
        const cur = readNum(cfg, g.id as string, p.key);
        const b = readNum(base, g.id as string, p.key);
        if (cur !== b) rows.push({ group: g.label, label: p.label, before: p.fmt(b), after: p.fmt(cur) });
      }
    }
    for (const a of ENSEMBLE_ALGOS) {
      const cur = readNum(cfg, 'ensemble', a.key);
      const b = readNum(base, 'ensemble', a.key);
      if (cur !== b) rows.push({ group: '算法配比', label: a.label, before: b.toFixed(2), after: cur.toFixed(2) });
    }
    for (const p of ENSEMBLE_EXTRA) {
      const cur = readNum(cfg, 'ensemble', p.key);
      const b = readNum(base, 'ensemble', p.key);
      if (cur !== b) rows.push({ group: '算法配比', label: p.label, before: p.fmt(b), after: p.fmt(cur) });
    }
    for (const f of FLAG_META) {
      const cur = readBool(cfg, f.key);
      const b = readBool(base, f.key);
      if (cur !== b) rows.push({ group: '功能开关', label: f.label, before: b ? '开' : '关', after: cur ? '开' : '关' });
    }
    return rows;
  });

  return (
    <div>
      <PageHead
        title="AMAS 调参"
        desc="用直观的滑块调节学习算法，无需懂技术细节；每项都有说明，可一键套用推荐预设，保存为可回滚版本。"
        right={
          <>
            <Show when={dirty()}>
              <Badge variant="warning" dot>有未保存修改</Badge>
            </Show>
            <Btn variant="ghost" icon="refresh" disabled={!dirty()} onClick={resetDefaults}>回滚到 baseline</Btn>
            <Btn variant="ghost" onClick={() => setDrawerOpen(true)}>版本历史</Btn>
            <Btn variant="secondary" icon="rotate" disabled={!dirty() || busy()} onClick={() => setConfirmKind('reload')}>热重载</Btn>
            <Btn variant="primary" icon="check" disabled={!dirty() || busy()} onClick={() => setConfirmKind('save')}>保存为新版本</Btn>
          </>
        }
      />

      <Show when={!loading()} fallback={<Loading />}>
        <Show
          when={!loadError()}
          fallback={
            <Card>
              <div style={sx({ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 12, padding: '48px 20px', textAlign: 'center' })}>
                <p style={sx({ margin: 0, fontWeight: 600 })}>加载失败，请重试</p>
                <p class="muted" style={sx({ margin: 0, fontSize: 13 })}>{loadError()}</p>
                <Btn variant="outline" onClick={() => window.location.reload()}>刷新页面</Btn>
              </div>
            </Card>
          }
        >
          <Tabs
            value={tab()}
            onChange={(v) => setTab(v as 'visual' | 'editor' | 'versions' | 'canary')}
            tabs={[
              { value: 'visual', label: '可视化配置' },
              { value: 'editor', label: '高级 (TOML)' },
              { value: 'versions', label: '版本历史', count: versions()?.length ?? null },
              { value: 'canary', label: '灰度发布' },
            ]}
          />

          <div style={sx({ marginTop: 16 })}>
            {/* ---------------- 可视化配置 ---------------- */}
            <Show when={tab() === 'visual'}>
              <div style={sx({ display: 'grid', gridTemplateColumns: 'minmax(0,2fr) minmax(0,1fr)', gap: 16 })} class="grid-collapse">
                <div style={sx({ display: 'flex', flexDirection: 'column', gap: 16 })}>
                  {/* 一键预设 */}
                  <Card>
                    <div style={sx({ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 10, marginBottom: 12, flexWrap: 'wrap' })}>
                      <div>
                        <b style={sx({ fontSize: 14 })}>一键预设</b>
                        <div class="muted-3" style={sx({ fontSize: 12, marginTop: 2, lineHeight: 1.5 })}>不确定怎么调？先选一个风格，再按需微调下面的滑块。</div>
                      </div>
                      <Show when={dirty()}>
                        <Badge variant="warning" dot>有未保存修改</Badge>
                      </Show>
                    </div>
                    <div style={sx({ display: 'grid', gridTemplateColumns: 'repeat(auto-fit,minmax(200px,1fr))', gap: 10 })}>
                      <For each={CFG_PRESETS}>
                        {(p) => {
                          const on = createMemo(() => presetActive(p));
                          return (
                            <button
                              onClick={() => applyPreset(p)}
                              style={sx({
                                textAlign: 'left',
                                padding: 13,
                                borderRadius: 12,
                                cursor: 'pointer',
                                border: '1.5px solid ' + (on() ? 'var(--accent)' : 'var(--border)'),
                                background: on() ? 'color-mix(in oklch, var(--accent) 12%, transparent)' : 'var(--surface-sunken)',
                              })}
                            >
                              <div style={sx({ display: 'flex', alignItems: 'center', gap: 7 })}>
                                <b style={sx({ fontSize: 13.5, color: on() ? 'var(--accent)' : 'var(--text)' })}>{p.label}</b>
                                <Show when={on()}><Icon name="check" size={14} style={sx({ color: 'var(--accent)' })} /></Show>
                              </div>
                              <div class="muted" style={sx({ fontSize: 11.5, marginTop: 4, lineHeight: 1.5 })}>{p.desc}</div>
                            </button>
                          );
                        }}
                      </For>
                    </div>
                  </Card>

                  <For each={CFG_GROUPS}>
                    {(g) => (
                      <Card>
                        <div style={sx({ display: 'flex', gap: 11, alignItems: 'flex-start', marginBottom: 16 })}>
                          <span style={sx({ width: 34, height: 34, borderRadius: 10, flex: 'none', display: 'grid', placeItems: 'center', background: `color-mix(in oklch, ${g.tone} 15%, transparent)`, color: g.tone })}>
                            <Icon name={g.icon} size={18} />
                          </span>
                          <div>
                            <div style={sx({ fontWeight: 700, fontSize: 14.5 })}>{g.label}</div>
                            <div class="muted" style={sx({ fontSize: 12, marginTop: 2, lineHeight: 1.5 })}>{g.desc}</div>
                          </div>
                        </div>
                        <div style={sx({ display: 'flex', flexDirection: 'column', gap: 18 })}>
                          <For each={g.params}>
                            {(p) => (
                              <ParamRow
                                meta={p}
                                section={g.id as string}
                                value={readNum(config(), g.id as string, p.key)}
                                def={readNum(baseline(), g.id as string, p.key)}
                                tone={g.tone}
                                onChange={(v) => setParam(g.id as string, p.key, v)}
                              />
                            )}
                          </For>
                        </div>
                      </Card>
                    )}
                  </For>

                  {/* ensemble 配比 */}
                  <Card>
                    <div style={sx({ display: 'flex', gap: 11, alignItems: 'flex-start', marginBottom: 14 })}>
                      <span style={sx({ width: 34, height: 34, borderRadius: 10, flex: 'none', display: 'grid', placeItems: 'center', background: 'color-mix(in oklch, var(--accent) 15%, transparent)', color: 'var(--accent)' })}>
                        <Icon name="layers" size={18} />
                      </span>
                      <div>
                        <div style={sx({ fontWeight: 700, fontSize: 14.5 })}>算法配比</div>
                        <div class="muted" style={sx({ fontSize: 12, marginTop: 2, lineHeight: 1.5 })}>系统综合多种记忆算法做决策，这里调节各自的话语权（系统会自动归一化）。</div>
                      </div>
                    </div>
                    <div style={sx({ display: 'flex', height: 14, borderRadius: 99, overflow: 'hidden', marginBottom: 16, border: '1px solid var(--hairline)' })}>
                      <For each={ENSEMBLE_ALGOS}>
                        {(a, i) => (
                          <div
                            title={a.label + ' ' + Math.round((readNum(config(), 'ensemble', a.key) / eSum()) * 100) + '%'}
                            style={sx({ width: (readNum(config(), 'ensemble', a.key) / eSum()) * 100 + '%', background: ALGO_COLORS[i() % ALGO_COLORS.length], transition: 'width .3s ease' })}
                          />
                        )}
                      </For>
                    </div>
                    <div style={sx({ display: 'flex', flexDirection: 'column', gap: 16 })}>
                      <For each={ENSEMBLE_ALGOS}>
                        {(a, i) => (
                          <div>
                            <div style={sx({ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline', marginBottom: 6 })}>
                              <span style={sx({ fontSize: 13, fontWeight: 600, display: 'inline-flex', gap: 7, alignItems: 'center' })}>
                                <i style={sx({ width: 9, height: 9, borderRadius: 3, background: ALGO_COLORS[i() % ALGO_COLORS.length] })} />
                                {a.label} <span class="muted-3" style={sx({ fontWeight: 400, fontSize: 11 })}>{a.desc}</span>
                              </span>
                              <span class="mono" style={sx({ fontSize: 13, fontWeight: 700, color: 'var(--accent)' })}>{Math.round((readNum(config(), 'ensemble', a.key) / eSum()) * 100)}%</span>
                            </div>
                            <input
                              type="range" min="0" max="1" step="0.01"
                              value={readNum(config(), 'ensemble', a.key)}
                              onInput={(e) => setParam('ensemble', a.key, +e.currentTarget.value)}
                              style={sx({ width: '100%', accentColor: ALGO_COLORS[i() % ALGO_COLORS.length] })}
                            />
                          </div>
                        )}
                      </For>
                    </div>
                    <div style={sx({ display: 'flex', flexDirection: 'column', gap: 18, marginTop: 18, paddingTop: 16, borderTop: '1px solid var(--hairline)' })}>
                      <For each={ENSEMBLE_EXTRA}>
                        {(p) => (
                          <ParamRow
                            meta={p}
                            section="ensemble"
                            value={readNum(config(), 'ensemble', p.key)}
                            def={readNum(baseline(), 'ensemble', p.key)}
                            tone="var(--accent)"
                            onChange={(v) => setParam('ensemble', p.key, v)}
                          />
                        )}
                      </For>
                    </div>
                  </Card>

                  {/* 功能开关 */}
                  <Card>
                    <div style={sx({ display: 'flex', gap: 11, alignItems: 'flex-start', marginBottom: 14 })}>
                      <span style={sx({ width: 34, height: 34, borderRadius: 10, flex: 'none', display: 'grid', placeItems: 'center', background: 'color-mix(in oklch, var(--success) 15%, transparent)', color: 'var(--success)' })}>
                        <Icon name="grid" size={18} />
                      </span>
                      <div>
                        <div style={sx({ fontWeight: 700, fontSize: 14.5 })}>功能开关</div>
                        <div class="muted" style={sx({ fontSize: 12, marginTop: 2, lineHeight: 1.5 })}>启停各算法模块。关闭某项后，相关策略不参与决策。</div>
                      </div>
                    </div>
                    <div style={sx({ display: 'grid', gridTemplateColumns: 'repeat(auto-fit,minmax(220px,1fr))', gap: 10 })}>
                      <For each={FLAG_META}>
                        {(f) => (
                          <label style={sx({ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 10, padding: '10px 12px', borderRadius: 10, background: 'var(--surface-sunken)' })}>
                            <span style={sx({ minWidth: 0 })}>
                              <span style={sx({ display: 'block', fontSize: 13, fontWeight: 600 })}>{f.label}</span>
                              <span class="muted-3" style={sx({ display: 'block', fontSize: 11 })}>{f.desc}</span>
                            </span>
                            <Switch checked={readBool(config(), f.key)} onChange={(v) => setParam('featureFlags', f.key, v)} />
                          </label>
                        )}
                      </For>
                    </div>
                  </Card>
                </div>

                {/* 右侧粘性辅助列：改动预览 + 影响评估 */}
                <div style={sx({ display: 'flex', flexDirection: 'column', gap: 16 })}>
                  <Panel title="改动预览" sub={dirty() ? '相对当前线上配置' : '暂无改动'}>
                    <Show
                      when={dirty()}
                      fallback={<Empty title="尚无改动" desc="拖动左侧滑块后，这里会列出你改了哪些项。" icon="sliders" />}
                    >
                      <div style={sx({ display: 'flex', flexDirection: 'column', gap: 8 })}>
                        <For each={changedRows()}>
                          {(r) => (
                            <div style={sx({ display: 'flex', justifyContent: 'space-between', alignItems: 'center', padding: '8px 10px', borderRadius: 9, background: 'var(--surface-sunken)', fontSize: 12 })}>
                              <span>{r.group} · {r.label}</span>
                              <span class="mono">
                                <span class="muted-3" style={sx({ textDecoration: 'line-through' })}>{r.before}</span>
                                {' → '}
                                <b style={sx({ color: 'var(--accent)' })}>{r.after}</b>
                              </span>
                            </div>
                          )}
                        </For>
                      </div>
                    </Show>
                  </Panel>

                  <Panel
                    title="影响评估"
                    sub="基于遥测回归估计"
                    right={<Btn size="sm" variant="secondary" onClick={checkImpact} disabled={diffLoading()}>{diffLoading() ? '评估中…' : '评估'}</Btn>}
                  >
                    <Show
                      when={diffImpact()}
                      fallback={<Empty title="点击「评估」" desc="预测这些改动对正确率、疲劳等指标的影响区间。" icon="chart" />}
                    >
                      {(d) => (
                        <div style={sx({ display: 'flex', flexDirection: 'column', gap: 10 })}>
                          <div class="muted-3" style={sx({ fontSize: 11.5 })}>样本 {d().telemetrySampleSize} · 置信度 {d().confidence}</div>
                          <For each={d().fields}>
                            {(f) => (
                              <div style={sx({ padding: 11, borderRadius: 10, background: 'var(--surface-sunken)' })}>
                                <div class="mono" style={sx({ fontSize: 11.5, fontWeight: 600 })}>{f.path}</div>
                                <div style={sx({ display: 'flex', flexDirection: 'column', gap: 4, marginTop: 7 })}>
                                  <For each={f.impacts}>
                                    {(im) => (
                                      <div style={sx({ display: 'flex', justifyContent: 'space-between', fontSize: 11.5 })}>
                                        <span class="muted">{METRIC_LABEL[im.metric] ?? im.metric}</span>
                                        <span class="mono" style={sx({ color: im.direction === 'up' ? 'var(--success)' : 'var(--error)' })}>
                                          {im.deltaLowPt > 0 ? '+' : ''}{im.deltaLowPt} ~ {im.deltaHighPt > 0 ? '+' : ''}{im.deltaHighPt}pt
                                        </span>
                                      </div>
                                    )}
                                  </For>
                                </div>
                              </div>
                            )}
                          </For>
                        </div>
                      )}
                    </Show>
                  </Panel>
                </div>
              </div>
            </Show>

            {/* ---------------- TOML 编辑 ---------------- */}
            <Show when={tab() === 'editor'}>
              <TomlEditor
                config={config()}
                onApply={(cfg) => {
                  setConfig(clone(cfg));
                  toast.success('已解析并应用 TOML', '记得保存或热重载生效');
                }}
              />
            </Show>

            {/* ---------------- 版本历史（行内表格） ---------------- */}
            <Show when={tab() === 'versions'}>
              <Card pad={false} style={sx({ overflow: 'hidden' })}>
                <Show when={versions.state !== 'pending'} fallback={<Loading />}>
                  <Show
                    when={(versions() ?? []).length > 0}
                    fallback={<div style={sx({ padding: 20 })}><Empty title="暂无版本" desc="保存为新版本后会出现在这里，可随时回滚。" icon="db" /></div>}
                  >
                    <div style={sx({ overflowX: 'auto' })}>
                      <table class="tbl">
                        <thead>
                          <tr>
                            <th>版本哈希</th><th>来源</th><th>备注</th><th>作者</th><th>时间</th><th style={sx({ textAlign: 'right' })}>操作</th>
                          </tr>
                        </thead>
                        <tbody>
                          <For each={versions()}>
                            {(v, i) => (
                              <tr>
                                <td class="mono" style={sx({ fontSize: 12 })}>
                                  {v.versionHash}
                                  <Show when={i() === 0}><Badge variant="success" style={sx({ marginLeft: 7 })}>live</Badge></Show>
                                </td>
                                <td><Badge variant={v.source === 'manual' ? 'default' : v.source === 'llm_auto' ? 'warning' : 'accent'}>{v.source}</Badge></td>
                                <td style={sx({ fontSize: 12.5 })}>{v.note ?? '—'}</td>
                                <td class="mono muted" style={sx({ fontSize: 11.5 })}>{v.authorAdminId}</td>
                                <td class="muted-3" style={sx({ fontSize: 11.5 })}>{fmtAgoLocal(v.createdAt)}</td>
                                <td style={sx({ textAlign: 'right' })}>
                                  <Show when={i() !== 0}>
                                    <RestoreBtn hash={v.versionHash} onRestore={restoreVersion} />
                                  </Show>
                                </td>
                              </tr>
                            )}
                          </For>
                        </tbody>
                      </table>
                    </div>
                  </Show>
                </Show>
              </Card>
            </Show>

            {/* ---------------- 灰度发布 ---------------- */}
            <Show when={tab() === 'canary'}>
              <CanaryPanel
                versions={versions() ?? []}
                canary={canary() ?? null}
                onChanged={() => void refetchCanary()}
              />
            </Show>
          </div>

          {/* 版本历史抽屉（带详情 + 回滚） */}
          <VersionDrawer
            open={drawerOpen()}
            onClose={() => setDrawerOpen(false)}
            versions={versions() ?? []}
            onRestore={restoreVersion}
          />
        </Show>
      </Show>

      {/* 保存确认 */}
      <Confirm
        open={confirmKind() === 'save'}
        onClose={() => setConfirmKind(null)}
        onConfirm={doSave}
        loading={busy()}
        title="保存为新版本"
        confirmText="保存"
        body={
          <div>
            <p style={sx({ marginTop: 0 })}>将当前配置保存为新的可追溯版本（随时可回滚），对所有在线用户立即生效。</p>
            <Field label="版本备注">
              <input class="input" value={note()} onInput={(e) => setNote(e.currentTarget.value)} placeholder="如：下调探索强度、放宽疲劳阈值" />
            </Field>
          </div>
        }
      />

      {/* 热重载确认 */}
      <Confirm
        open={confirmKind() === 'reload'}
        onClose={() => setConfirmKind(null)}
        onConfirm={doReload}
        loading={busy()}
        danger
        title="热重载配置"
        confirmText="热重载"
        body="将当前配置立即应用到运行时（绕过版本化），对新的学习决策生效，不会创建新版本快照。务必先确认无误。"
      />
    </div>
  );
}

/* ---------------------------- ParamRow ---------------------------- */
function ParamRow(props: {
  meta: ParamMeta;
  section: string;
  value: number;
  def: number;
  tone: string;
  onChange: (v: number) => void;
}) {
  const [showHelp, setShowHelp] = createSignal(false);
  const [explain, setExplain] = createSignal<AmasExplainResponse | null>(null);
  const [explaining, setExplaining] = createSignal(false);
  const isDefault = () => props.value === props.def;

  async function doExplain() {
    if (explain()) {
      setExplain(null);
      return;
    }
    setExplaining(true);
    try {
      const r = await adminApi.amasExplainParam(`${props.section}.${props.meta.key}`, props.value);
      setExplain(r);
    } catch (err: unknown) {
      toast.error('解释失败', err instanceof Error ? err.message : '未知错误');
    } finally {
      setExplaining(false);
    }
  }

  return (
    <div>
      <div style={sx({ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline', marginBottom: 6, gap: 10 })}>
        <span style={sx({ fontSize: 13.5, fontWeight: 600, display: 'inline-flex', alignItems: 'center', gap: 6 })}>
          {props.meta.label}
          <button
            onClick={() => setShowHelp((s) => !s)}
            title="说明"
            style={sx({ border: 'none', background: 'transparent', cursor: 'pointer', color: 'var(--text-3)', padding: 0, display: 'inline-flex' })}
          >
            <Icon name="info" size={13} />
          </button>
          <button
            onClick={doExplain}
            title="LLM 解释"
            disabled={explaining()}
            style={sx({ border: 'none', background: 'transparent', cursor: 'pointer', color: 'var(--text-3)', padding: 0, display: 'inline-flex' })}
          >
            <Icon name="bulb" size={13} />
          </button>
        </span>
        <span style={sx({ display: 'inline-flex', alignItems: 'center', gap: 8 })}>
          <Show when={!isDefault()}>
            <button
              onClick={() => props.onChange(props.def)}
              class="muted-3"
              style={sx({ border: 'none', background: 'transparent', cursor: 'pointer', fontSize: 10.5 })}
              title="恢复 baseline"
            >
              baseline {props.meta.fmt(props.def)} ↺
            </button>
          </Show>
          <span class="mono" style={sx({ fontSize: 13.5, fontWeight: 700, color: props.tone, minWidth: 52, textAlign: 'right' })}>{props.meta.fmt(props.value)}</span>
        </span>
      </div>
      <div class="muted" style={sx({ fontSize: 11.5, marginBottom: 7, lineHeight: 1.45 })}>{props.meta.what}</div>
      <input
        type="range"
        min={props.meta.min}
        max={props.meta.max}
        step={props.meta.step}
        value={props.value}
        onInput={(e) => props.onChange(+e.currentTarget.value)}
        style={sx({ width: '100%', accentColor: props.tone })}
      />
      <div style={sx({ display: 'flex', justifyContent: 'space-between', marginTop: 3 })}>
        <span class="muted-3" style={sx({ fontSize: 10 })}>{props.meta.fmt(props.meta.min)}</span>
        <span class="muted-3" style={sx({ fontSize: 10 })}>{props.meta.fmt(props.meta.max)}</span>
      </div>
      <Show when={showHelp()}>
        <div class="fade-in" style={sx({ marginTop: 8, padding: 10, borderRadius: 9, background: 'var(--surface-sunken)', fontSize: 11.5, lineHeight: 1.6, display: 'flex', flexDirection: 'column', gap: 4 })}>
          <span style={sx({ color: 'var(--success)' })}>↑ 调高：{props.meta.high}</span>
          <span style={sx({ color: 'var(--warning)' })}>↓ 调低：{props.meta.low}</span>
        </div>
      </Show>
      <Show when={explain()}>
        {(e) => (
          <div class="fade-in" style={sx({ marginTop: 8, padding: 11, borderRadius: 10, background: 'color-mix(in oklch, var(--accent) 8%, transparent)', fontSize: 11.5, lineHeight: 1.6 })}>
            <div class="eyebrow" style={sx({ marginBottom: 6 })}>LLM 解释 · {e().model} · ${e().costUsd.toFixed(4)}</div>
            <p style={sx({ margin: 0 })}>{e().explanation}</p>
          </div>
        )}
      </Show>
    </div>
  );
}

/* ---------------------------- 回滚按钮（带确认） ---------------------------- */
function RestoreBtn(props: { hash: string; onRestore: (h: string) => Promise<void> }) {
  const [open, setOpen] = createSignal(false);
  const [busy, setBusy] = createSignal(false);
  return (
    <>
      <Btn size="xs" variant="outline" icon="rotate" onClick={() => setOpen(true)}>回滚到此</Btn>
      <Confirm
        open={open()}
        onClose={() => setOpen(false)}
        onConfirm={async () => {
          setBusy(true);
          try {
            await props.onRestore(props.hash);
          } finally {
            setBusy(false);
            setOpen(false);
          }
        }}
        loading={busy()}
        danger
        title="回滚到此版本"
        confirmText="回滚"
        body={<span>将把运行时配置回滚到版本 <b class="mono">{props.hash}</b>，并写入新版本记录。此操作对所有在线用户生效。</span>}
      />
    </>
  );
}

/* ---------------------------- TOML 编辑器 ---------------------------- */
function TomlEditor(props: { config: AmasConfig; onApply: (cfg: AmasConfig) => void }) {
  const [text, setText] = createSignal('');
  const [loading, setLoading] = createSignal(true);
  const [parsing, setParsing] = createSignal(false);
  const [serializing, setSerializing] = createSignal(false);

  async function loadToml() {
    setLoading(true);
    try {
      const r = await adminApi.amasSerializeToml(props.config);
      setText(r.toml);
    } catch (err: unknown) {
      toast.error('序列化失败', err instanceof Error ? err.message : '未知错误');
    } finally {
      setLoading(false);
    }
  }

  onMount(loadToml);

  async function syncFromConfig() {
    setSerializing(true);
    try {
      const r = await adminApi.amasSerializeToml(props.config);
      setText(r.toml);
      toast.success('已用当前可视化配置刷新 TOML');
    } catch (err: unknown) {
      toast.error('序列化失败', err instanceof Error ? err.message : '未知错误');
    } finally {
      setSerializing(false);
    }
  }

  async function parseAndApply() {
    setParsing(true);
    try {
      const cfg = await adminApi.amasParseToml(text());
      props.onApply(cfg);
    } catch (err: unknown) {
      toast.error('TOML 解析失败', err instanceof Error ? err.message : '请检查语法');
    } finally {
      setParsing(false);
    }
  }

  return (
    <Card>
      <div style={sx({ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', marginBottom: 10, gap: 12, flexWrap: 'wrap' })}>
        <div>
          <b style={sx({ fontSize: 13.5 })}>配置源 (TOML)</b>
          <div class="muted-3" style={sx({ fontSize: 11.5, marginTop: 2, maxWidth: 560, lineHeight: 1.5 })}>面向工程师的原始视图。编辑后点「解析并应用」回写到可视化配置；解析失败不会改动当前配置。</div>
        </div>
        <div style={sx({ display: 'flex', gap: 8 })}>
          <Btn size="sm" variant="ghost" icon="copy" onClick={() => { void navigator.clipboard?.writeText(text()); toast.success('已复制 TOML'); }}>复制</Btn>
          <Btn size="sm" variant="secondary" icon="refresh" disabled={serializing()} onClick={syncFromConfig}>从可视化刷新</Btn>
          <Btn size="sm" variant="primary" icon="check" disabled={parsing() || loading()} onClick={parseAndApply}>{parsing() ? '解析中…' : '解析并应用'}</Btn>
        </div>
      </div>
      <Show when={!loading()} fallback={<Loading h={300} />}>
        <textarea
          class="textarea mono"
          style={sx({ minHeight: 460, fontSize: 12.5, lineHeight: 1.7, background: 'var(--surface-sunken)', width: '100%' })}
          value={text()}
          onInput={(e) => setText(e.currentTarget.value)}
          spellcheck={false}
        />
      </Show>
    </Card>
  );
}

/* ---------------------------- 灰度发布面板 ---------------------------- */
function CanaryPanel(props: {
  versions: AmasConfigVersionRow[];
  canary: AmasCanaryConfig | null;
  onChanged: () => void;
}) {
  const [vh, setVh] = createSignal('');
  const [percent, setPercent] = createSignal(5);
  const [minAccountAgeDays, setMinAccountAgeDays] = createSignal(0);
  const [preferActive, setPreferActive] = createSignal(false);
  const [webOnly, setWebOnly] = createSignal(false);
  const [autoScale24h, setAutoScale24h] = createSignal(false);
  const [audience, setAudience] = createSignal<CanaryAudience | null>(null);
  const [busy, setBusy] = createSignal(false);

  // 默认选中 live 版本 / 已有灰度版本
  onMount(() => {
    if (props.canary) {
      setVh(props.canary.versionHash);
      setPercent(props.canary.percent);
    } else if (props.versions.length) {
      setVh(props.versions[0].versionHash);
    }
  });

  async function apply() {
    if (!vh()) {
      toast.warning('请选择灰度版本');
      return;
    }
    setBusy(true);
    try {
      const req: SetCanaryExtRequest = {
        versionHash: vh(),
        percent: percent(),
        minAccountAgeDays: minAccountAgeDays(),
        preferActive: preferActive(),
        webOnly: webOnly(),
        autoScale24h: autoScale24h(),
      };
      const r = await adminApi.amasSetCanaryExt(req);
      setAudience(r.audience);
      toast.success('灰度已设置', percent() + '% · 影响约 ' + (r.audience?.affectedUsers ?? 0) + ' 人');
      props.onChanged();
    } catch (err: unknown) {
      toast.error('设置失败', err instanceof Error ? err.message : '未知错误');
    } finally {
      setBusy(false);
    }
  }

  async function disable() {
    setBusy(true);
    try {
      await adminApi.amasDisableCanary();
      setAudience(null);
      toast.success('已关闭灰度');
      props.onChanged();
    } catch (err: unknown) {
      toast.error('关闭失败', err instanceof Error ? err.message : '未知错误');
    } finally {
      setBusy(false);
    }
  }

  return (
    <div style={sx({ display: 'grid', gridTemplateColumns: 'minmax(0,1fr) minmax(0,1fr)', gap: 16 })} class="grid-collapse">
      <Panel title="灰度配置" sub="人群过滤 + 比例分流（estimate-only）">
        <Show when={props.canary}>
          <div style={sx({ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 14, padding: '8px 11px', borderRadius: 9, background: 'color-mix(in oklch, var(--accent) 10%, transparent)', fontSize: 12 })}>
            <Badge variant="accent" dot>灰度中</Badge>
            <span class="mono">{props.canary!.versionHash} · {props.canary!.percent}%</span>
          </div>
        </Show>
        <div style={sx({ display: 'flex', flexDirection: 'column', gap: 14 })}>
          <Field label="灰度版本">
            <select class="select" value={vh()} onChange={(e) => setVh(e.currentTarget.value)}>
              <For each={props.versions}>
                {(v) => <option value={v.versionHash}>{v.versionHash} · {(v.note ?? '').slice(0, 18)}</option>}
              </For>
            </select>
          </Field>
          <div>
            <div class="field-label">灰度比例 {percent()}%</div>
            <input type="range" min="0" max="100" value={percent()} onInput={(e) => setPercent(+e.currentTarget.value)} style={sx({ width: '100%', accentColor: 'var(--accent)' })} />
          </div>
          <div style={sx({ display: 'flex', flexDirection: 'column', gap: 9 })}>
            <Field label="最小账号年龄（天）">
              <input class="input" type="number" value={minAccountAgeDays()} onInput={(e) => setMinAccountAgeDays(+e.currentTarget.value)} />
            </Field>
            <label style={sx({ display: 'flex', justifyContent: 'space-between', alignItems: 'center', fontSize: 13 })}>优先活跃用户<Switch checked={preferActive()} onChange={setPreferActive} /></label>
            <label style={sx({ display: 'flex', justifyContent: 'space-between', alignItems: 'center', fontSize: 13 })}>仅 Web 端<Switch checked={webOnly()} onChange={setWebOnly} /></label>
            <label style={sx({ display: 'flex', justifyContent: 'space-between', alignItems: 'center', fontSize: 13 })}>24h 自动扩量<Switch checked={autoScale24h()} onChange={setAutoScale24h} /></label>
          </div>
          <div style={sx({ display: 'flex', gap: 8 })}>
            <Btn variant="primary" onClick={apply} disabled={busy()}>{busy() ? '应用中…' : '应用灰度'}</Btn>
            <Btn variant="ghost" onClick={disable} disabled={busy()}>关闭灰度</Btn>
          </div>
        </div>
      </Panel>
      <Panel title="受众估计">
        <Show
          when={audience()}
          fallback={<Empty title="尚未应用灰度" desc="配置左侧参数后点击「应用灰度」查看受众估计。" icon="users" />}
        >
          {(a) => (
            <div>
              <div style={sx({ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12, marginBottom: 14 })}>
                <MetricBox label="总用户" value={String(a().totalUsers)} />
                <MetricBox label="符合条件" value={String(a().eligibleUsers)} />
                <MetricBox label="受影响" value={String(a().affectedUsers)} />
                <MetricBox label="执行模式" value={a().enforcement} />
              </div>
              <Progress value={a().totalUsers ? (a().affectedUsers / a().totalUsers) * 100 : 0} tone="accent" height={8} showLabel />
            </div>
          )}
        </Show>
      </Panel>
    </div>
  );
}

function MetricBox(props: { label: string; value: string }) {
  return (
    <div style={sx({ padding: 12, borderRadius: 10, background: 'var(--surface-sunken)' })}>
      <div class="muted-3" style={sx({ fontSize: 10.5 })}>{props.label}</div>
      <div class="mono tnum" style={sx({ fontSize: 18, fontWeight: 700, marginTop: 3 })}>{props.value}</div>
    </div>
  );
}

/* ---------------------------- 版本历史抽屉 ---------------------------- */
function VersionDrawer(props: {
  open: boolean;
  onClose: () => void;
  versions: AmasConfigVersionRow[];
  onRestore: (h: string) => Promise<void>;
}) {
  const [detail, setDetail] = createSignal<string | null>(null);
  const [snapshot, setSnapshot] = createSignal<string>('');
  const [snapLoading, setSnapLoading] = createSignal(false);

  async function openDetail(hash: string) {
    if (detail() === hash) {
      setDetail(null);
      return;
    }
    setDetail(hash);
    setSnapLoading(true);
    setSnapshot('');
    try {
      const d = await adminApi.amasGetVersion(hash);
      setSnapshot(JSON.stringify(d.snapshotJson, null, 2));
    } catch (err: unknown) {
      setSnapshot('// 无法加载快照：' + (err instanceof Error ? err.message : '未知错误'));
    } finally {
      setSnapLoading(false);
    }
  }

  return (
    <Drawer open={props.open} onClose={props.onClose} title="版本历史" subtitle={`${props.versions.length} 个版本`} width={560}>
      <Show
        when={props.versions.length > 0}
        fallback={<Empty title="暂无版本" desc="保存为新版本后会出现在这里。" icon="db" />}
      >
        <div style={sx({ display: 'flex', flexDirection: 'column', gap: 10 })}>
          <For each={props.versions}>
            {(v, i) => (
              <div style={sx({ padding: 12, borderRadius: 11, border: '1px solid var(--hairline)', background: 'var(--surface-sunken)' })}>
                <div style={sx({ display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: 10, flexWrap: 'wrap' })}>
                  <div style={sx({ display: 'flex', alignItems: 'center', gap: 8 })}>
                    <span class="mono" style={sx({ fontSize: 12.5, fontWeight: 600 })}>{v.versionHash}</span>
                    <Show when={i() === 0}><Badge variant="success">live</Badge></Show>
                    <Badge variant={v.source === 'manual' ? 'default' : v.source === 'llm_auto' ? 'warning' : 'accent'}>{v.source}</Badge>
                  </div>
                  <div style={sx({ display: 'flex', gap: 6 })}>
                    <IconBtn name="search" title="查看快照" onClick={() => void openDetail(v.versionHash)} />
                    <Show when={i() !== 0}>
                      <RestoreBtn hash={v.versionHash} onRestore={props.onRestore} />
                    </Show>
                  </div>
                </div>
                <div class="muted" style={sx({ fontSize: 12, marginTop: 6 })}>{v.note ?? '—'}</div>
                <div class="muted-3 mono" style={sx({ fontSize: 11, marginTop: 4 })}>{v.authorAdminId} · {fmtAgoLocal(v.createdAt)}</div>
                <Show when={detail() === v.versionHash}>
                  <Show when={!snapLoading()} fallback={<div style={sx({ marginTop: 8 })}><Loading h={80} /></div>}>
                    <textarea
                      class="textarea mono"
                      readonly
                      style={sx({ marginTop: 8, minHeight: 220, fontSize: 11, lineHeight: 1.6, background: 'var(--surface-solid)', width: '100%' })}
                      value={snapshot()}
                    />
                  </Show>
                </Show>
              </div>
            )}
          </For>
        </div>
      </Show>
    </Drawer>
  );
}

/* ---------------------------- 本地工具 ---------------------------- */
function fmtAgoLocal(iso: string): string {
  const t = new Date(iso).getTime();
  if (Number.isNaN(t)) return iso;
  const diff = Date.now() - t;
  const m = Math.floor(diff / 60000);
  if (m < 1) return '刚刚';
  if (m < 60) return m + ' 分钟前';
  const h = Math.floor(m / 60);
  if (h < 24) return h + ' 小时前';
  const d = Math.floor(h / 24);
  if (d < 30) return d + ' 天前';
  return new Date(iso).toLocaleDateString();
}
