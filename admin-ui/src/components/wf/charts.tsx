import { createSignal, createMemo, createUniqueId, onMount, onCleanup, For, Show, type JSX } from 'solid-js';
import { sx } from './sx';

/* ============================================================
   Charts (pure SVG) — ported from the design handoff ui.jsx
   ============================================================ */

export interface Series { name: string; data: number[]; color: string; }

export function Sparkline(props: {
  data: number[]; color?: string; width?: number; height?: number; fill?: boolean;
}) {
  const gid = 'sg' + createUniqueId();
  const geo = createMemo(() => {
    const data = props.data;
    if (!data || data.length < 2) return null;
    const width = props.width ?? 100;
    const height = props.height ?? 32;
    const max = Math.max(...data), min = Math.min(...data), rng = max - min || 1;
    const pts = data.map((v, i): [number, number] => [
      (i / (data.length - 1)) * width,
      height - ((v - min) / rng) * (height - 4) - 2,
    ]);
    const line = pts.map((p, i) => (i ? 'L' : 'M') + p[0].toFixed(1) + ' ' + p[1].toFixed(1)).join(' ');
    return { width, height, pts, line };
  });
  return (
    <Show when={geo()}>
      {(g) => {
        const color = props.color ?? 'var(--accent)';
        const fill = props.fill ?? true;
        const last = g().pts[g().pts.length - 1];
        return (
          <svg width={g().width} height={g().height} style={sx({ overflow: 'visible', display: 'block' })}>
            <defs>
              <linearGradient id={gid} x1="0" y1="0" x2="0" y2="1">
                <stop offset="0%" stop-color={color} stop-opacity="0.28" />
                <stop offset="100%" stop-color={color} stop-opacity="0" />
              </linearGradient>
            </defs>
            <Show when={fill}>
              <path d={`${g().line} L ${g().width} ${g().height} L 0 ${g().height} Z`} fill={`url(#${gid})`} />
            </Show>
            <path d={g().line} fill="none" stroke={color} stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
            <circle cx={last[0]} cy={last[1]} r="2.6" fill={color} />
          </svg>
        );
      }}
    </Show>
  );
}

function useWidth(initial = 600): [() => number, (el: HTMLDivElement) => void] {
  const [w, setW] = createSignal(initial);
  const attach = (el: HTMLDivElement) => {
    const ro = new ResizeObserver((e) => setW(e[0].contentRect.width));
    onMount(() => ro.observe(el));
    onCleanup(() => ro.disconnect());
  };
  return [w, attach];
}

export function Legend(props: { series: Series[] }) {
  return (
    <div style={sx({ display: 'flex', gap: 16, justifyContent: 'center', marginTop: 6, flexWrap: 'wrap' })}>
      <For each={props.series}>
        {(s) => (
          <span style={sx({ display: 'inline-flex', gap: 6, alignItems: 'center', fontSize: 11.5, color: 'var(--text-2)' })}>
            <i style={sx({ width: 9, height: 9, borderRadius: 3, background: s.color })} />
            {s.name}
          </span>
        )}
      </For>
    </div>
  );
}

