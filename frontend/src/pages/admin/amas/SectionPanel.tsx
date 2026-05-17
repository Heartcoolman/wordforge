import { For, createSignal, Show } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { ParamField } from './ParamField';
import { PARAM_DICT, getByPath, setByPath, type FieldError, type ParamMeta } from './schema';

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

/** 分节配置：按 section 折叠，每节内一列 ParamField。 */
export function SectionPanel(props: SectionPanelProps) {
  const errorMap = () => new Map(props.errors.map((e) => [e.path, e.message]));
  const [openSection, setOpenSection] = createSignal<string | null>('memoryModel');
  const sections = Object.keys(PARAM_DICT);

  function setValue(path: string, v: unknown) {
    const next = structuredClone(props.config);
    setByPath(next, path, v);
    props.onChange(next);
  }

  function sectionErrorCount(section: string, paramsArr: ParamMeta[]): number {
    const set = new Set(paramsArr.map((p) => p.path));
    return props.errors.filter((e) => set.has(e.path) || e.path.startsWith(`${section}.`)).length;
  }

  return (
    <div class="space-y-2">
      <For each={sections}>
        {(section) => {
          const params = PARAM_DICT[section];
          const isOpen = () => openSection() === section;
          return (
            <Card variant="outlined" padding="none">
              <button
                type="button"
                class="w-full flex items-center justify-between px-4 py-3 hover:bg-surface-secondary/50 transition-colors"
                onClick={() => setOpenSection(isOpen() ? null : section)}
                aria-expanded={isOpen()}
              >
                <span class="flex items-center gap-2 text-sm font-semibold text-content">
                  {SECTION_LABEL_ZH[section] ?? section}
                  <span class="text-xs text-content-tertiary font-normal">{params.length} 项</span>
                  <Show when={sectionErrorCount(section, params) > 0}>
                    <span class="inline-flex items-center px-1.5 py-0.5 rounded-full bg-error-light text-error text-[10px]">
                      {sectionErrorCount(section, params)} 错
                    </span>
                  </Show>
                </span>
                <span class="text-content-tertiary text-xs">{isOpen() ? '收起' : '展开'}</span>
              </button>
              <Show when={isOpen()}>
                <div class="px-4 pb-4 pt-1 space-y-3 border-t border-border">
                  <For each={params}>
                    {(meta) => (
                      <div class="border-b border-border/50 pb-3 last:border-b-0 last:pb-0">
                        <ParamField
                          meta={meta}
                          value={getByPath(props.config, meta.path)}
                          error={errorMap().get(meta.path)}
                          onChange={(v) => setValue(meta.path, v)}
                          compact
                        />
                      </div>
                    )}
                  </For>
                </div>
              </Show>
            </Card>
          );
        }}
      </For>
    </div>
  );
}
