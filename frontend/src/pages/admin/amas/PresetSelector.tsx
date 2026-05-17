import { createSignal, Show, For } from 'solid-js';
import { Button } from '@/components/ui/Button';
import { PRESETS, applyPreset, diffKnown, type PresetId } from './schema';

interface PresetSelectorProps {
  config: Record<string, unknown>;
  onApply: (next: Record<string, unknown>) => void;
}

/** 内置 preset 应用按钮 + diff 预览。 */
export function PresetSelector(props: PresetSelectorProps) {
  const [preview, setPreview] = createSignal<{ preset: PresetId; next: Record<string, unknown> } | null>(null);

  function startPreview(preset: PresetId) {
    const next = applyPreset(props.config, preset);
    setPreview({ preset, next });
  }

  function confirm() {
    const p = preview();
    if (!p) return;
    props.onApply(p.next);
    setPreview(null);
  }

  const diff = () => {
    const p = preview();
    if (!p) return [];
    return diffKnown(props.config, p.next);
  };

  return (
    <div class="flex items-center gap-2 flex-wrap">
      <span class="text-xs text-content-tertiary mr-1">Preset：</span>
      <For each={PRESETS}>
        {(p) => (
          <Button size="sm" variant="outline" onClick={() => startPreview(p.id)} title={p.description_zh}>
            {p.label_zh}
          </Button>
        )}
      </For>

      <Show when={preview()}>
        {(pv) => (
          <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4" onClick={() => setPreview(null)}>
            <div class="bg-surface-elevated rounded-xl shadow-2xl max-w-2xl w-full max-h-[80vh] flex flex-col" onClick={(e) => e.stopPropagation()}>
              <div class="px-5 py-4 border-b border-border">
                <h3 class="text-base font-semibold text-content">应用 Preset：{PRESETS.find((x) => x.id === pv().preset)!.label_zh}</h3>
                <p class="text-xs text-content-tertiary mt-1">以下字段将被覆盖（共 {diff().length} 项），其它字段保留</p>
              </div>
              <div class="flex-1 overflow-auto px-5 py-3">
                <Show when={diff().length > 0} fallback={<p class="text-sm text-content-tertiary text-center py-8">当前配置已与此 preset 一致，无变化</p>}>
                  <table class="w-full text-xs font-mono">
                    <thead>
                      <tr class="text-content-tertiary border-b border-border">
                        <th class="text-left py-2 pr-2 font-medium">字段</th>
                        <th class="text-right py-2 pr-2 font-medium">当前</th>
                        <th class="text-right py-2 font-medium">→ Preset</th>
                      </tr>
                    </thead>
                    <tbody>
                      <For each={diff()}>
                        {(d) => (
                          <tr class="border-b border-border/40">
                            <td class="py-1 pr-2 text-content-secondary">
                              <div class="text-content">{d.label_zh}</div>
                              <div class="text-[10px] text-content-tertiary">{d.path}</div>
                            </td>
                            <td class="py-1 pr-2 text-right text-content-tertiary">{formatVal(d.before)}</td>
                            <td class="py-1 text-right text-success">{formatVal(d.after)}</td>
                          </tr>
                        )}
                      </For>
                    </tbody>
                  </table>
                </Show>
              </div>
              <div class="px-5 py-3 border-t border-border flex justify-end gap-2">
                <Button size="sm" variant="ghost" onClick={() => setPreview(null)}>取消</Button>
                <Button size="sm" onClick={confirm} disabled={diff().length === 0}>应用到表单</Button>
              </div>
            </div>
          </div>
        )}
      </Show>
    </div>
  );
}

function formatVal(v: unknown): string {
  if (typeof v === 'boolean') return v ? 'true' : 'false';
  if (typeof v === 'number') return Number.isInteger(v) ? String(v) : v.toFixed(6).replace(/0+$/, '').replace(/\.$/, '');
  if (v === undefined) return '—';
  return JSON.stringify(v);
}
