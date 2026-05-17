import { Show, createSignal } from 'solid-js';
import { Switch } from '@/components/ui/Switch';
import { Badge } from '@/components/ui/Badge';
import { adminApi } from '@/api/admin';
import { uiStore } from '@/stores/ui';
import type { ParamMeta } from './schema';

// session 级缓存：同一参数只问一次 AI
const explainCache = new Map<string, string>();

interface ParamFieldProps {
  meta: ParamMeta;
  value: unknown;
  error?: string;
  onChange: (v: unknown) => void;
  compact?: boolean; // 紧凑模式：分节页用，去掉描述
}

/** 单个参数的输入控件：bool 用 Switch；数值用 number input + 可选 slider；附 default / tuned chip。 */
export function ParamField(props: ParamFieldProps) {
  const m = props.meta;
  const isBool = m.type === 'bool';
  const isNumber = m.type === 'number' || m.type === 'integer';
  const showSlider = () => isNumber && m.min != null && m.max != null;
  const numericValue = () => (typeof props.value === 'number' ? props.value : Number(m.default));
  const isChanged = () => !Object.is(props.value, m.default);

  const [explanation, setExplanation] = createSignal<string | null>(explainCache.get(m.path) ?? null);
  const [askingAi, setAskingAi] = createSignal(false);

  async function askAi() {
    if (askingAi()) return;
    if (explainCache.has(m.path)) {
      setExplanation(explainCache.get(m.path)!);
      return;
    }
    setAskingAi(true);
    try {
      const r = await adminApi.amasExplainParam(m.path, props.value ?? m.default);
      explainCache.set(m.path, r.explanation);
      setExplanation(r.explanation);
    } catch (err) {
      uiStore.toast.error('AI 解释失败', err instanceof Error ? err.message : '');
    } finally {
      setAskingAi(false);
    }
  }

  function handleNumberInput(raw: string) {
    if (raw === '') return; // 暂时容忍空，便于编辑
    const v = m.type === 'integer' ? parseInt(raw, 10) : parseFloat(raw);
    if (Number.isFinite(v)) props.onChange(v);
  }

  return (
    <div class="space-y-1.5">
      <div class="flex items-baseline justify-between gap-2">
        <div class="flex items-center gap-2 min-w-0">
          <label class="text-sm font-medium text-content truncate">{m.label_zh}</label>
          <Show when={isChanged()}>
            <span class="inline-flex h-1.5 w-1.5 rounded-full bg-accent" title="已修改" />
          </Show>
        </div>
        <div class="flex items-center gap-1.5 shrink-0">
          <Show when={m.sensitivity === 'ultra' || m.sensitivity === 'high'}>
            <Badge variant={m.sensitivity === 'ultra' ? 'warning' : 'info'} size="sm">
              {m.sensitivity === 'ultra' ? '高敏' : '中敏'}
            </Badge>
          </Show>
        </div>
      </div>

      <Show when={!props.compact}>
        <p class="text-xs text-content-tertiary leading-snug">{m.description_zh}</p>
      </Show>

      <Show when={isBool}>
        <Switch
          checked={Boolean(props.value)}
          onChange={(v) => props.onChange(v)}
          label={Boolean(props.value) ? '已启用' : '已停用'}
        />
      </Show>

      <Show when={isNumber}>
        <div class="flex items-center gap-2">
          <input
            type="number"
            class="w-28 h-9 px-2 rounded-md text-sm bg-surface border border-border focus:outline-none focus:ring-2 focus:ring-accent/30 focus:border-accent font-mono"
            min={m.min}
            max={m.max}
            step={m.step ?? (m.type === 'integer' ? 1 : 0.01)}
            value={numericValue()}
            onInput={(e) => handleNumberInput(e.currentTarget.value)}
          />
          <Show when={showSlider()}>
            <input
              type="range"
              class="flex-1 accent-accent"
              min={m.min}
              max={m.max}
              step={m.step ?? (m.type === 'integer' ? 1 : 0.01)}
              value={numericValue()}
              onInput={(e) => handleNumberInput(e.currentTarget.value)}
            />
          </Show>
        </div>
      </Show>

      <div class="flex items-center gap-2 flex-wrap text-xs">
        <button
          type="button"
          class="text-content-tertiary hover:text-accent transition-colors underline-offset-2 hover:underline"
          title="恢复为出厂默认"
          onClick={() => props.onChange(m.default)}
        >
          默认 {formatChip(m.default)}
        </button>
        <Show when={m.tuned_2026_05_15 !== undefined}>
          <button
            type="button"
            class="text-content-tertiary hover:text-success transition-colors underline-offset-2 hover:underline"
            title="应用 2026-05-15 已调优值"
            onClick={() => props.onChange(m.tuned_2026_05_15)}
          >
            已调优 {formatChip(m.tuned_2026_05_15!)}
          </button>
        </Show>
        <Show when={m.affects && m.affects.length > 0}>
          <span class="text-content-tertiary">影响：{m.affects!.map(zhAffect).join(' / ')}</span>
        </Show>
        <button
          type="button"
          class="text-content-tertiary hover:text-accent transition-colors"
          title="让 DeepSeek 用一句话解释这个参数"
          disabled={askingAi()}
          onClick={askAi}
        >
          {askingAi() ? '问 AI 中…' : '问 AI'}
        </button>
      </div>

      <Show when={explanation()}>
        <div class="text-xs text-content-secondary border-l-2 border-accent pl-2 py-1 bg-accent-light/20 rounded-r">
          🤖 {explanation()}
        </div>
      </Show>

      <Show when={props.error}>
        <p class="text-xs text-error" role="alert">{props.error}</p>
      </Show>
    </div>
  );
}

function formatChip(v: number | boolean): string {
  if (typeof v === 'boolean') return v ? '是' : '否';
  if (Number.isInteger(v)) return String(v);
  return v.toFixed(Math.abs(v) >= 1 ? 3 : 4);
}

function zhAffect(a: string): string {
  return ({
    retention: '留存',
    prediction: '预测',
    latency: '延迟',
    fatigue: '疲劳',
    engagement: '投入',
    exploration: '探索',
  } as Record<string, string>)[a] ?? a;
}
