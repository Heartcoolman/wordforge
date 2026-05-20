import { createSignal, Show, For, createEffect, onCleanup, createMemo } from 'solid-js';
import { Button } from '@/components/ui/Button';
import { PRESETS, applyPreset, diffKnown, type PresetId } from './schema';

interface PresetSelectorProps {
  config: Record<string, unknown>;
  onApply: (next: Record<string, unknown>) => void;
}

/** 内置 preset 应用按钮 + diff 预览。 */
export function PresetSelector(props: PresetSelectorProps) {
  const [preview, setPreview] = createSignal<{ preset: PresetId; next: Record<string, unknown> } | null>(null);
  let containerRef: HTMLDivElement | undefined;

  function startPreview(preset: PresetId) {
    const next = applyPreset(props.config, preset);
    setPreview({ preset, next });
  }

  function closePreview() {
    setPreview(null);
  }

  function confirm() {
    const p = preview();
    if (!p) return;
    props.onApply(p.next);
    setPreview(null);
  }

  const diff = createMemo(() => {
    const p = preview();
    if (!p) return [];
    return diffKnown(props.config, p.next);
  });

  // a11y：ESC 关闭 + body 锁滚动 + 首焦点 + Tab focus trap
  createEffect(() => {
    if (!preview()) return;
    const prevOverflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';

    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') {
        e.preventDefault();
        closePreview();
        return;
      }
      if (e.key === 'Tab' && containerRef) {
        const focusables = containerRef.querySelectorAll<HTMLElement>(
          'button:not([disabled]), input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])',
        );
        if (focusables.length === 0) return;
        const first = focusables[0];
        const last = focusables[focusables.length - 1];
        if (e.shiftKey && document.activeElement === first) {
          e.preventDefault();
          last.focus();
        } else if (!e.shiftKey && document.activeElement === last) {
          e.preventDefault();
          first.focus();
        }
      }
    }
    document.addEventListener('keydown', onKey);

    requestAnimationFrame(() => {
      containerRef?.querySelector<HTMLElement>('button:not([aria-label="关闭"]),input,select')?.focus();
    });

    onCleanup(() => {
      document.removeEventListener('keydown', onKey);
      document.body.style.overflow = prevOverflow;
    });
  });

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
          <div
            class="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4"
            onClick={closePreview}
            role="dialog"
            aria-modal="true"
            aria-labelledby="preset-preview-title"
          >
            <div
              ref={containerRef}
              class="bg-surface-elevated rounded-xl shadow-2xl max-w-2xl w-full max-h-[80vh] flex flex-col"
              onClick={(e) => e.stopPropagation()}
            >
              <div class="px-5 py-4 border-b border-border">
                <h3 id="preset-preview-title" class="text-base font-semibold text-content">
                  应用 Preset：{PRESETS.find((x) => x.id === pv().preset)!.label_zh}
                </h3>
                <p class="text-xs text-content-tertiary mt-1">以下字段将被覆盖（共 {diff().length} 项），其它字段保留</p>
              </div>
              <div class="flex-1 overflow-auto px-5 py-3">
                <Show
                  when={diff().length > 0}
                  fallback={<p class="text-sm text-content-tertiary text-center py-8">当前配置已与此 preset 一致，无变化</p>}
                >
                  <div class="overflow-x-auto">
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
                  </div>
                </Show>
              </div>
              <div class="px-5 py-3 border-t border-border flex justify-end gap-2">
                <Button size="sm" variant="ghost" onClick={closePreview}>
                  {diff().length === 0 ? '关闭' : '取消'}
                </Button>
                <Show when={diff().length > 0}>
                  <Button size="sm" onClick={confirm}>应用到表单</Button>
                </Show>
                <Show when={diff().length === 0}>
                  <span class="text-xs text-content-tertiary self-center">已一致</span>
                </Show>
              </div>
            </div>
          </div>
        )}
      </Show>
    </div>
  );
}

export function formatVal(v: unknown): string {
  if (typeof v === 'boolean') return v ? 'true' : 'false';
  if (typeof v === 'number') return Number.isInteger(v) ? String(v) : v.toFixed(6).replace(/0+$/, '').replace(/\.$/, '');
  if (v === undefined) return '—';
  return JSON.stringify(v);
}
