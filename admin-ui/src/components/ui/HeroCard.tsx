import { type ParentProps, Show, For } from 'solid-js';
import { cn } from '@/utils/cn';
import { useCountUp } from '@/lib/motion';

interface HeroMeta {
  /** 数值（number 走 count-up；string 直显） */
  value: number | string;
  /** 单位（如 "用户"、"%"），紧跟在数值后面，灰色小号 */
  unit?: string;
  /** 标签，灰色 caption 风格 */
  label: string;
  /** 自定义格式化（仅 value 是 number 生效），默认 toLocaleString */
  format?: (v: number) => string;
}

interface HeroCardProps extends ParentProps {
  /** 顶部 chip 文案，如 "管理后台 · 提案版" / "运行正常"。可选 */
  eyebrow?: string;
  /** chip 颜色，默认 accent */
  eyebrowVariant?: 'accent' | 'success' | 'warning' | 'error' | 'info';
  /** 主标题，可包含 <em> 高亮（accent 渐变） */
  title?: string;
  /** 副描述，max-width 60ch */
  desc?: string;
  /** 4-6 个指标格，hero 底部 meta-grid 渲染。空数组时不渲染该区 */
  meta?: HeroMeta[];
  /** 右侧 / 顶部 CTA 区（按钮组等），children 优先级高于 meta */
  cta?: any;
  class?: string;
}

const eyebrowVariants = {
  accent: 'bg-accent-light text-accent',
  success: 'bg-success-light text-success-strong',
  warning: 'bg-warning-light text-warning-strong',
  error: 'bg-error-light text-error-strong',
  info: 'bg-info-light text-info-strong',
} as const;

const dotColors = {
  accent: 'var(--accent)',
  success: 'var(--success)',
  warning: 'var(--warning)',
  error: 'var(--error)',
  info: 'var(--info)',
} as const;

/**
 * 页面顶部 hero 卡。
 *
 * 视觉：surface-elevated + radius-2xl + elevation-2，
 * 顶部 eyebrow chip + display 标题 + desc + meta-grid。
 *
 * @example
 *   <HeroCard
 *     eyebrow="运行正常"
 *     eyebrowVariant="success"
 *     title="今日学习概况"
 *     desc="过去 24 小时 AMAS 决策与学习产出汇总。"
 *     meta={[
 *       { value: 1284, label: 'DAU', unit: '人' },
 *       { value: 4283, label: '答题数' },
 *       { value: 95.4, label: '准确率', unit: '%' },
 *       { value: '99.95%', label: 'SLO' },
 *     ]}
 *   />
 */
export function HeroCard(props: HeroCardProps) {
  return (
    <section
      class={cn(
        'relative overflow-hidden rounded-2xl bg-surface-elevated shadow-elevation-2',
        'px-8 py-7 md:px-11 md:py-9',
        'animate-fade-in-up',
        props.class,
      )}
    >
      {/* eyebrow */}
      <Show when={props.eyebrow}>
        <span
          class={cn(
            'relative inline-flex items-center gap-2 rounded-full px-2.5 py-1',
            'text-[11.5px] font-semibold tracking-wide',
            eyebrowVariants[props.eyebrowVariant ?? 'accent'],
          )}
        >
          <span
            class="inline-block size-1.5 rounded-full"
            style={{ background: dotColors[props.eyebrowVariant ?? 'accent'] }}
          />
          {props.eyebrow}
        </span>
      </Show>

      {/* title */}
      <Show when={props.title}>
        {(t) => (
          <h1
            class="relative mt-3.5 mb-3 max-w-[24ch] text-[clamp(28px,3.4vw,44px)] font-bold leading-[1.08] tracking-[-0.028em]"
            // 允许 title 含 <em>...</em> 走 accent 纯色高亮（不允许其他 HTML，简单字符串安全）
            // eslint-disable-next-line solid/no-innerhtml
            innerHTML={t().replace(
              /<em>(.*?)<\/em>/g,
              '<em class="not-italic" style="color:var(--solid)">$1</em>',
            )}
          />
        )}
      </Show>

      {/* desc */}
      <Show when={props.desc}>
        <p class="relative max-w-[60ch] text-[15px] leading-[1.55] text-content-secondary">{props.desc}</p>
      </Show>

      {/* CTA / children */}
      <Show when={props.cta || props.children}>
        <div class="relative mt-5 flex flex-wrap items-center gap-2.5">
          {props.cta}
          {props.children}
        </div>
      </Show>

      {/* meta grid */}
      <Show when={props.meta && props.meta.length > 0}>
        <div
          class="relative mt-7 grid gap-px overflow-hidden rounded-xl"
          style={{
            background: 'var(--border-hairline)',
            'grid-template-columns': `repeat(${props.meta!.length}, minmax(0, 1fr))`,
          }}
        >
          <For each={props.meta}>{(m) => <HeroMetaCell {...m} />}</For>
        </div>
      </Show>
    </section>
  );
}

function HeroMetaCell(props: HeroMeta) {
  // 仅 number 走 count-up；string 直显（如 "99.95%" / "oklch"）
  const isNumber = () => typeof props.value === 'number';
  const fmt = props.format ?? ((v: number) => v.toLocaleString('en-US'));
  const animated = useCountUp({
    to: () => (typeof props.value === 'number' ? props.value : 0),
    format: fmt,
  });

  return (
    <div class="bg-surface-elevated px-4 py-3.5">
      <div class="text-[22px] font-semibold leading-[1.2] tracking-[-0.02em] tabular-nums">
        {isNumber() ? animated() : (props.value as string)}
        <Show when={props.unit}>
          <span class="ml-1 text-xs font-medium text-content-tertiary">{props.unit}</span>
        </Show>
      </div>
      <div class="mt-0.5 text-[11.5px] uppercase tracking-wide text-content-tertiary">{props.label}</div>
    </div>
  );
}