export function LineChart(props: {
  series: Series[]; labels: string[]; height?: number; yMax?: number; area?: boolean; fmtY?: (v: number) => string;
}) {
  const [w, attach] = useWidth(600);
  const [hover, setHover] = createSignal<number | null>(null);
  const height = () => props.height ?? 280;
  const padL = 44, padR = 16, padT = 14, padB = 26;
  const geo = createMemo(() => {
    const series = props.series, labels = props.labels;
    const all = series.flatMap((s) => s.data);
    const max = props.yMax != null ? props.yMax : Math.max(1, ...all) * 1.1;
    const min = 0;
    const innerW = Math.max(10, w() - padL - padR);
    const innerH = height() - padT - padB;
    const n = labels.length;
    const X = (i: number) => padL + (n <= 1 ? innerW / 2 : (i / (n - 1)) * innerW);
    const Y = (v: number) => padT + innerH - ((v - min) / (max - min || 1)) * innerH;
    return { max, innerW, innerH, n, X, Y };
  });
  const ticks = 4;
  const area = () => props.area ?? true;
  const fmtY = (v: number) => (props.fmtY ? props.fmtY(v) : (v >= 1000 ? (v / 1000).toFixed(0) + 'k' : String(Math.round(v))));
  return (
    <div ref={attach} style={sx({ position: 'relative' })}>
      <svg
        width={w()} height={height()}
        onMouseLeave={() => setHover(null)}
        onMouseMove={(e) => {
          const r = (e.currentTarget as SVGSVGElement).getBoundingClientRect();
          const x = e.clientX - r.left;
          const g = geo();
          const i = Math.round(((x - padL) / g.innerW) * (g.n - 1));
          if (i >= 0 && i < g.n) setHover(i);
        }}
      >
        <For each={Array.from({ length: ticks + 1 })}>
          {(_, t) => {
            const v = () => (geo().max / ticks) * t();
            const y = () => geo().Y(v());
            return (
              <g>
                <line x1={padL} y1={y()} x2={w() - padR} y2={y()} stroke="var(--hairline)" />
                <text x={padL - 8} y={y() + 3.5} text-anchor="end" font-size="10" fill="var(--text-3)" class="mono">{fmtY(v())}</text>
              </g>
            );
          }}
        </For>
        <For each={props.series}>
          {(s, si) => {
            const gid = 'lg' + createUniqueId();
            const line = () => s.data.map((v, i) => (i ? 'L' : 'M') + geo().X(i).toFixed(1) + ' ' + geo().Y(v).toFixed(1)).join(' ');
            return (
              <g>
                <defs>
                  <linearGradient id={gid} x1="0" y1="0" x2="0" y2="1">
                    <stop offset="0%" stop-color={s.color} stop-opacity={area() ? 0.2 : 0} />
                    <stop offset="100%" stop-color={s.color} stop-opacity="0" />
                  </linearGradient>
                </defs>
                <Show when={area()}>
                  <path d={`${line()} L ${geo().X(geo().n - 1)} ${padT + geo().innerH} L ${geo().X(0)} ${padT + geo().innerH} Z`} fill={`url(#${gid})`} />
                </Show>
                <path d={line()} fill="none" stroke={s.color} stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" />
                <Show when={hover() != null}>
                  <circle cx={geo().X(hover()!)} cy={geo().Y(s.data[hover()!])} r="3.5" fill={s.color} stroke="var(--surface-solid)" stroke-width="2" />
                </Show>
                {void si}
              </g>
            );
          }}
        </For>
        <Show when={hover() != null}>
          <line x1={geo().X(hover()!)} y1={padT} x2={geo().X(hover()!)} y2={padT + geo().innerH} stroke="var(--border-strong)" stroke-dasharray="3 3" />
        </Show>
        <For each={props.labels}>
          {(l, i) => (
            <Show when={geo().n <= 12 || i() % Math.ceil(geo().n / 8) === 0}>
              <text x={geo().X(i())} y={height() - 8} text-anchor="middle" font-size="10" fill="var(--text-3)" class="mono">{l}</text>
            </Show>
          )}
        </For>
      </svg>
      <Show when={hover() != null}>
        <div
          class="card"
          style={sx({
            position: 'absolute', left: Math.min(geo().X(hover()!) + 10, w() - 140), top: 10,
            padding: '8px 10px', pointerEvents: 'none', fontSize: 11.5, boxShadow: 'var(--shadow-2)', zIndex: 2,
          })}
        >
          <div class="mono muted-3" style={sx({ marginBottom: 4 })}>{props.labels[hover()!]}</div>
          <For each={props.series}>
            {(s) => (
              <div style={sx({ display: 'flex', gap: 8, alignItems: 'center', justifyContent: 'space-between' })}>
                <span style={sx({ display: 'inline-flex', gap: 5, alignItems: 'center' })}>
                  <i style={sx({ width: 8, height: 8, borderRadius: 2, background: s.color, display: 'inline-block' })} />{s.name}
                </span>
                <b class="tnum">{props.fmtY ? props.fmtY(s.data[hover()!]) : s.data[hover()!].toLocaleString()}</b>
              </div>
            )}
          </For>
        </div>
      </Show>
      <Legend series={props.series} />
    </div>
  );
}

export interface BarDatum { label: string; value: number; color?: string; }

