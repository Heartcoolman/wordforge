import { splitProps, Show, For, createEffect, onCleanup, type JSX } from 'solid-js';
import { Icon } from './Icon';
import { Sparkline } from './charts';
import { sx, cx } from './sx';

/* ---------- Buttons ---------- */
export function Btn(props: {
  variant?: 'primary' | 'secondary' | 'ghost' | 'danger' | 'success' | 'warning' | 'outline';
  size?: 'xs' | 'sm' | 'md' | 'lg';
  icon?: string;
  children?: JSX.Element;
  class?: string;
} & JSX.ButtonHTMLAttributes<HTMLButtonElement>) {
  const [local, rest] = splitProps(props, ['variant', 'size', 'icon', 'children', 'class']);
  return (
    <button
      class={cx('btn', `btn-${local.variant ?? 'secondary'}`, local.size && local.size !== 'md' ? `btn-${local.size}` : '', local.class)}
      {...rest}
    >
      <Show when={local.icon}>{(n) => <Icon name={n()} size={local.size === 'xs' ? 12 : 15} />}</Show>
      {local.children}
    </button>
  );
}

export function IconBtn(props: {
  name: string; title?: string; class?: string; size?: number;
} & JSX.ButtonHTMLAttributes<HTMLButtonElement>) {
  const [local, rest] = splitProps(props, ['name', 'title', 'class', 'size']);
  return (
    <button class={cx('btn btn-ghost btn-icon', local.class)} title={local.title} aria-label={local.title} {...rest}>
      <Icon name={local.name} size={local.size ?? 17} />
    </button>
  );
}

/* ---------- Badge ---------- */
export function Badge(props: {
  variant?: 'default' | 'accent' | 'success' | 'warning' | 'error' | 'info';
  dot?: boolean; children?: JSX.Element; style?: JSX.CSSProperties | string;
}) {
  return (
    <span class={cx('badge', `badge-${props.variant ?? 'default'}`)} style={props.style}>
      <Show when={props.dot}><span class="dot" /></Show>
      {props.children}
    </span>
  );
}

/* ---------- Card ---------- */
export function Card(props: {
  children?: JSX.Element; class?: string; pad?: boolean; hover?: boolean; style?: JSX.CSSProperties | string;
} & JSX.HTMLAttributes<HTMLDivElement>) {
  const [local, rest] = splitProps(props, ['children', 'class', 'pad', 'hover', 'style']);
  return (
    <div
      class={cx('card', (local.pad ?? true) && 'card-pad', local.hover && 'card-hover', local.class)}
      style={local.style}
      {...rest}
    >
      {local.children}
    </div>
  );
}

/* ---------- Field ---------- */
export function Field(props: { label?: string; hint?: string; children?: JSX.Element }) {
  return (
    <label style={sx({ display: 'block' })}>
      <Show when={props.label}><span class="field-label">{props.label}</span></Show>
      {props.children}
      <Show when={props.hint}><span class="field-hint">{props.hint}</span></Show>
    </label>
  );
}

/* ---------- Switch ---------- */
export function Switch(props: { checked?: boolean; onChange?: (v: boolean) => void; disabled?: boolean }) {
  return (
    <label class="switch">
      <input
        type="checkbox"
        checked={props.checked}
        disabled={props.disabled}
        onChange={(e) => props.onChange?.(e.currentTarget.checked)}
      />
      <span class="track" />
      <span class="thumb" />
    </label>
  );
}

/* ---------- Tabs / Seg ---------- */
export function Tabs(props: {
  tabs: { value: string; label: string; count?: number | null }[];
  value: string; onChange: (v: string) => void;
}) {
  return (
    <div class="tabs">
      <For each={props.tabs}>
        {(t) => (
          <button class={cx('tab', props.value === t.value && 'active')} onClick={() => props.onChange(t.value)}>
            {t.label}
            <Show when={t.count != null}>
              <span class="mono" style={sx({ marginLeft: 6, opacity: 0.7, fontSize: 11 })}>{t.count}</span>
            </Show>
          </button>
        )}
      </For>
    </div>
  );
}

