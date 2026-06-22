import { createSignal, createMemo, createEffect, onMount, onCleanup, For, Show } from 'solid-js';
import { Card, StatCard, Panel, PageHead, Badge, Btn, sx, fmtNum, fmtBytes, fmtDur } from '@/components/wf';
import { adminApi } from '@/api/admin';
import { themeStore } from '@/stores/theme';

/* ---------- /health 公开端点的结构（HealthInfo 仅强类型化部分字段，此处补全所需形状） ---------- */
interface SvcHealth {
  healthy?: boolean;
  status?: string;
  driftSecs?: number;
  thresholdSecs?: number;
  lastCheckAt?: number;
  activeConnections?: number;
  activeDevices?: number;
  probeSkipped?: boolean;
}
interface Health {
  status?: string;
  version?: string;
  uptimeSecs?: number;
  serverTime?: number;
  dbSizeBytes?: number | null;
  availability?: { pct: number; effectiveSecs: number; totalRequests: number } | null;
  services?: Record<string, SvcHealth>;
}

/* 子系统展示顺序与中文角色名（amas = AMAS 自适应引擎，store = 数据存储…） */
const SVCS: { key: string; role: string }[] = [
  { key: 'amas', role: 'AMAS 引擎' },
  { key: 'store', role: '数据存储' },
  { key: 'sse', role: '实时推送' },
  { key: 'clock', role: '时钟同步' },
  { key: 'wordbookCenter', role: '词库中心' },
];

const POLL_MS = 10_000; // 对齐跨端心跳契约（≤10s）

const pad = (n: number) => String(n).padStart(2, '0');
/** 运行时长 → "Nd HH:MM:SS" */
function fmtUptimeClock(s: number): string {
  s = Math.max(0, Math.floor(s));
  const d = Math.floor(s / 86400); s %= 86400;
  const h = Math.floor(s / 3600); s %= 3600;
  const m = Math.floor(s / 60), sec = s % 60;
  return (d > 0 ? d + 'd ' : '') + pad(h) + ':' + pad(m) + ':' + pad(sec);
}
const fmtClockMs = (ms: number) => new Date(ms).toLocaleTimeString('zh-CN', { hour12: false });

type State = 'boot' | 'ok' | 'degraded' | 'fault';

