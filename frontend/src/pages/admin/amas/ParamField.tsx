import { Show, createSignal, createEffect, createMemo } from 'solid-js';
import { Switch } from '@/components/ui/Switch';
import { Badge } from '@/components/ui/Badge';
import { Input } from '@/components/ui/Input';
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
  // 不解构 props.meta，保留响应性
  const isBool = () => props.meta.type === 'bool';
  const isNumber = () => props.meta.type === 'number' || props.meta.type === 'integer';
  const showSlider = () => isNumber() && props.meta.min != null && props.meta.max != null;
  const numericValue = () =>
    typeof props.value === 'number' ? props.value : Number(props.meta.default);
  const isChanged = () => !Object.is(props.value, props.meta.default);

  const [explanation, setExplanation] = createSignal<string | null>(null);
  const [askingAi, setAskingAi] = createSignal(false);

  // 缓存按当前 path 读取；path 变了重新读
  createEffect(() => {
    const cached = explainCache.get(props.meta.path);
    setExplanation(cached ?? null);
  });

  async function askAi() {
    if (askingAi()) return;
    const path = props.meta.path;
    if (explainCache.has(path)) {
      setExplanation(explainCache.get(path)!);
      return;
    }
    setAskingAi(true);
    try {
      const r = await adminApi.amasExplainParam(path, props.value ?? props.meta.default);
      explainCache.set(path, r.explanation);
      setExplanation(r.explanation);
    } catch (err) {
      uiStore.toast.error('AI 解释失败', err instanceof Error ? err.message : '');
    } finally {
      setAskingAi(false);
    }
  }

  function handleNumberInput(target: HTMLInputElement) {
    const raw = target.value;
    if (raw === '') return; // 暂时容忍空，便于编辑
    const v = props.meta.type === 'integer' ? parseInt(raw, 10) : parseFloat(raw);
    if (!Number.isFinite(v)) return;
    // clamp 校验：超界打 setCustomValidity，不立即写入
    if (props.meta.min != null && v < props.meta.min) {
      target.setCustomValidity(`小于最小值 ${props.meta.min}`);
      target.reportValidity();
      return;
    }
    if (props.meta.max != null && v > props.meta.max) {
      target.setCustomValidity(`大于最大值 ${props.meta.max}`);
      target.reportValidity();
      return;
    }
    target.setCustomValidity('');
    props.onChange(v);
  }

  const switchLabel = createMemo(() => (Boolean(props.value) ? '已启用' : '已停用'));

  return (
    <div class="space-y-1.5">
      <div class="flex items-baseline justify-between gap-2">
        <div class="flex items-center gap-2 min-w-0">
          <label class="text-sm font-medium text-content truncate">{props.meta.label_zh}</label>
          <Show when={isChanged()}>
            <span class="inline-flex h-1.5 w-1.5 rounded-full bg-accent" title="已修改" />
          </Show>
        </div>
        <div class="flex items-center gap-1.5 shrink-0">
          <Show when={props.meta.sensitivity === 'ultra' || props.meta.sensitivity === 'high'}>
            <Badge variant={props.meta.sensitivity === 'ultra' ? 'warning' : 'info'} size="sm">
              {props.meta.sensitivity === 'ultra' ? '高敏' : '中敏'}
            </Badge>
          </Show>
        </div>
      </div>

      <Show when={!props.compact}>
        <p class="text-xs text-content-tertiary leading-snug">{props.meta.description_zh}</p>
      </Show>

      <Show when={isBool()}>
        <div class="flex items-center gap-2">
          <Switch
            checked={Boolean(props.value)}
            onChange={(v) => props.onChange(v)}
          />
          {/* 固宽防止"已启用 / 已停用"切换抖动 */}
          <span class="text-sm text-content inline-block min-w-[3em]">{switchLabel()}</span>
        </div>
      </Show>

      <Show when={isNumber()}>
        <div class="flex items-center gap-2 flex-wrap min-w-0">
          <Input
            type="number"
            class="w-28 h-9 font-mono"
            min={props.meta.min}
            max={props.meta.max}
            step={props.meta.step ?? (props.meta.type === 'integer' ? 1 : 0.01)}
            value={numericValue()}
            aria-label={`${props.meta.label_zh}（数值输入）`}
            onInput={(e) => handleNumberInput(e.currentTarget)}
          />
          <Show when={showSlider()}>
            <input
              type="range"
              class="flex-1 min-w-0 h-2 rounded-full appearance-none cursor-pointer bg-surface-secondary accent-accent focus-ring-soft
                     [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:w-4 [&::-webkit-slider-thumb]:h-4
                     [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-accent
                     [&::-webkit-slider-thumb]:shadow [&::-webkit-slider-thumb]:transition-transform
                     [&::-webkit-slider-thumb]:hover:scale-110
                     [&::-moz-range-thumb]:w-4 [&::-moz-range-thumb]:h-4 [&::-moz-range-thumb]:rounded-full
                     [&::-moz-range-thumb]:bg-accent [&::-moz-range-thumb]:border-0"
              min={props.meta.min}
              max={props.meta.max}
              step={props.meta.step ?? (props.meta.type === 'integer' ? 1 : 0.01)}
              value={numericValue()}
              aria-label={`${props.meta.label_zh}（滑块）`}
              // 拖动结束才触发，避免 295 字段全量克隆抖动
              onChange={(e) => handleNumberInput(e.currentTarget)}
            />
          </Show>
        </div>
      </Show>

      {/* 元信息分两行：主行（默认 / 已调优 / 问 AI）+ 影响行；窄屏不挤压 */}
      <div class="space-y-1.5">
        <div class="flex items-center gap-2 flex-wrap text-xs">
          <button
            type="button"
            class="focus-ring-soft px-1.5 py-0.5 rounded -mx-1 text-content-tertiary hover:text-accent transition-colors underline-offset-2 hover:underline"
            title="恢复为出厂默认"
            aria-label={`恢复 ${props.meta.label_zh} 为出厂默认值 ${formatChip(props.meta.default)}`}
            onClick={() => props.onChange(props.meta.default)}
          >
            默认 {formatChip(props.meta.default)}
          </button>
          <Show when={props.meta.tuned_2026_05_15 !== undefined}>
            <button
              type="button"
              class="focus-ring-soft px-1.5 py-0.5 rounded -mx-1 text-content-tertiary hover:text-success transition-colors underline-offset-2 hover:underline"
              title="应用 2026-05-15 已调优值"
              aria-label={`应用 ${props.meta.label_zh} 的 2026-05-15 调优值 ${formatChip(props.meta.tuned_2026_05_15!)}`}
              onClick={() => props.onChange(props.meta.tuned_2026_05_15)}
            >
              已调优 {formatChip(props.meta.tuned_2026_05_15!)}
            </button>
          </Show>
          <button
            type="button"
            class="focus-ring-soft px-1.5 py-0.5 rounded -mx-1 text-content-tertiary hover:text-accent transition-colors disabled:opacity-60 disabled:cursor-not-allowed"
            aria-label={`让 AI 用一句话解释 ${props.meta.label_zh}`}
            disabled={askingAi()}
            onClick={askAi}
          >
            {askingAi() ? '问 AI 中…' : '问 AI'}
          </button>
        </div>
        <Show when={props.meta.affects && props.meta.affects.length > 0}>
          <div class="text-xs text-content-tertiary">
            影响：{props.meta.affects!.map(zhAffect).join(' / ')}
          </div>
        </Show>
      </div>

      <Show when={explanation()}>
        <div class="text-xs text-content-secondary border-l-2 border-accent pl-2 py-1 bg-accent-light/20 rounded-r flex items-start gap-1.5">
          <svg
            class="w-3.5 h-3.5 mt-0.5 shrink-0 text-accent"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <rect x="3" y="11" width="18" height="10" rx="2" />
            <circle cx="12" cy="5" r="2" />
            <path d="M12 7v4" />
            <line x1="8" y1="16" x2="8" y2="16" />
            <line x1="16" y1="16" x2="16" y2="16" />
          </svg>
          <span>
            <span class="font-medium text-content">AI：</span>
            {explanation()}
          </span>
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