export function Seg(props: {
  options: (string | { value: string; label: string })[]; value: string; onChange: (v: string) => void;
}) {
  return (
    <div class="tabs">
      <For each={props.options}>
        {(o) => {
          const v = typeof o === 'object' ? o.value : o;
          const l = typeof o === 'object' ? o.label : o;
          return <button class={cx('tab', props.value === v && 'active')} onClick={() => props.onChange(v)}>{l}</button>;
        }}
      </For>
    </div>
  );
}

/* ---------- Empty / Skeleton / Spinner ---------- */
export function Empty(props: { title?: string; desc?: string; icon?: string; action?: JSX.Element }) {
  return (
    <div class="fade-in" style={sx({ textAlign: 'center', padding: '40px 20px', color: 'var(--text-3)' })}>
      <div style={sx({ width: 46, height: 46, margin: '0 auto 12px', borderRadius: 14, display: 'grid', placeItems: 'center', background: 'var(--surface-sunken)' })}>
        <Icon name={props.icon ?? 'inbox'} size={22} />
      </div>
      <div style={sx({ fontWeight: 600, color: 'var(--text-2)', fontSize: 13.5 })}>{props.title}</div>
      <Show when={props.desc}>
        <div style={sx({ fontSize: 12.5, marginTop: 4, maxWidth: 340, marginInline: 'auto' })}>{props.desc}</div>
      </Show>
      <Show when={props.action}><div style={sx({ marginTop: 14 })}>{props.action}</div></Show>
    </div>
  );
}

export function Skel(props: { h?: number; w?: number | string; r?: number; style?: JSX.CSSProperties | string }) {
  return <div class="skel" style={sx({ height: props.h ?? 16, width: props.w ?? '100%', borderRadius: props.r ?? 8 })} />;
}

export function Spinner(props: { size?: number }) {
  return <div class="spinner" style={sx({ width: props.size ?? 22, height: props.size ?? 22 })} />;
}

export function Loading(props: { h?: number }) {
  return <div style={sx({ display: 'grid', placeItems: 'center', minHeight: props.h ?? 200 })}><Spinner /></div>;
}

/* ---------- Modal & Drawer ---------- */
const MODAL_WIDTHS = { sm: 420, md: 560, lg: 720, xl: 920 };

export function Modal(props: {
  open: boolean; onClose: () => void; title?: string; children?: JSX.Element; footer?: JSX.Element;
  size?: 'sm' | 'md' | 'lg' | 'xl';
}) {
  createEffect(() => {
    if (!props.open) return;
    const h = (e: KeyboardEvent) => e.key === 'Escape' && props.onClose();
    window.addEventListener('keydown', h);
    onCleanup(() => window.removeEventListener('keydown', h));
  });
  return (
    <Show when={props.open}>
      <div
        class="fade-in"
        style={sx({ position: 'fixed', inset: 0, zIndex: 1000, display: 'grid', placeItems: 'center', padding: 20, background: 'rgba(0,0,0,0.5)' })}
        onMouseDown={(e) => e.target === e.currentTarget && props.onClose()}
      >
        <div
          class="card scale-in"
          style={sx({ width: '100%', maxWidth: MODAL_WIDTHS[props.size ?? 'md'], maxHeight: '88vh', display: 'flex', flexDirection: 'column', padding: 0, boxShadow: 'var(--shadow-3)', background: 'var(--surface-solid)', backdropFilter: 'none' })}
        >
          <div style={sx({ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '15px 20px', borderBottom: '1px solid var(--hairline)' })}>
            <h3 style={sx({ margin: 0, fontSize: 15.5, fontWeight: 700 })}>{props.title}</h3>
            <IconBtn name="x" title="关闭" onClick={() => props.onClose()} />
          </div>
          <div style={sx({ padding: 20, overflowY: 'auto' })}>{props.children}</div>
          <Show when={props.footer}>
            <div style={sx({ display: 'flex', gap: 9, justifyContent: 'flex-end', padding: '14px 20px', borderTop: '1px solid var(--hairline)' })}>{props.footer}</div>
          </Show>
        </div>
      </div>
    </Show>
  );
}

