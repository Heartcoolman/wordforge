import { createMemo, createSignal, For, Show } from 'solid-js';
import { Button } from '@/components/ui/Button';
import { Badge } from '@/components/ui/Badge';
import { PRESETS, applyPreset, diffKnown, type PresetId } from '@/pages/amas/schema';
import { cn } from '@/utils/cn';

interface PresetBarProps {
  config: Record<string, unknown>;
  /** 服务端拉回的 baseline,用于"回滚 baseline" */
  baseline: Record<string, unknown>;
  onApply: (next: Record<string, unknown>) => void;
}

/** UI 层 preset 清单(对齐 amas-config.html .preset-bar 设计图 5 项)。
 *  schema 实际只落了 2 个真实 preset;另 3 个保留 disabled 占位作为路线图。 */
interface UiPreset {
  id: PresetId | 'conservative' | 'aggressive' | 'heuristic_only';
  label: string;
  implemented: boolean;
}

const UI_PRESETS: UiPreset[] = [
  { id: 'factory_default', label: '出厂默认 (factory_default)', implemented: true },
  { id: 'conservative', label: '保守 (conservative)', implemented: false },
  { id: 'aggressive', label: '激进 (aggressive)', implemented: false },
  { id: 'heuristic_only', label: '仅启发式 (heuristic-only)', implemented: false },
];

/**
 * 预设条:展示当前 preset 状态 + 切换下拉 + 3 actions。
 *
 * 对齐 amas-config.html `.preset-bar`:
 * - 当前预设 chip(自动匹配,无法匹配显示"自定义草稿")
 * - select 5 项(2 真 + 3 占位 disabled)
 * - 另存为预设…(占位)/ 重置到该预设 / 回滚到 baseline 三 actions
 *
 * 替代旧 PresetSelector 的按钮组 + modal preview 风格。
 */
export function PresetBar(props: PresetBarProps) {
  const [selected, setSelected] = createSignal<UiPreset['id']>('factory_default');

  /** 当前 config 是否完全匹配某个 preset。未匹配返回 null(=自定义草稿)。 */
  const matchedPreset = createMemo<PresetId | null>(() => {
    for (const p of PRESETS) {
      const fresh = applyPreset({}, p.id);
      const diff = diffKnown(fresh, props.config);
      if (diff.length === 0) return p.id;
    }
    return null;
  });

  const currentLabel = createMemo(() => {
    const m = matchedPreset();
    if (m === 'factory_default') return '出厂默认';
    return '自定义草稿';
  });

  function applySelected() {
    const sel = UI_PRESETS.find((p) => p.id === selected());
    if (!sel || !sel.implemented) return;
    const next = applyPreset(props.config, sel.id as PresetId);
    props.onApply(next);
  }

  function rollbackToBaseline() {
    props.onApply(structuredClone(props.baseline));
  }

  return (
    <div
      class={cn(
        'flex flex-wrap items-center gap-3 rounded-lg bg-surface-elevated px-4 py-3 shadow-elevation-1',
      )}
    >
      <span class="text-[11px] font-mono uppercase tracking-wide text-content-tertiary">
        当前预设
      </span>
      <div class="flex items-center gap-2">
        <span class="text-sm font-semibold text-content">{currentLabel()}</span>
        <Show when={matchedPreset() === null}>
          <Badge variant="warning" size="sm">
            草稿
          </Badge>
        </Show>
      </div>

      <label class="sr-only" for="amas-preset-select">
        切换预设
      </label>
      <select
        id="amas-preset-select"
        class={cn(
          'h-8 rounded border border-border bg-surface px-2 text-sm text-content',
          'focus-ring-soft transition-colors',
        )}
        value={selected()}
        onChange={(e) => setSelected(e.currentTarget.value as UiPreset['id'])}
      >
        <For each={UI_PRESETS}>
          {(p) => (
            <option value={p.id} disabled={!p.implemented}>
              {p.label}
              {!p.implemented ? ' (尚未实施)' : ''}
            </option>
          )}
        </For>
      </select>

      <div class="ml-auto flex flex-wrap gap-1.5">
        <Button
          size="sm"
          variant="ghost"
          disabled
          title="另存为自定义预设 — 尚未实施"
        >
          另存为预设
        </Button>
        <Button
          size="sm"
          variant="ghost"
          onClick={applySelected}
          disabled={!UI_PRESETS.find((p) => p.id === selected())?.implemented}
        >
          重置到该预设
        </Button>
        <Button
          size="sm"
          variant="ghost"
          onClick={rollbackToBaseline}
          class="hover:bg-error-light hover:text-error-strong"
          title="回滚到服务端 baseline(放弃所有未保存修改)"
        >
          回滚 baseline
        </Button>
      </div>
    </div>
  );
}