export default function VitalsPage() {
  const [health, setHealth] = createSignal<Health | null>(null);
  const [err, setErr] = createSignal<string | null>(null);
  const [lastOk, setLastOk] = createSignal(0);     // 最近一次成功拉取的本地时刻(ms)
  const [now, setNow] = createSignal(Date.now());   // 每秒滴答，驱动运行时长/时钟/相对时间
  const [loading, setLoading] = createSignal(true);

  const elapsed = () => Math.max(0, (now() - lastOk()) / 1000);

  const state = createMemo<State>(() => {
    if (err()) return 'fault';
    const h = health();
    if (!h) return 'boot';
    const bad = SVCS.some((s) => h.services?.[s.key]?.healthy === false);
    if (h.status !== 'ok' || bad) return 'degraded';
    return 'ok';
  });

  const badList = () => {
    const h = health();
    return h ? SVCS.filter((s) => h.services?.[s.key]?.healthy === false).map((s) => s.key) : [];
  };
  const healthyCount = () => {
    const h = health();
    return h ? SVCS.filter((s) => h.services?.[s.key]?.healthy !== false).length : 0;
  };

  // 派生读数（运行时长/服务器时钟在故障态冻结，避免暗示仍在心跳）
  const uptimeText = () => fmtUptimeClock((health()?.uptimeSecs ?? 0) + (state() === 'fault' ? 0 : elapsed()));
  const serverClock = () => {
    const base = health()?.serverTime;
    return base ? fmtClockMs(base * 1000 + (state() === 'fault' ? 0 : elapsed() * 1000)) : '—:—:—';
  };
  const agoText = () => {
    if (!lastOk()) return '连接中…';
    const s = Math.round(elapsed());
    const rel = s < 60 ? `${s} 秒前` : s < 3600 ? `${Math.floor(s / 60)} 分钟前` : `${Math.floor(s / 3600)} 小时前`;
    return state() === 'fault' ? `数据陈旧 · ${rel}` : `更新于 ${rel}`;
  };

  const pct = () => health()?.availability?.pct ?? null;
  const availTone = (): 'success' | 'warning' | 'error' => {
    const p = pct();
    if (state() === 'fault' || p == null) return 'error';
    return p >= 99.9 ? 'success' : p >= 99 ? 'warning' : 'error';
  };
  const statusTone = () => (state() === 'fault' ? 'error' : state() === 'degraded' ? 'warning' : 'success');
  const statusWord = () => ({ boot: '连接中', ok: '正常', degraded: '降级', fault: '不可达' }[state()]);
  const statusMsg = () => {
    if (state() === 'fault') return `无法连接服务端（${err() || '网络错误'}），正在每 ${POLL_MS / 1000}s 重试。`;
    if (state() === 'boot') return '正在建立与服务端的连接…';
    if (state() === 'degraded') return `${badList().join('、')} 需要关注 — ${healthyCount()}/${SVCS.length} 个子系统健康。`;
    return `全部系统正常 · ${healthyCount()}/${SVCS.length} 个子系统健康。`;
  };

  // ---------- 轮询 + 滴答 ----------
  const load = async () => {
    try {
      const h = (await adminApi.health()) as unknown as Health;
      setHealth(h);
      setErr(null);
      setLastOk(Date.now());
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  };

  let pollT: ReturnType<typeof setInterval>;
  let tickT: ReturnType<typeof setInterval>;
  onMount(() => {
    load();
    pollT = setInterval(load, POLL_MS);
    tickT = setInterval(() => setNow(Date.now()), 1000);
  });
  onCleanup(() => { clearInterval(pollT); clearInterval(tickT); });

  // ================= 心电脉冲示波器（标志元素） =================
  let canvas: HTMLCanvasElement | undefined;
  onMount(() => {
    const cv = canvas;
    const cx = cv?.getContext('2d');
    if (!cv || !cx) return;
    const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;
    let W = 0, H = 0, dpr = 1, raf = 0, faultLevel = 0;

    const resize = () => {
      dpr = Math.min(2, window.devicePixelRatio || 1);
      const r = cv.getBoundingClientRect();
      W = Math.max(1, r.width); H = Math.max(1, r.height);
      cv.width = Math.round(W * dpr); cv.height = Math.round(H * dpr);
      cx.setTransform(dpr, 0, 0, dpr, 0, 0);
    };

    const g = (x: number, mu: number, sig: number, amp: number) => amp * Math.exp(-((x - mu) * (x - mu)) / (2 * sig * sig));
    // 单个心动周期 x∈[0,1)，R 峰≈1.0
    const ecg = (x: number) =>
      g(x, 0.16, 0.026, 0.13) - g(x, 0.305, 0.009, 0.17) + g(x, 0.345, 0.010, 1.0)
      - g(x, 0.385, 0.013, 0.30) + g(x, 0.58, 0.046, 0.24);

    const CYCLE = 230, SPEED = 0.085;
    const col = () => getComputedStyle(cv).getPropertyValue(
      state() === 'fault' ? '--error' : state() === 'degraded' ? '--warning' : '--success',
    ).trim() || '#2f8a5b';

    const draw = (t: number) => {
      const sig = col();
      const mid = H * 0.58, amp = H * 0.30;
      faultLevel += ((state() === 'fault' ? 1 : 0) - faultLevel) * 0.06;
      const offset = t * SPEED;

      cx.clearRect(0, 0, W, H);
      // 基线网格
      cx.lineWidth = 1; cx.strokeStyle = 'rgba(128,128,128,0.10)';
      cx.beginPath();
      for (let x = (W % 44); x < W; x += 44) { cx.moveTo(x + .5, 0); cx.lineTo(x + .5, H); }
      for (let y = mid % 38; y < H; y += 38) { cx.moveTo(0, y + .5); cx.lineTo(W, y + .5); }
      cx.stroke();
      cx.strokeStyle = 'rgba(128,128,128,0.16)';
      cx.beginPath(); cx.moveTo(0, mid + .5); cx.lineTo(W, mid + .5); cx.stroke();

      // 波形
      cx.lineJoin = 'round'; cx.lineCap = 'round'; cx.lineWidth = 2;
      cx.strokeStyle = sig; cx.shadowColor = sig; cx.shadowBlur = 8;
      cx.beginPath();
      let lastY = mid;
      for (let px = 0; px <= W; px += 2) {
        const s = offset + px;
        const phase = (s % CYCLE) / CYCLE;
        const live = ecg(phase) * amp * (1 - faultLevel);
        const y = mid - live;
        if (px === 0) cx.moveTo(px, y); else cx.lineTo(px, y);
        lastY = y;
      }
      cx.stroke();
      // 前导游标点
      cx.fillStyle = sig; cx.shadowBlur = 13;
      cx.beginPath(); cx.arc(W - 1, lastY, 2.6, 0, Math.PI * 2); cx.fill();
      cx.shadowBlur = 0;
    };

    resize();
    window.addEventListener('resize', resize);

    if (reduced) {
      // 降低动效：静止单帧，仅在状态/主题变化时重绘
      createEffect(() => { state(); themeStore.effective(); draw(0); });
    } else {
      const loop = (t: number) => { draw(t); raf = requestAnimationFrame(loop); };
      raf = requestAnimationFrame(loop);
    }
    onCleanup(() => { cancelAnimationFrame(raf); window.removeEventListener('resize', resize); });
  });

  return (
    <div style={sx({ display: 'flex', flexDirection: 'column', gap: 16 })}>
      <PageHead
        eyebrow="VITALS · 实时体征"
        title="服务体征"
        desc="服务端公开 /health 端点的一眼概览：整体脉搏、关键指标与各子系统状态，每 10s 自动刷新。"
        right={
          <>
            <Badge variant={statusTone() as 'success' | 'warning' | 'error'} dot>{statusWord()}</Badge>
            <span class="mono muted-3" style={sx({ fontSize: 11.5 })}>{agoText()}</span>
            <Btn size="sm" variant="secondary" icon="refresh" onClick={load} disabled={loading()}>刷新</Btn>
          </>
        }
      />

      {/* ---- 心电脉冲 hero ---- */}
      <Card pad={false} style={sx({ position: 'relative', overflow: 'hidden', minHeight: 196 })}>
        <canvas
          ref={canvas}
          role="img"
          aria-label="服务实时脉冲波形"
          style={sx({
            position: 'absolute', inset: 0, width: '100%', height: '100%', display: 'block',
            maskImage: 'linear-gradient(90deg, transparent 0%, #000 14%)',
            WebkitMaskImage: 'linear-gradient(90deg, transparent 0%, #000 14%)',
          })}
        />
        <div
          style={sx({
            position: 'relative', zIndex: 1, height: '100%', minHeight: 196,
            display: 'flex', flexDirection: 'column', justifyContent: 'space-between', gap: 14,
            padding: 'clamp(18px, 2.4vw, 26px)', pointerEvents: 'none',
            background: 'linear-gradient(90deg, color-mix(in srgb, var(--surface-solid) 90%, transparent) 0%, color-mix(in srgb, var(--surface-solid) 50%, transparent) 36%, transparent 64%)',
          })}
        >
          <div>
            <div style={sx({ display: 'flex', alignItems: 'center', gap: 13 })}>
              <span
                class={state() !== 'fault' ? 'live-dot' : ''}
                style={sx({
                  width: 13, height: 13, borderRadius: 50, flex: 'none',
                  background: `var(--${statusTone()})`,
                  boxShadow: state() === 'fault' ? 'none' : `0 0 14px 2px color-mix(in srgb, var(--${statusTone()}) 55%, transparent)`,
                })}
              />
              <span class="tnum" style={sx({ fontSize: 'clamp(34px, 6vw, 60px)', fontWeight: 800, lineHeight: .92, letterSpacing: '-0.02em', color: `var(--${statusTone()})` })}>
                {statusWord()}
              </span>
            </div>
            <p class="muted" aria-live="polite" style={sx({ margin: '12px 0 0', fontSize: 13.5, maxWidth: '52ch', lineHeight: 1.5 })}>
              {statusMsg()}
            </p>
          </div>
          <div class="eyebrow" style={sx({ fontSize: 10 })}>
            实时心跳 · 每 {POLL_MS / 1000}s · v{health()?.version ?? '—'}
          </div>
        </div>
      </Card>

      {/* ---- 关键指标 ---- */}
      <div style={sx({ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(190px, 1fr))', gap: 14 })}>
        <StatCard
          tone={availTone()} icon="zap" label="可用性"
          value={pct() == null ? '—' : pct()!.toFixed(2)} unit={pct() == null ? undefined : '%'}
          deltaLabel={health()?.availability?.effectiveSecs ? `测量窗口 ${fmtDur(health()!.availability!.effectiveSecs)}` : undefined}
        />
        <StatCard tone="info" icon="clock" label="运行时长" value={<span class="mono">{uptimeText()}</span>} deltaLabel="自上次启动" />
        <StatCard tone="accent" icon="chart" label="累计请求" value={fmtNum(health()?.availability?.totalRequests ?? null)} deltaLabel="服务全周期" />
        <StatCard tone="accent" icon="db" label="数据库" value={fmtBytes(health()?.dbSizeBytes ?? null)} deltaLabel="磁盘占用" />
      </div>

      {/* ---- 子系统 ---- */}
      <Panel
        title="子系统"
        right={<span class="mono muted-3" style={sx({ fontSize: 11.5 })}>{healthyCount()}/{SVCS.length} 健康 · 服务器时钟 {serverClock()}</span>}
      >
        <div style={sx({ display: 'flex', flexDirection: 'column' })}>
          <For each={SVCS}>
            {(s, i) => {
              const svc = () => health()?.services?.[s.key] ?? {};
              const ok = () => svc().healthy !== false && state() !== 'fault';
              const detail = () => {
                const v = svc();
                if (s.key === 'sse') return `${fmtNum(v.activeConnections ?? 0)} 连接 · ${fmtNum(v.activeDevices ?? 0)} 设备`;
                if (s.key === 'clock') return `漂移 ${v.driftSecs ?? 0}s / ${v.thresholdSecs ?? 60}s`;
                if (s.key === 'wordbookCenter') return v.probeSkipped ? '探针 已跳过' : '探针 在线';
                return '';
              };
              return (
                <div
                  style={sx({
                    display: 'grid', gridTemplateColumns: '12px 1fr auto', alignItems: 'center', gap: 14,
                    padding: '13px 2px',
                    borderTop: i() === 0 ? 'none' : '1px solid var(--hairline)',
                  })}
                >
                  <span
                    class={ok() ? 'live-dot' : ''}
                    style={sx({ width: 9, height: 9, borderRadius: 50, background: ok() ? 'var(--success)' : 'var(--error)' })}
                  />
                  <span style={sx({ minWidth: 0 })}>
                    <span class="mono" style={sx({ fontSize: 13.5, fontWeight: 600, color: 'var(--text)' })}>{s.key}</span>
                    <span class="muted-3" style={sx({ fontSize: 11.5, marginLeft: 9 })}>{s.role}</span>
                  </span>
                  <span style={sx({ display: 'inline-flex', alignItems: 'center', gap: 14, justifyContent: 'flex-end' })}>
                    <Show when={detail()}>
                      <span class="mono muted" style={sx({ fontSize: 12 })}>{detail()}</span>
                    </Show>
                    <span style={sx({ fontSize: 11.5, fontWeight: 600, color: ok() ? 'var(--success)' : 'var(--error)', minWidth: 52, textAlign: 'right' })}>
                      {ok() ? '健康' : state() === 'fault' ? '未知' : '异常'}
                    </span>
                  </span>
                </div>
              );
            }}
          </For>
        </div>
      </Panel>

      <div class="muted-3 mono" style={sx({ fontSize: 11, textAlign: 'right' })}>
        数据源 GET /health · 公开端点
      </div>
    </div>
  );
}