export function Drawer(props: {
  open: boolean; onClose: () => void; title?: string; subtitle?: string; children?: JSX.Element; footer?: JSX.Element; width?: number;
}) {
  createEffect(() => {
    if (!props.open) return;
    const h = (e: KeyboardEvent) => e.key === 'Escape' && props.onClose();
    window.addEventListener('keydown', h);
    onCleanup(() => window.removeEventListener('keydown', h));
  });
  return (
    <Show when={props.open}>
      <div
        class="fade-in"
        style={sx({ position: 'fixed', inset: 0, zIndex: 1000, background: 'rgba(0,0,0,0.5)' })}
        onMouseDown={(e) => e.target === e.currentTarget && props.onClose()}
      >
        <div
          style={sx({ position: 'absolute', top: 0, right: 0, height: '100%', width: '100%', maxWidth: props.width ?? 480, background: 'var(--surface-solid)', borderLeft: '1px solid var(--border)', boxShadow: 'var(--shadow-3)', display: 'flex', flexDirection: 'column', animation: 'wf-slideL .28s var(--ease)' })}
        >
          <div style={sx({ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', padding: '16px 20px', borderBottom: '1px solid var(--hairline)' })}>
            <div style={sx({ minWidth: 0 })}>
              <h3 style={sx({ margin: 0, fontSize: 16, fontWeight: 700 })}>{props.title}</h3>
              <Show when={props.subtitle}><div class="muted mono" style={sx({ fontSize: 12, marginTop: 3 })}>{props.subtitle}</div></Show>
            </div>
            <IconBtn name="x" title="关闭" onClick={() => props.onClose()} />
          </div>
          <div style={sx({ padding: 20, overflowY: 'auto', flex: 1 })}>{props.children}</div>
          <Show when={props.footer}>
            <div style={sx({ display: 'flex', gap: 9, justifyContent: 'flex-end', padding: '14px 20px', borderTop: '1px solid var(--hairline)' })}>{props.footer}</div>
          </Show>
        </div>
      </div>
    </Show>
  );
}

export function Confirm(props: {
  open: boolean; onClose: () => void; onConfirm: () => void; title?: string; body?: JSX.Element;
  danger?: boolean; confirmText?: string; loading?: boolean;
}) {
  return (
    <Modal
      open={props.open} onClose={props.onClose} title={props.title} size="sm"
      footer={
        <>
          <Btn variant="ghost" onClick={() => props.onClose()}>取消</Btn>
          <Btn variant={props.danger ? 'danger' : 'primary'} onClick={() => props.onConfirm()} disabled={props.loading}>
            {props.loading ? '处理中…' : (props.confirmText ?? '确认')}
          </Btn>
        </>
      }
    >
      <div class="muted" style={sx({ fontSize: 13.5, lineHeight: 1.6 })}>{props.body}</div>
    </Modal>
  );
}

/* ---------- Stat card ---------- */
export function StatCard(props: {
  tone?: 'accent' | 'success' | 'info' | 'warning' | 'error';
  label: string; value: JSX.Element | string | number; unit?: string; icon?: string;
  delta?: number | null; deltaLabel?: string; spark?: number[]; sparkColor?: string;
}) {
  const toneCol = () => ({ accent: 'var(--accent)', success: 'var(--success)', info: 'var(--info)', warning: 'var(--warning)', error: 'var(--error)' }[props.tone ?? 'accent']);
  const up = () => props.delta != null && props.delta >= 0;
  return (
    <Card hover style={sx({ overflow: 'hidden', display: 'flex', flexDirection: 'column', gap: 10, minHeight: 116 })}>
      <div style={sx({ display: 'flex', alignItems: 'center', justifyContent: 'space-between' })}>
        <span style={sx({ display: 'inline-flex', alignItems: 'center', gap: 8, fontSize: 12.5, fontWeight: 600, color: 'var(--text-2)' })}>
          <Show when={props.icon}>
            <span style={sx({ display: 'grid', placeItems: 'center', width: 26, height: 26, borderRadius: 8, background: `color-mix(in srgb, ${toneCol()} 12%, transparent)`, color: toneCol() })}>
              <Icon name={props.icon!} size={15} />
            </span>
          </Show>
          {props.label}
        </span>
        <Show when={props.delta != null}>
          <Badge variant={up() ? 'success' : 'error'}>{up() ? '▲' : '▼'} {Math.abs(props.delta!).toFixed(1)}%</Badge>
        </Show>
      </div>
      <div style={sx({ display: 'flex', alignItems: 'baseline', gap: 4 })}>
        <span class="tnum" style={sx({ fontSize: 30, fontWeight: 800, letterSpacing: '-0.03em', lineHeight: 1 })}>{props.value}</span>
        <Show when={props.unit}><span class="muted" style={sx({ fontSize: 15, fontWeight: 600 })}>{props.unit}</span></Show>
      </div>
      <div style={sx({ display: 'flex', alignItems: 'flex-end', justifyContent: 'space-between', gap: 8, marginTop: 'auto' })}>
        <Show when={props.deltaLabel} fallback={<span />}>
          <span class="muted-3" style={sx({ fontSize: 11.5 })}>{props.deltaLabel}</span>
        </Show>
        <Show when={props.spark && props.spark.length > 1}>
          <Sparkline data={props.spark!} color={props.sparkColor ?? toneCol()} width={88} height={30} />
        </Show>
      </div>
    </Card>
  );
}

/* ---------- Panel ---------- */
export function Panel(props: {
  title?: string; sub?: string; right?: JSX.Element; children?: JSX.Element; class?: string; style?: JSX.CSSProperties | string;
}) {
  return (
    <Card class={props.class} style={typeof props.style === 'object' ? { display: 'flex', 'flex-direction': 'column', ...props.style } : props.style}>
      <div style={sx({ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', gap: 10, paddingBottom: 13, marginBottom: 14, borderBottom: '1px solid var(--hairline)' })}>
        <div style={sx({ minWidth: 0 })}>
          <h2 class="section-title">{props.title}</h2>
          <Show when={props.sub}><div class="muted-3 mono" style={sx({ fontSize: 11.5, marginTop: 3 })}>{props.sub}</div></Show>
        </div>
        {props.right}
      </div>
      <div style={sx({ flex: 1, minWidth: 0 })}>{props.children}</div>
    </Card>
  );
}

/* ---------- Progress ---------- */
export function Progress(props: {
  value: number; max?: number; tone?: 'accent' | 'success' | 'warning' | 'error' | 'info'; height?: number; showLabel?: boolean;
}) {
  const pct = () => Math.min(100, (props.value / (props.max ?? 100)) * 100);
  const col = () => ({ accent: 'var(--accent)', success: 'var(--success)', warning: 'var(--warning)', error: 'var(--error)', info: 'var(--info)' }[props.tone ?? 'accent']);
  return (
    <div style={sx({ display: 'flex', alignItems: 'center', gap: 8 })}>
      <div class="bar" style={sx({ height: props.height ?? 7, flex: 1 })}><i style={sx({ width: pct() + '%', background: col() })} /></div>
      <Show when={props.showLabel}>
        <span class="mono muted" style={sx({ fontSize: 11, minWidth: 38, textAlign: 'right' })}>{pct().toFixed(0)}%</span>
      </Show>
    </div>
  );
}

/* ---------- Pagination ---------- */
export function Pagination(props: { page: number; totalPages: number; onPage: (p: number) => void }) {
  return (
    <Show when={props.totalPages > 1}>
      <div style={sx({ display: 'flex', alignItems: 'center', justifyContent: 'center', gap: 6, marginTop: 14 })}>
        <Btn size="sm" variant="ghost" disabled={props.page <= 1} onClick={() => props.onPage(props.page - 1)}>上一页</Btn>
        <span class="mono muted" style={sx({ fontSize: 12.5, padding: '0 6px' })}>{props.page} / {props.totalPages}</span>
        <Btn size="sm" variant="ghost" disabled={props.page >= props.totalPages} onClick={() => props.onPage(props.page + 1)}>下一页</Btn>
      </div>
    </Show>
  );
}

/* ---------- PageHead ---------- */
export function PageHead(props: { title: string; desc?: string; eyebrow?: string; right?: JSX.Element }) {
  return (
    <div style={sx({ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', gap: 16, flexWrap: 'wrap', marginBottom: 20 })}>
      <div style={sx({ minWidth: 0 })}>
        <Show when={props.eyebrow}><div class="eyebrow" style={sx({ marginBottom: 6 })}>{props.eyebrow}</div></Show>
        <h1 class="page-title">{props.title}</h1>
        <Show when={props.desc}><p class="muted" style={sx({ margin: '6px 0 0', fontSize: 13.5, maxWidth: 720 })}>{props.desc}</p></Show>
      </div>
      <Show when={props.right}><div style={sx({ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap' })}>{props.right}</div></Show>
    </div>
  );
}
