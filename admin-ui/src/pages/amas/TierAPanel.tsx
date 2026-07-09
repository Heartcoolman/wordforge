import { For, createMemo, Show } from 'solid-js';
import { Switch } from '@/components/ui/Switch';
import { Badge } from '@/components/ui/Badge';
import { Card } from '@/components/ui/Card';
import { ParamCard } from '@/components/amas/ParamCard';
import {
  TIER_A_META,
  PARAM_INDEX,
  applyPreset,
  diffKnown,
  getByPath,
  setByPath,
  type FieldError,
  type PresetId,
} from './schema';
import { cn } from '@/utils/cn';

interface TierAPanelProps {
  config: Record<string, unknown>;
  errors: FieldError[];
  onChange: (next: Record<string, unknown>) => void;
}

/** 4 preset 卡的展示元数据(对齐 amas-config.html .tier-a-cards 设计图) */
interface PresetCardInfo {
  id: PresetId | 'conservative' | 'aggressive' | 'heuristic_only';
  name: string;
  badge: string;
  desc: string;
  implemented: boolean;
  /** 显示在卡底的 4 个特征属性,纯展示用 */
  props: Array<{ k: string; v: string }>;
}

const PRESET_CARDS: PresetCardInfo[] = [
  {
    id: 'factory_default',
    name: '出厂默认 (FSRS-6)',
    badge: '推荐',
    desc: 'AMASConfig::default()=FSRS-6 公版参数(21 维 w+可训练 decay)。2026-06-10 重调参确认其已近 Pareto 前沿,生产首选。',
    implemented: true,
    props: [
      { k: 'baseDesiredRetention', v: '0.9' },
      { k: 'maxIntervalDays', v: '90' },
      { k: 'w[20] decay', v: '0.154' },
      { k: 'w[2]', v: '2.31' },
    ],
  },
  {
    id: 'conservative',
    name: '保守 (conservative)',
    badge: '低风险',
    desc: '低探索率、关闭实验算法 (IAD / MTP / SSP)。新用户冷启动平稳,但探索性差。',
    implemented: false,
    props: [
      { k: 'explore_eps', v: '0.02' },
      { k: 'flags 关', v: '3 个' },
      { k: 'cooldown', v: '12 min' },
      { k: 'ensemble_th', v: '0.74' },
    ],
  },
  {
    id: 'aggressive',
    name: '激进 (aggressive)',
    badge: '高探索',
    desc: '高探索率,启用全部 8 个 flag,MDM 衰减加快。适合数据密集型 / A-B 研究。',
    implemented: false,
    props: [
      { k: 'explore_eps', v: '0.18' },
      { k: 'flags 全开', v: '8/8' },
      { k: 'cooldown', v: '3 min' },
      { k: 'ensemble_th', v: '0.42' },
    ],
  },
];

/** 7 个 featureFlags(4 决策蓝条 + 3 记忆/排程黄条),对齐设计图 .flag-grid */
interface FlagInfo {
  path: string;
  kind: 'decision' | 'memory';
  role: string;
}

const FLAG_LIST: FlagInfo[] = [
  { path: 'featureFlags.ensembleEnabled', kind: 'decision', role: '投票组合 (默认主路由)' },
  { path: 'featureFlags.heuristicEnabled', kind: 'decision', role: '规则启发式 (兜底)' },
  { path: 'featureFlags.igeEnabled', kind: 'decision', role: '信息增益探索' },
  { path: 'featureFlags.swdEnabled', kind: 'decision', role: '滑窗判别' },
  { path: 'featureFlags.sspEnabled', kind: 'memory', role: '间隔节省成本' },
  { path: 'featureFlags.confusionIsolationEnabled', kind: 'memory', role: '混淆隔离排程' },
  { path: 'featureFlags.multiTraceEnabled', kind: 'memory', role: '跨题型多痕迹' },
];

/** 核心 4 旋钮:从 TIER_A 中选 sensitivity=ultra 的 4 个高频字段 */
const QUICK_KNOB_PATHS: readonly string[] = [
  'memoryModel.baseDesiredRetention',
  'memoryModel.maxIntervalDays',
  'memoryModel.w[2]',
  'memoryModel.w[16]',
] as const;

/**
 * 重点参数页(对齐 amas-config.html TierA pane 三层运维视角):
 * 1. 4 preset cards 横排(2 真 + 2 占位 disabled)
 * 2. 算法 flag block(8 flag · 4 决策蓝条 + 4 记忆黄条)
 * 3. 核心 4 旋钮 block(高敏 ultra 字段)
 * 4. Tier-A 12 维 grid(FSRS-6 高杠杆精雕项,含 w[20] 曲线 decay)
 */
