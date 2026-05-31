import { For, Show, type JSX } from 'solid-js';

interface SectionProps {
  id: string;
  icon: JSX.Element;
  iconStyle?: JSX.CSSProperties;
  title: string;
  desc: string;
  /** 右上角状态：连接 pill / last-edit 文本 */
  right?: JSX.Element;
  dirtyCount?: number;
  children: JSX.Element;
  ref?: (el: HTMLElement) => void;
}

export function SectionCard(props: SectionProps) {
  return (
    <section id={props.id} class="st-section" ref={props.ref}>
      <div class="sec-head">
        <div class="ic" style={props.iconStyle}>{props.icon}</div>
        <div>
          <div class="title">{props.title}</div>
          <div class="desc">{props.desc}</div>
        </div>
        <div class="right">
          <Show when={props.dirtyCount} fallback={props.right}>
            <span class="dirty">{props.dirtyCount} 处未保存</span>
          </Show>
        </div>
      </div>
      <div class="sec-body">{props.children}</div>
    </section>
  );
}

interface RowProps {
  label: string;
  hint?: string;
  metaPill?: string;
  children: JSX.Element;
}

export function Row(props: RowProps) {
  return (
    <div class="st-row">
      <div class="lbl">
        <strong>{props.label}</strong>
        <Show when={props.hint}><p>{props.hint}</p></Show>
        <Show when={props.metaPill}><span class="meta-pill">{props.metaPill}</span></Show>
      </div>
      <div class="ctrl">{props.children}</div>
    </div>
  );
}

interface FieldProps {
  value: unknown;
  onChange: (v: string) => void;
  mono?: boolean;
  type?: string;
  placeholder?: string;
  size?: 'compact' | 'medium' | 'wide';
  width?: string;
}
export function Field(props: FieldProps) {
  return (
    <input
      class={`st-input ${props.mono ? 'is-mono' : ''} ${props.size ?? ''}`}
      style={props.width ? { width: props.width } : undefined}
      type={props.type ?? 'text'}
      placeholder={props.placeholder}
      value={props.value == null ? '' : String(props.value)}
      onInput={(e) => props.onChange(e.currentTarget.value)}
    />
  );
}

interface SegOption<T> {
  value: T;
  label: string;
}
interface SegProps<T extends string> {
  options: SegOption<T>[];
  value: T;
  onChange: (v: T) => void;
}
/** 单选分段控件(镜像 settings.html .seg）。 */
export function Seg<T extends string>(props: SegProps<T>) {
  return (
    <div class="seg">
      <For each={props.options}>
        {(o) => (
          <button
            type="button"
            class={props.value === o.value ? 'is-on' : ''}
            onClick={() => props.onChange(o.value)}
          >
            {o.label}
          </button>
        )}
      </For>
    </div>
  );
}
