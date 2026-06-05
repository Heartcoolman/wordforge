import { For, createMemo, Show } from 'solid-js';
import { Badge } from '@/components/ui/Badge';
import { ParamCard } from '@/components/amas/ParamCard';
import { PARAM_DICT, getByPath, setByPath, type FieldError } from './schema';

interface SectionPanelProps {
  config: Record<string, unknown>;
  errors: FieldError[];
  onChange: (next: Record<string, unknown>) => void;
}

const SECTION_LABEL_ZH: Record<string, string> = {
  featureFlags: '功能开关',
  ensemble: '集成（Ensemble）',
  modeling: '状态建模',
  constraints: '约束',
  objectiveWeights: '目标权重',
  memoryModel: '记忆模型（FSRS-5）',
  monitoring: '监控',
  coldStart: '冷启动',
};

const SECTION_DESC_ZH: Record<string, string> = {
  featureFlags: '8 个算法路由开关,关闭后退回 Heuristic 兜底',
  ensemble: '3 路集成 (Heuristic / IGE / SWD) 投票权重 + 信任融合',
  modeling: '注意力 / 疲劳 / 动机的 EMA 平滑参数',
  constraints: '高疲劳保护 / 低注意力降难度的硬阈值',
  objectiveWeights: '决策目标函数 5 项权重 (和应 ≈ 1)',
  memoryModel: 'FSRS-5 w[0..18] + 目标留存率 + 间隔上限',
  monitoring: '采样率 + 指标落盘间隔',
  coldStart: '冷启动 → 探索 → 利用三相位切换阈值',
};

/**
 * 分节配置(对齐 amas-config.html .grid.cols-4 设计图):
 * 按 section 分组的扁平 4 列 ParamCard 网格,section 之间 spacing-y-5。
 *
 * 与旧 collapsible 版本的区别:
 * - 旧版:8 section 折叠,默认只开第 1 个,需要点击展开 → 调参时来回点
 * - 新版:全部默认展开,4 列扁平铺开,section 标题作为视觉锚点
 *
 * 移除了 Card 容器和折叠箭头,空间让给参数本身。
 * 未在 PARAM_DICT 中精雕的字段(其余 ~225 项)在「JSON 高级」编辑。
 */
export function SectionPanel(props: SectionPanelProps) {
  const sections = Object.keys(PARAM_DICT);
  const errorMap = createMemo(() => new Map(props.errors.map((e) => [e.path, e.message])));
  const errorsBySection = createMemo(() => {
    const m = new Map<string, number>();
    for (const e of props.errors) {
      const section = e.path.split('.')[0];
      m.set(section, (m.get(section) ?? 0) + 1);
    }
    return m;
  });
  const totalParams = Object.values(PARAM_DICT).reduce((a, b) => a + b.length, 0);

  function setValue(path: string, v: unknown) {
    const next = structuredClone(props.config);
    setByPath(next, path, v);
    props.onChange(next);
  }

  return (
    <div class="space-y-5">
      <p class="text-xs text-content-tertiary">
        共 <span class="font-mono tabular-nums">{totalParams}</span> 个已知字段,按 section 分组
        4 列展开。其余字段请到「JSON 高级」编辑。
      </p>
      <For each={sections}>
        {(section) => {
          const params = PARAM_DICT[section];
          const sectionErrors = () => errorsBySection().get(section) ?? 0;
          return (
            <section id={`amas-section-${section}`} class="scroll-mt-24 space-y-3">
              <div class="flex items-baseline gap-3 flex-wrap">
                <h3 class="text-sm font-semibold text-content">
                  {SECTION_LABEL_ZH[section] ?? section}
                </h3>
                <span class="text-xs text-content-tertiary">
                  <span class="font-mono tabular-nums">{params.length}</span> 项
                  <Show when={SECTION_DESC_ZH[section]}>
                    {' · '}
                    {SECTION_DESC_ZH[section]}
                  </Show>
                </span>
                <Show when={sectionErrors() > 0}>
                  <Badge variant="error" size="sm">
                    {sectionErrors()} 错
                  </Badge>
                </Show>
              </div>
              <div class="grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4 gap-3">
                <For each={params}>
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
            </section>
          );
        }}
      </For>
    </div>
  );
}