export function TierAPanel(props: TierAPanelProps) {
  const errorMap = createMemo(() => new Map(props.errors.map((e) => [e.path, e.message])));

  function setValue(path: string, v: unknown) {
    const next = structuredClone(props.config);
    setByPath(next, path, v);
    props.onChange(next);
  }

  /** 当前 config 是否完全匹配某 preset */
  const matchedPreset = createMemo<PresetId | null>(() => {
    for (const id of ['factory_default'] as PresetId[]) {
      const fresh = applyPreset({}, id);
      if (diffKnown(fresh, props.config).length === 0) return id;
    }
    return null;
  });

  function applyPresetCard(id: PresetCardInfo['id']) {
    if (id === 'conservative' || id === 'aggressive' || id === 'heuristic_only') return;
    const next = applyPreset(props.config, id);
    props.onChange(next);
  }

  return (
    <div class="space-y-4">
      {/* 4 preset cards */}
      <div class="grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-4 gap-3">
        <For each={PRESET_CARDS}>
          {(info) => {
            const isCurrent = () => info.implemented && matchedPreset() === info.id;
            return (
              <button
                type="button"
                class={cn(
                  'rounded-lg bg-surface-elevated p-4 text-left shadow-elevation-1',
                  'border-2 border-transparent transition-all duration-fast ease-out-expo',
                  'hover:-translate-y-0.5 hover:shadow-elevation-2',
                  'focus-ring-soft',
                  isCurrent() && 'border-accent bg-accent-light/10',
                  !info.implemented && 'opacity-60 cursor-not-allowed hover:translate-y-0 hover:shadow-elevation-1',
                )}
                onClick={() => applyPresetCard(info.id)}
                disabled={!info.implemented}
                aria-pressed={isCurrent()}
                title={info.implemented ? '点击应用该预设' : '尚未实施'}
              >
                <div class="flex items-center justify-between gap-2">
                  <span class="text-sm font-semibold text-content">{info.name}</span>
                  <Badge variant={isCurrent() ? 'accent' : 'default'} size="sm">
                    {isCurrent() ? '当前' : info.badge}
                  </Badge>
                </div>
                <p class="mt-2 text-xs leading-relaxed text-content-secondary line-clamp-3">
                  {info.desc}
                </p>
                <div class="mt-3 pt-3 border-t border-border-hairline grid grid-cols-2 gap-x-3 gap-y-1.5">
                  <For each={info.props}>
                    {(p) => (
                      <div class="text-[10.5px] font-mono text-content-tertiary truncate">
                        {p.k} <strong class="text-content tabular-nums">{p.v}</strong>
                      </div>
                    )}
                  </For>
                </div>
              </button>
            );
          }}
        </For>
      </div>

      {/* 算法 flag block */}
      <Card variant="elevated" padding="lg">
        <div class="space-y-3">
          <div>
            <h3 class="text-sm font-semibold text-content">算法 flag · featureFlags 一层开关</h3>
            <p class="mt-1 text-xs text-content-tertiary leading-snug">
              对应 <code class="font-mono text-[11px] px-1.5 py-0.5 rounded bg-surface-secondary text-accent">[featureFlags]</code>{' '}
              · 4 决策算法 + 4 记忆模型 (
              <span class="inline-block w-2 h-2 align-middle bg-info rounded-sm mx-0.5" /> 决策{' '}
              <span class="inline-block w-2 h-2 align-middle bg-warning rounded-sm mx-0.5" /> 记忆)
            </p>
          </div>
          <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-2.5">
            <For each={FLAG_LIST}>
              {(flag) => {
                const meta = PARAM_INDEX.get(flag.path);
                const checked = () => Boolean(getByPath(props.config, flag.path));
                return (
                  <Show when={meta}>
                    {(m) => (
                      <div
                        class={cn(
                          'flex items-center gap-2.5 rounded-md bg-surface-secondary p-3',
                          'border-l-[3px] transition-colors hover:bg-surface-tertiary',
                          flag.kind === 'decision' && 'border-info',
                          flag.kind === 'memory' && 'border-warning',
                        )}
                      >
                        <div class="flex-1 min-w-0">
                          <div class="text-[12.5px] font-mono font-semibold text-content tracking-wide truncate">
                            {flag.path.split('.')[1]}
                          </div>
                          <div class="mt-0.5 text-[10.5px] text-content-tertiary leading-tight">
                            {flag.role}
                          </div>
                        </div>
                        <Switch checked={checked()} onChange={(v) => setValue(flag.path, v)} />
                      </div>
                    )}
                  </Show>
                );
              }}
            </For>
          </div>
        </div>
      </Card>

      {/* 核心 4 旋钮 */}
      <Card variant="elevated" padding="lg">
        <div class="space-y-3">
          <div>
            <h3 class="text-sm font-semibold text-content">核心 4 旋钮 · 最高频调整</h3>
            <p class="mt-1 text-xs text-content-tertiary leading-snug">
              90% 的真实调参只动这 4 个 ultra 敏感字段,详细参数请到「分节配置」或「JSON 高级」。
            </p>
          </div>
          <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
            <For each={QUICK_KNOB_PATHS}>
              {(path) => {
                const meta = PARAM_INDEX.get(path);
                return (
                  <Show when={meta}>
                    {(m) => (
                      <ParamCard
                        meta={m()}
                        value={getByPath(props.config, path)}
                        error={errorMap().get(path)}
                        onChange={(v) => setValue(path, v)}
                        compact
                      />
                    )}
                  </Show>
                );
              }}
            </For>
          </div>
        </div>
      </Card>

      {/* Tier-A 12 维 grid (保留原有功能) */}
      <Card variant="elevated" padding="lg">
        <div class="space-y-3">
          <div>
            <h3 class="text-sm font-semibold text-content">Tier-A 12 维 · 高杠杆精雕项</h3>
            <p class="mt-1 text-xs text-content-tertiary leading-snug">
              经 2026-05-15 调参验证,调整这 11 个参数即可拿到核心收益(预测 +10.6% / 记忆量 +14%)。
              其它参数对核心指标贡献较小,建议保持默认或在「分节配置」中谨慎调整。
            </p>
          </div>
          <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-3">
            <For each={TIER_A_META}>
              {(meta) => (
                <ParamCard
                  meta={meta}
                  value={getByPath(props.config, meta.path)}
                  error={errorMap().get(meta.path)}
                  onChange={(v) => setValue(meta.path, v)}
                  compact
                />
              )}
            </For>
          </div>
        </div>
      </Card>
    </div>
  );
}