export function BarChart(props: {
  data: BarDatum[]; height?: number; color?: string; horizontal?: boolean; fmtV?: (v: number) => string;
}) {
  const [w, attach] = useWidth(500);
  const height = () => props.height ?? 260;
  const color = () => props.color ?? 'var(--accent)';
  const max = () => Math.max(1, ...props.data.map((d) => d.value));
  return (
    <Show
      when={props.horizontal}
      fallback={(() => {
        const padB = 28;
        const innerH = () => height() - padB;
        const gap = 8;
        const bw = () => (w() - gap * (props.data.length - 1)) / props.data.length;
        return (
          <div ref={attach}>
            <svg width={w()} height={height()}>
              <For each={props.data}>
                {(d, i) => {
                  const h = () => (d.value / max()) * (innerH() - 10);
                  const x = () => i() * (bw() + gap);
                  return (
                    <g>
                      <rect x={x()} y={innerH() - h()} width={bw()} height={h()} rx="5" fill={d.color || color()} opacity="0.92" />
                      <text x={x() + bw() / 2} y={height() - 9} text-anchor="middle" font-size="10" fill="var(--text-3)">{d.label}</text>
                    </g>
                  );
                }}
              </For>
            </svg>
          </div>
        );
      })()}
    >
      <div ref={attach} style={sx({ display: 'flex', flexDirection: 'column', gap: 9 })}>
        <For each={props.data}>
          {(d) => (
            <div style={sx({ display: 'grid', gridTemplateColumns: '90px 1fr 56px', alignItems: 'center', gap: 10, fontSize: 12.5 })}>
              <span class="muted" style={sx({ textAlign: 'right', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' })} title={d.label}>{d.label}</span>
              <div class="bar" style={sx({ height: 9 })}><i style={sx({ width: (d.value / max() * 100) + '%', background: d.color || color() })} /></div>
              <span class="mono tnum" style={sx({ textAlign: 'right' })}>{props.fmtV ? props.fmtV(d.value) : d.value.toLocaleString()}</span>
            </div>
          )}
        </For>
      </div>
    </Show>
  );
}

export interface DonutDatum { label: string; value: number; color: string; }

export function Donut(props: {
  data: DonutDatum[]; size?: number; thickness?: number; centerLabel?: string; centerValue?: string | number;
}) {
  const size = () => props.size ?? 180;
  const thickness = () => props.thickness ?? 26;
  const total = () => props.data.reduce((s, d) => s + d.value, 0) || 1;
  const r = () => (size() - thickness()) / 2;
  const C = () => 2 * Math.PI * r();
  const segs = createMemo(() => {
    let off = 0;
    return props.data.map((d) => {
      const frac = d.value / total();
      const dash = frac * C();
      const seg = { d, dash, off };
      off += dash;
      return seg;
    });
  });
  return (
    <div style={sx({ display: 'flex', alignItems: 'center', gap: 20, flexWrap: 'wrap' })}>
      <svg width={size()} height={size()} style={sx({ transform: 'rotate(-90deg)', flex: 'none' })}>
        <For each={segs()}>
          {(s) => (
            <circle
              cx={size() / 2} cy={size() / 2} r={r()} fill="none" stroke={s.d.color} stroke-width={thickness()}
              stroke-dasharray={`${s.dash} ${C() - s.dash}`} stroke-dashoffset={-s.off} stroke-linecap="butt"
            />
          )}
        </For>
        <Show when={props.centerValue != null}>
          <g transform={`rotate(90 ${size() / 2} ${size() / 2})`}>
            <text x={size() / 2} y={size() / 2 - 2} text-anchor="middle" font-size="22" font-weight="800" fill="var(--text)">{props.centerValue}</text>
            <text x={size() / 2} y={size() / 2 + 16} text-anchor="middle" font-size="10.5" fill="var(--text-3)">{props.centerLabel}</text>
          </g>
        </Show>
      </svg>
      <div style={sx({ display: 'flex', flexDirection: 'column', gap: 7, minWidth: 130, flex: 1 })}>
        <For each={props.data}>
          {(d) => (
            <div style={sx({ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 10, fontSize: 12.5 })}>
              <span style={sx({ display: 'inline-flex', gap: 7, alignItems: 'center', minWidth: 0 })}>
                <i style={sx({ width: 9, height: 9, borderRadius: 3, background: d.color, flex: 'none' })} />
                <span style={sx({ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' })}>{d.label}</span>
              </span>
              <b class="mono tnum">{((d.value / total()) * 100).toFixed(1)}%</b>
            </div>
          )}
        </For>
      </div>
    </div>
  );
}

export function Heatmap(props: {
  rows: number; cols: number; values: number[][]; colorFor?: (v: number) => string;
  cellH?: number; rowLabels?: string[]; colLabels?: string[]; gap?: number; fmtCell?: (r: number, c: number, v: number) => string;
}) {
  const cellH = () => props.cellH ?? 18;
  const gap = () => props.gap ?? 2;
  return (
    <div style={sx({ overflowX: 'auto' })}>
      <div style={sx({ display: 'grid', gridTemplateColumns: `${props.rowLabels ? 48 : 0}px 1fr`, gap: 6 })}>
        <div />
        <div style={sx({ display: 'grid', gridTemplateColumns: `repeat(${props.cols}, 1fr)`, gap: gap(), fontSize: 9, color: 'var(--text-3)', textAlign: 'center' })}>
          <Show when={props.colLabels}>
            <For each={props.colLabels}>{(l, i) => <div>{(props.cols <= 14 || i() % 3 === 0) ? l : ''}</div>}</For>
          </Show>
        </div>
        <For each={Array.from({ length: props.rows })}>
          {(_, r) => (
            <div style={sx({ display: 'contents' })}>
              <Show when={props.rowLabels} fallback={<div />}>
                <div style={sx({ fontSize: 10, color: 'var(--text-3)', display: 'flex', alignItems: 'center', justifyContent: 'flex-end', paddingRight: 6 })}>{props.rowLabels![r()]}</div>
              </Show>
              <div style={sx({ display: 'grid', gridTemplateColumns: `repeat(${props.cols}, 1fr)`, gap: gap() })}>
                <For each={Array.from({ length: props.cols })}>
                  {(__, c) => {
                    const v = () => (props.values[r()] ? props.values[r()][c()] : -1);
                    const empty = () => v() == null || v() < 0;
                    return (
                      <div
                        title={props.fmtCell ? props.fmtCell(r(), c(), v()) : (v() >= 0 ? v().toFixed(2) : '')}
                        style={sx({
                          height: cellH(), borderRadius: 3,
                          background: empty() ? 'var(--surface-sunken)' : (props.colorFor ? props.colorFor(v()) : `color-mix(in oklch, var(--accent) ${Math.round(v() * 100)}%, transparent)`),
                        })}
                      />
                    );
                  }}
                </For>
              </div>
            </div>
          )}
        </For>
      </div>
    </div>
  );
}

export interface ScatterPoint { x: number; y: number; r?: number; [k: string]: unknown; }

export function Scatter(props: {
  points: ScatterPoint[]; height?: number; xLabel?: string; yLabel?: string; colorFor?: (p: ScatterPoint) => string;
}) {
  const [w, attach] = useWidth(500);
  const height = () => props.height ?? 280;
  const padL = 42, padR = 14, padT = 12, padB = 30;
  const geo = createMemo(() => {
    const xs = props.points.map((p) => p.x), ys = props.points.map((p) => p.y);
    const xMin = Math.min(...xs), xMax = Math.max(...xs), yMin = 0, yMax = Math.max(...ys) * 1.05 || 1;
    const innerW = Math.max(10, w() - padL - padR), innerH = height() - padT - padB;
    const X = (v: number) => padL + ((v - xMin) / (xMax - xMin || 1)) * innerW;
    const Y = (v: number) => padT + innerH - ((v - yMin) / (yMax - yMin || 1)) * innerH;
    return { xMin, xMax, innerH, X, Y };
  });
  return (
    <div ref={attach}>
      <svg width={w()} height={height()}>
        <For each={Array.from({ length: 5 })}>
          {(_, t) => {
            const y = () => padT + (geo().innerH / 4) * t();
            return <line x1={padL} y1={y()} x2={w() - padR} y2={y()} stroke="var(--hairline)" />;
          }}
        </For>
        <For each={props.points}>
          {(p) => <circle cx={geo().X(p.x)} cy={geo().Y(p.y)} r={p.r || 3} fill={props.colorFor ? props.colorFor(p) : 'var(--accent)'} opacity="0.6" />}
        </For>
        <text x={padL} y={height() - 8} font-size="10" fill="var(--text-3)" class="mono">{Math.round(geo().xMin)}</text>
        <text x={w() - padR} y={height() - 8} font-size="10" fill="var(--text-3)" text-anchor="end" class="mono">{Math.round(geo().xMax)}</text>
        <Show when={props.xLabel}>
          <text x={padL + (w() - padL - padR) / 2} y={height() - 8} font-size="10.5" fill="var(--text-2)" text-anchor="middle">{props.xLabel}</text>
        </Show>
      </svg>
    </div>
  );
}
