import { For, createSignal, createMemo, Show } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Badge } from '@/components/ui/Badge';
import { ParamField } from './ParamField';
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

/** 分节配置：按 section 折叠，每节内一列 ParamField。 */
export function SectionPanel(props: SectionPanelProps) {
  const sections = Object.keys(PARAM_DICT);
  // path → 错误消息
  const errorMap = createMemo(() => new Map(props.errors.map((e) => [e.path, e.message])));
  // section → 错误数（预聚合，避免每渲染 filter）
  const errorsBySection = createMemo(() => {
    const m = new Map<string, number>();
    for (const e of props.errors) {
      const section = e.path.split('.')[0];
      m.set(section, (m.get(section) ?? 0) + 1);
    }
    return m;
  });

  const [openSection, setOpenSection] = createSignal<string | null>(sections[0] ?? null);

  function setValue(path: string, v: unknown) {
    const next = structuredClone(props.config);
    setByPath(next, path, v);
    props.onChange(next);
  }

  return (
    <div class="space-y-2">
      <For each={sections}>
        {(section) => {
          const params = PARAM_DICT[section];
          const isOpen = () => openSection() === section;
          const panelId = `amas-section-${section}`;
          const sectionErrors = () => errorsBySection().get(section) ?? 0;
          return (
            <Card variant="outlined" padding="none">
              <button
                type="button"
                class="focus-ring-soft w-full flex items-center justify-between px-4 py-3 hover:bg-surface-secondary/50 transition-colors"
                onClick={() => setOpenSection(isOpen() ? null : section)}
                aria-expanded={isOpen()}
                aria-controls={panelId}
              >
                <span class="flex items-center gap-2 text-sm font-semibold text-content">
                  {SECTION_LABEL_ZH[section] ?? section}
                  <span class="text-xs text-content-tertiary font-normal">{params.length} 项</span>
                  <Show when={sectionErrors() > 0}>
                    <Badge variant="error" size="sm">{sectionErrors()} 错</Badge>
                  </Show>
                </span>
                <svg
                  class={`w-4 h-4 text-content-tertiary transition-transform duration-200 ${isOpen() ? 'rotate-180' : ''}`}
                  viewBox="0 0 20 20"
                  fill="currentColor"
                  aria-hidden="true"
                >
                  <path
                    fill-rule="evenodd"
                    d="M5.23 7.21a.75.75 0 011.06.02L10 11.06l3.71-3.83a.75.75 0 111.08 1.04l-4.25 4.39a.75.75 0 01-1.08 0L5.21 8.27a.75.75 0 01.02-1.06z"
                    clip-rule="evenodd"
                  />
                </svg>
              </button>
              {/* 保留 DOM 以让 aria-controls 始终有效 */}
              <div id={panelId} hidden={!isOpen()} class="px-4 pb-4 pt-1 space-y-3 border-t border-border">
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
            </Card>
          );
        }}
      </For>
    </div>
  );
}
