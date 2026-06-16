/**
 * 远程探针（remote-probe）— sandbox dispatch 控制台。
 * 向在线设备下发受控诊断脚本，订阅 batch SSE 实时收集回传，
 * 失败时回退到 probeApi.list 轮询；D 类 confirm_required 弹二次确认。
 *
 * 真实 API：probeApi.dispatch / list / confirm + connectProbeBatchStream（@/api/probe）。
 * 脚本模板来自 @/pages/probe-templates。
 */

import { createMemo, createSignal, For, Show, onCleanup, onMount } from 'solid-js';
import {
  connectProbeBatchStream,
  probeApi,
  type ProbeResultEvent,
  type ProbeExecutionRow,
} from '@/api/probe';
import { PROBE_TEMPLATES } from './probe-templates';
import {
  Badge,
  Btn,
  Empty,
  Field,
  Loading,
  Modal,
  Panel,
  PageHead,
  Progress,
  Seg,
  fmtAgo,
  sx,
  toast,
} from '@/components/wf';

/* ---------------------------- 类型 & 常量 ---------------------------- */

interface ResultCardData extends ProbeResultEvent {
  receivedAt: number;
}

type TargetMode = 'single' | 'multi' | 'allOnline';

/** results 数组 ring buffer 软上限：超量时丢弃最旧 */
const RESULTS_BUFFER_CAP = 200;

const DEFAULT_SCRIPT = `return {
  ua: ctx.nav.ua,
  language: ctx.nav.language,
  mem: ctx.perf.memoryMB(),
};`;

const AUDIENCE_LABEL: Record<TargetMode, string> = {
  single: '单设备',
  multi: '多设备',
  allOnline: '全部在线',
};

const STATUS_TONE: Record<string, 'success' | 'warning' | 'error' | 'info'> = {
  ok: 'success',
  confirm_required: 'warning',
  timeout: 'warning',
  expired: 'warning',
  error: 'error',
};

function statusTone(status: string): 'success' | 'warning' | 'error' | 'info' {
  return STATUS_TONE[status] ?? 'info';
}

/** 从毫秒时间戳取本地「时:分:秒」。注意：不可对 fmtTime 输出位置切片——
 *  fmtTime 用 toLocaleString('zh-CN') 产出非 ISO 串（月/日不补零），slice 会错位。 */
function fmtClock(ms: number): string {
  return new Date(ms).toLocaleTimeString('zh-CN', { hour12: false });
}

/** confirm_required 结果里嵌入的待执行动作预览（D 类受控写）。 */
function extractActionsPreview(resultJson: unknown): string[] {
  if (resultJson && typeof resultJson === 'object' && '_actions' in resultJson) {
    const actions = (resultJson as { _actions: Array<{ type?: string }> })._actions;
    if (Array.isArray(actions)) return actions.map((a) => a?.type ?? '?');
  }
  return [];
}

/* ---------------------------- 结果卡片 ---------------------------- */

function ResultCard(props: {
  data: ResultCardData;
  onConfirm: () => void;
  onReplay: () => void;
}) {
  const r = () => props.data;
  const ok = () => r().status === 'ok';
  const needConfirm = () => r().status === 'confirm_required';
  const pretty = () => {
    const v = r().resultJson;
    if (v == null) return '';
    try {
      return JSON.stringify(v, null, 1);
    } catch {
      return String(v);
    }
  };
  return (
    <div
      style={sx({
        padding: 12,
        borderRadius: 11,
        border: '1px solid var(--hairline)',
        background: ok() ? 'var(--surface-sunken)' : needConfirm() ? 'var(--warning-soft)' : 'var(--error-soft)',
        display: 'flex',
        flexDirection: 'column',
        gap: 7,
        minWidth: 0,
      })}
    >
      <div style={sx({ display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: 8 })}>
        <span class="mono" style={sx({ fontSize: 11, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' })}>
          {r().deviceId}
        </span>
        <Badge variant={statusTone(r().status)} dot>{r().status}</Badge>
      </div>
      <div class="muted-3 mono" style={sx({ fontSize: 10.5 })}>
        {fmtClock(r().receivedAt)}
        <Show when={r().durationMs != null}> · {r().durationMs}ms</Show>
        <Show when={r().truncated}> · 截断</Show>
      </div>
      <Show
        when={!r().stderr || ok()}
        fallback={
          <div style={sx({ fontSize: 11, color: 'var(--error)', whiteSpace: 'pre-wrap', wordBreak: 'break-all' })}>
            {r().stderr}
          </div>
        }
      >
        <Show when={pretty()}>
          <pre
            class="mono"
            style={sx({
              margin: 0,
              fontSize: 10.5,
              maxHeight: 140,
              overflow: 'auto',
              whiteSpace: 'pre-wrap',
              wordBreak: 'break-all',
              color: 'var(--text-2)',
            })}
          >
            {pretty()}
          </pre>
        </Show>
      </Show>
      <div style={sx({ display: 'flex', gap: 6, marginTop: 'auto' })}>
        <Show when={needConfirm()}>
          <Btn size="xs" variant="warning" icon="shield" onClick={() => props.onConfirm()}>确认执行</Btn>
        </Show>
        <Btn size="xs" variant="ghost" icon="rotate" onClick={() => props.onReplay()}>回放</Btn>
      </div>
    </div>
  );
}

/* ---------------------------- 主页面 ---------------------------- */

export default function ProbePage() {
  const [mode, setMode] = createSignal<TargetMode>('single');
  const [singleDeviceId, setSingleDeviceId] = createSignal('');
  const [multiDeviceIds, setMultiDeviceIds] = createSignal('');
  const [script, setScript] = createSignal(DEFAULT_SCRIPT);
  // string 缓存中间态：避免清空时被 ||3000 立即回填
  const [timeoutMsInput, setTimeoutMsInput] = createSignal('3000');
  const [note, setNote] = createSignal('');
  const [sending, setSending] = createSignal(false);
  const [errMsg, setErrMsg] = createSignal<string | null>(null);
  const [results, setResults] = createSignal<ResultCardData[]>([]);
  const [completed, setCompleted] = createSignal<{ received: number; expected: number } | null>(null);
  const [currentBatch, setCurrentBatch] = createSignal<string | null>(null);
  const [confirmTarget, setConfirmTarget] = createSignal<ResultCardData | null>(null);
  const [recentBatches, setRecentBatches] = createSignal<ProbeExecutionRow[]>([]);
  // 二次确认 modal 状态
  const [suffix, setSuffix] = createSignal('');
  const [confirming, setConfirming] = createSignal(false);
  const [confirmErr, setConfirmErr] = createSignal<string | null>(null);

  let stopStream: (() => void) | undefined;
  let pollTimer: ReturnType<typeof setInterval> | undefined;
  // batch 防越界：旧 SSE 回调被新 handleSend 覆盖后必须直接丢弃
  let batchSeq = 0;

  onMount(() => void loadRecent());
  onCleanup(() => {
    stopStream?.();
    if (pollTimer) clearInterval(pollTimer);
  });

  // 错误聚类：按 status 聚合非 ok 结果
  const errorClusters = createMemo(() => {
    const buckets = new Map<string, number>();
    for (const r of results()) {
      if (r.status === 'ok') continue;
      buckets.set(r.status, (buckets.get(r.status) ?? 0) + 1);
    }
    const clusters = Array.from(buckets.entries())
      .map(([status, count]) => ({ status, count }))
      .sort((a, b) => b.count - a.count);
    return { clusters, total: clusters.reduce((a, b) => a + b.count, 0) };
  });

  const loadRecent = async () => {
    try {
      const { rows } = await probeApi.list({ limit: 30 });
      // 前端按 dispatchedAt desc 显式排序，不依赖后端隐式顺序；
      // 排序后每 batch 只保留最新一条作为代表（最近下发在前）。
      const sorted = [...rows].sort(
        (a, b) => new Date(b.dispatchedAt).getTime() - new Date(a.dispatchedAt).getTime(),
      );
      const seen = new Set<string>();
      const dedup = sorted.filter((r) => {
        if (seen.has(r.batchId)) return false;
        seen.add(r.batchId);
        return true;
      });
      setRecentBatches(dedup.slice(0, 10));
    } catch {
      // 后端 disabled 时无 list；安静忽略，dispatch 时会展示 PROBE_DISABLED 提示
    }
  };

  const resolvedTargets = (): { deviceIds?: string[]; allOnline?: boolean } | null => {
    if (mode() === 'single') {
      const id = singleDeviceId().trim();
      return id ? { deviceIds: [id] } : null;
    }
    if (mode() === 'multi') {
      const ids = multiDeviceIds()
        .split(/[\s,，]+/)
        .map((s) => s.trim())
        .filter(Boolean);
      return ids.length > 0 ? { deviceIds: ids } : null;
    }
    return { allOnline: true };
  };

  // 合并/覆盖同 requestId 的结果，并维护 ring buffer 软上限
  const upsertResult = (payload: ProbeResultEvent) => {
    setResults((prev) => {
      const without = prev.filter((r) => r.requestId !== payload.requestId);
      const next = [...without, { ...payload, receivedAt: Date.now() }];
      return next.length > RESULTS_BUFFER_CAP ? next.slice(next.length - RESULTS_BUFFER_CAP) : next;
    });
  };

  // SSE 失败时的兜底：轮询 probeApi.list(batchId) 拉全量结果
  const startPolling = (batchId: string, expected: number, myBatch: number) => {
    if (pollTimer) clearInterval(pollTimer);
    const tick = async () => {
      if (myBatch !== batchSeq) return;
      try {
        const { rows } = await probeApi.list({ batchId, limit: 200 });
        if (myBatch !== batchSeq) return;
        for (const row of rows) {
          if (row.status === 'dispatched' || row.status === 'pending') continue;
          upsertResult({
            deviceId: row.deviceId,
            requestId: row.id,
            batchId: row.batchId,
            status: row.status,
            resultJson: row.resultJson ? safeParse(row.resultJson) : undefined,
            stderr: row.stderr ?? undefined,
            durationMs: row.durationMs ?? undefined,
            truncated: row.truncated,
          });
        }
        const done = rows.filter((r) => r.completedAt != null).length;
        if (done >= expected && expected > 0) {
          setCompleted({ received: done, expected });
          setSending(false);
          if (pollTimer) clearInterval(pollTimer);
          void loadRecent();
        }
      } catch {
        /* 单次轮询失败忽略，下一拍重试 */
      }
    };
    void tick();
    pollTimer = setInterval(tick, 2000);
  };

  const handleSend = async () => {
    setErrMsg(null);
    setResults([]);
    setCompleted(null);
    stopStream?.();
    if (pollTimer) clearInterval(pollTimer);
    const myBatch = ++batchSeq;

    const targets = resolvedTargets();
    if (!targets) {
      setErrMsg('请填写 deviceId');
      return;
    }
    if (!script().trim()) {
      setErrMsg('script 不能为空');
      return;
    }

    // submit 时一次性把缓存 string → number（clamp 到 [100,10000]）
    const timeoutMs = Math.max(100, Math.min(10000, Number(timeoutMsInput()) || 3000));

    setSending(true);
    try {
      const res = await probeApi.dispatch({
        targets,
        script: script(),
        timeoutMs,
        note: note().trim() || undefined,
      });
      if (myBatch !== batchSeq) return; // 已被新 send 覆盖
      setCurrentBatch(res.batchId);

      if (res.dispatched.length === 0) {
        setErrMsg(`目标设备全部离线：${res.skippedOffline.join(', ') || '(none online)'}`);
        setSending(false);
        return;
      }

      toast.success('已下发探针', `匹配 ${res.dispatched.length} 设备，等待回传…`);
      const expected = res.dispatched.length;

      stopStream = connectProbeBatchStream(res.batchId, {
        onResult: (payload) => {
          if (myBatch !== batchSeq) return;
          upsertResult(payload);
        },
        onCompleted: (payload) => {
          if (myBatch !== batchSeq) return;
          setCompleted(payload);
          setSending(false);
          void loadRecent();
        },
        onError: () => {
          if (myBatch !== batchSeq) return;
          // SSE 断开 → 回退轮询，不打断用户
          toast.warning('实时流中断', '已切换为轮询拉取结果');
          startPolling(res.batchId, expected, myBatch);
        },
      });
    } catch (err: any) {
      if (myBatch !== batchSeq) return;
      const code = err?.code ?? err?.data?.error?.code;
      if (code === 'PROBE_DISABLED') {
        setErrMsg('远程探针未启用。请联系系统管理员设置 PROBE_ENABLED=true 后重启服务。');
      } else {
        setErrMsg(err?.message ?? String(err));
      }
      setSending(false);
    }
  };

  const exportBatchJson = () => {
    const data = { batchId: currentBatch(), script: script(), results: results() };
    const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `probe-${currentBatch() ?? 'batch'}.json`;
    a.click();
    URL.revokeObjectURL(url);
  };

  // 回放历史 batch：脚本 / timeout / note 回填编辑器
  const replayBatch = (row: ProbeExecutionRow) => {
    setScript(row.scriptBody);
    setTimeoutMsInput(String(row.timeoutMs));
    setNote(row.note ?? '');
    setMode('single');
    setSingleDeviceId(row.deviceId);
    window.scrollTo({ top: 0, behavior: 'smooth' });
    toast.info('已回填脚本', `batch ${row.batchId.slice(0, 8)}…`);
  };

  // 现场重放某结果卡片：回填脚本目标到该设备
  const handleReplay = (r: ResultCardData) => {
    setMode('single');
    setSingleDeviceId(r.deviceId);
    window.scrollTo({ top: 0, behavior: 'smooth' });
  };

  // 选模板填入脚本
  const pickTemplate = (name: string) => {
    const tpl = PROBE_TEMPLATES.find((t) => t.name === name);
    if (tpl) setScript(tpl.body);
  };

  /* ---------- 二次确认 ---------- */
  const expectedSuffix = () => (confirmTarget()?.deviceId ?? '').slice(-5);
  const suffixMatches = () =>
    suffix().length === expectedSuffix().length &&
    suffix().toLowerCase() === expectedSuffix().toLowerCase();

  const openConfirm = (r: ResultCardData) => {
    setSuffix('');
    setConfirmErr(null);
    setConfirmTarget(r);
  };
  const closeConfirm = () => {
    if (confirming()) return;
    setConfirmTarget(null);
  };
  const submitConfirm = async () => {
    const target = confirmTarget();
    if (!target || !suffixMatches()) return;
    setConfirmErr(null);
    setConfirming(true);
    try {
      await probeApi.confirm(target.requestId, { deviceIdSuffix: suffix() });
      // 本地剔除该结果（SSE/轮询会推后续 ok/err 事件）
      setResults((prev) => prev.filter((r) => r.requestId !== target.requestId));
      toast.success('已确认执行', target.deviceId.slice(-8));
      void loadRecent();
      setConfirmTarget(null);
    } catch (e: any) {
      setConfirmErr(e?.message ?? String(e));
    } finally {
      setConfirming(false);
    }
  };

  const audienceDesc = () => {
    if (mode() === 'single') return singleDeviceId().trim() || '未指定设备';
    if (mode() === 'multi') {
      const n = multiDeviceIds().split(/[\s,，]+/).filter(Boolean).length;
      return `${n} 个设备`;
    }
    return '全部在线设备';
  };

  return (
    <div class="fade-up">
      <PageHead
        title="远程探针"
        eyebrow="脱敏 + ring buffer · 沙箱执行"
        desc="向在线设备下发受控诊断脚本并实时收集回传。脚本在客户端 Worker 沙箱内通过白名单 ctx 执行，全程写入审计；含 D 类受控写需二次确认。"
        right={
          <Btn variant="secondary" icon="refresh" onClick={() => void loadRecent()}>
            刷新历史
          </Btn>
        }
      />

      <div
        class="grid-collapse"
        style={sx({ display: 'grid', gridTemplateColumns: 'minmax(0,1fr) minmax(0,1.2fr)', gap: 16 })}
      >
        {/* ---------------- 下发 ---------------- */}
        <Panel title="下发" sub="脚本 + 受众">
          <div style={sx({ display: 'flex', flexDirection: 'column', gap: 14 })}>
            {/* 模板 */}
            <Field label="脚本模板">
              <div style={sx({ display: 'flex', gap: 7, flexWrap: 'wrap' })}>
                <For each={PROBE_TEMPLATES}>
                  {(t) => (
                    <button
                      class="badge badge-default"
                      title={t.description}
                      style={sx({ cursor: 'pointer', padding: '5px 11px' })}
                      onClick={() => pickTemplate(t.name)}
                    >
                      {t.name}
                    </button>
                  )}
                </For>
              </div>
            </Field>

            {/* 受众目标 */}
            <Field label="受众目标">
              <Seg
                options={[
                  { value: 'single', label: '单设备' },
                  { value: 'multi', label: '多设备' },
                  { value: 'allOnline', label: '全部在线' },
                ]}
                value={mode()}
                onChange={(v) => setMode(v as TargetMode)}
              />
              <div style={sx({ marginTop: 10 })}>
                <Show when={mode() === 'single'}>
                  <input
                    class="input mono"
                    style={sx({ fontSize: 12 })}
                    value={singleDeviceId()}
                    onInput={(e) => setSingleDeviceId(e.currentTarget.value)}
                    placeholder="设备 ID"
                    spellcheck={false}
                  />
                </Show>
                <Show when={mode() === 'multi'}>
                  <textarea
                    class="textarea mono"
                    rows={3}
                    style={sx({ fontSize: 12, lineHeight: 1.6 })}
                    value={multiDeviceIds()}
                    onInput={(e) => setMultiDeviceIds(e.currentTarget.value)}
                    placeholder="多个 deviceId，用空格 / 逗号分隔"
                    spellcheck={false}
                  />
                </Show>
                <Show when={mode() === 'allOnline'}>
                  <div
                    style={sx({
                      display: 'flex',
                      alignItems: 'center',
                      gap: 8,
                      padding: '9px 12px',
                      borderRadius: 10,
                      border: '1px solid color-mix(in oklch, var(--warning) 30%, transparent)',
                      background: 'var(--warning-soft)',
                      color: 'var(--warning)',
                      fontSize: 12,
                    })}
                  >
                    将下发到当前所有在线设备
                  </div>
                </Show>
              </div>
            </Field>

            {/* 脚本编辑器 */}
            <Field label="诊断脚本 (JS · 白名单 ctx)">
              <textarea
                class="textarea mono"
                rows={9}
                style={sx({ fontSize: 12, lineHeight: 1.6 })}
                value={script()}
                onInput={(e) => setScript(e.currentTarget.value)}
                spellcheck={false}
              />
            </Field>

            {/* timeout + note */}
            <div style={sx({ display: 'grid', gridTemplateColumns: '1fr 1.4fr', gap: 12 })}>
              <Field label="timeout (ms)">
                <input
                  class="input mono"
                  type="number"
                  min={100}
                  max={10000}
                  value={timeoutMsInput()}
                  onInput={(e) => setTimeoutMsInput(e.currentTarget.value)}
                />
              </Field>
              <Field label="note（可选）">
                <input
                  class="input"
                  value={note()}
                  onInput={(e) => setNote(e.currentTarget.value)}
                  placeholder="排查 X 用户的 OOM"
                />
              </Field>
            </div>

            <div
              style={sx({
                padding: '9px 12px',
                borderRadius: 10,
                background: 'var(--warning-soft)',
                color: 'var(--warning)',
                fontSize: 11.5,
                lineHeight: 1.5,
              })}
            >
              ⚠ 脚本将在目标设备沙箱执行，请勿采集敏感信息；操作全程写入审计日志。
            </div>

            <div style={sx({ display: 'flex', alignItems: 'center', gap: 10, flexWrap: 'wrap' })}>
              <Btn variant="primary" icon="send" onClick={() => void handleSend()} disabled={sending()}>
                {sending() ? '下发中…' : '下发探针'}
              </Btn>
              <Show when={errMsg()}>
                <div
                  class="fade-in"
                  style={sx({
                    display: 'inline-flex',
                    alignItems: 'center',
                    gap: 6,
                    padding: '4px 10px',
                    borderRadius: 8,
                    border: '1px solid color-mix(in oklch, var(--error) 30%, transparent)',
                    background: 'var(--error-soft)',
                    color: 'var(--error)',
                    fontSize: 12,
                  })}
                >
                  {errMsg()}
                </div>
              </Show>
            </div>
          </div>
        </Panel>

        {/* ---------------- 结果 ---------------- */}
        <Panel
          title="结果"
          sub={results().length > 0 ? `${results().length} 个回传` : '等待下发'}
          right={
            <Show when={currentBatch()}>
              <div style={sx({ display: 'flex', alignItems: 'center', gap: 8 })}>
                <span class="mono muted-3" style={sx({ fontSize: 11 })}>
                  batch={currentBatch()!.slice(0, 8)}…
                  <Show when={completed()}>
                    {' '}· {completed()!.received}/{completed()!.expected}
                  </Show>
                </span>
                <Show when={results().length > 0}>
                  <Btn size="xs" variant="ghost" icon="download" onClick={exportBatchJson}>导出 JSON</Btn>
                  <Btn size="xs" variant="ghost" icon="x" onClick={() => setResults([])}>清空</Btn>
                </Show>
              </div>
            </Show>
          }
        >
          {/* ring buffer + 错误聚类 */}
          <Show when={results().length > 0}>
            <div
              style={sx({
                display: 'grid',
                gridTemplateColumns: '1fr 1fr',
                gap: 10,
                marginBottom: 12,
              })}
            >
              <div style={sx({ border: '1px solid var(--hairline)', borderRadius: 10, padding: '8px 11px' })}>
                <div style={sx({ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 6 })}>
                  <span class="eyebrow">RING BUFFER</span>
                  <span class="mono tnum" style={sx({ fontSize: 11.5 })}>{results().length} / {RESULTS_BUFFER_CAP}</span>
                </div>
                <Progress
                  value={results().length}
                  max={RESULTS_BUFFER_CAP}
                  tone={results().length >= RESULTS_BUFFER_CAP * 0.9 ? 'warning' : 'accent'}
                  height={6}
                />
                <div class="muted-3 mono" style={sx({ display: 'flex', justifyContent: 'space-between', marginTop: 5, fontSize: 10.5 })}>
                  <span>{fmtClock(results()[0].receivedAt)}</span>
                  <span>{fmtClock(results()[results().length - 1].receivedAt)}</span>
                </div>
              </div>
              <div style={sx({ border: '1px solid var(--hairline)', borderRadius: 10, padding: '8px 11px' })}>
                <div style={sx({ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 6 })}>
                  <span class="eyebrow">错误聚类</span>
                  <span class="mono tnum" style={sx({ fontSize: 11.5 })}>{errorClusters().total} / {results().length}</span>
                </div>
                <Show
                  when={errorClusters().clusters.length > 0}
                  fallback={<div class="muted-3" style={sx({ fontSize: 11.5 })}>全部 ok</div>}
                >
                  <div style={sx({ display: 'flex', flexDirection: 'column', gap: 2 })}>
                    <For each={errorClusters().clusters}>
                      {(c) => (
                        <div class="mono" style={sx({ display: 'flex', justifyContent: 'space-between', fontSize: 11 })}>
                          <span>{c.status}</span>
                          <span class="muted-3">×{c.count}</span>
                        </div>
                      )}
                    </For>
                  </div>
                </Show>
              </div>
            </div>
          </Show>

          <Show
            when={!sending() || results().length > 0}
            fallback={<Loading h={300} />}
          >
            <Show
              when={results().length > 0}
              fallback={
                <Empty
                  title="尚无结果"
                  desc="点击「下发探针」后，实时结果会在此以卡片网格展示。"
                  icon="remote"
                />
              }
            >
              <div style={sx({ display: 'grid', gridTemplateColumns: 'repeat(auto-fill,minmax(200px,1fr))', gap: 10 })}>
                <For each={results()}>
                  {(r) => (
                    <ResultCard
                      data={r}
                      onConfirm={() => openConfirm(r)}
                      onReplay={() => handleReplay(r)}
                    />
                  )}
                </For>
              </div>
            </Show>
          </Show>
        </Panel>
      </div>

      {/* ---------------- 最近 batch ---------------- */}
      <Panel title="最近 batch" sub="最近 10 次下发 · 点回放回填脚本" style={sx({ marginTop: 16 })}>
        <Show
          when={recentBatches().length > 0}
          fallback={<Empty title="无历史" desc="下发探针后，历史 batch 会在此列出。" icon="inbox" />}
        >
          <div style={sx({ overflowX: 'auto' })}>
            <table class="tbl">
              <thead>
                <tr>
                  <th>batch</th>
                  <th>设备</th>
                  <th>操作者</th>
                  <th>状态</th>
                  <th>note</th>
                  <th>下发时间</th>
                  <th style={sx({ textAlign: 'right' })}>操作</th>
                </tr>
              </thead>
              <tbody>
                <For each={recentBatches()}>
                  {(row) => (
                    <tr>
                      <td class="mono" style={sx({ fontWeight: 600 })}>{row.batchId.slice(0, 8)}…</td>
                      <td class="mono" style={sx({ fontSize: 11.5 })}>{row.deviceId.slice(-12)}</td>
                      <td style={sx({ fontSize: 12 })}>{row.adminUsername || '—'}</td>
                      <td><Badge variant={statusTone(row.status)}>{row.status}</Badge></td>
                      <td class="muted" style={sx({ fontSize: 12, maxWidth: 180, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' })}>
                        {row.note || '—'}
                      </td>
                      <td class="muted-3" style={sx({ fontSize: 11.5 })}>{fmtAgo(row.dispatchedAt)}</td>
                      <td>
                        <div style={sx({ display: 'flex', gap: 6, justifyContent: 'flex-end' })}>
                          <Btn size="xs" variant="secondary" icon="rotate" onClick={() => replayBatch(row)}>回放</Btn>
                        </div>
                      </td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          </div>
        </Show>
      </Panel>

      {/* ---------------- D 类二次确认 ---------------- */}
      <Modal
        open={confirmTarget() !== null}
        onClose={closeConfirm}
        title="确认远程受控写"
        size="sm"
        footer={
          <>
            <Btn variant="ghost" onClick={closeConfirm} disabled={confirming()}>取消</Btn>
            <Btn
              variant="danger"
              icon="shield"
              onClick={() => void submitConfirm()}
              disabled={!suffixMatches() || confirming()}
            >
              {confirming() ? '提交中…' : '确认执行'}
            </Btn>
          </>
        }
      >
        <Show when={confirmTarget()}>
          {(t) => (
            <div style={sx({ display: 'flex', flexDirection: 'column', gap: 12, fontSize: 13 })}>
              <div class="muted" style={sx({ fontSize: 13, lineHeight: 1.6 })}>
                即将对目标设备执行受控动作：
                <span class="mono" style={sx({ marginLeft: 4, color: 'var(--warning)' })}>
                  {extractActionsPreview(t().resultJson).join(', ') || '(empty)'}
                </span>
              </div>
              <div
                class="mono"
                style={sx({
                  padding: '10px 12px',
                  borderRadius: 8,
                  background: 'var(--surface-sunken)',
                  fontSize: 11.5,
                  wordBreak: 'break-all',
                })}
              >
                deviceId = {t().deviceId}
              </div>
              <Field label="输入该 device 最后 5 位以确认">
                <input
                  class="input mono"
                  maxLength={5}
                  style={sx({ letterSpacing: '0.18em' })}
                  value={suffix()}
                  onInput={(e) => setSuffix(e.currentTarget.value)}
                  placeholder="后 5 位"
                  spellcheck={false}
                />
              </Field>
              <Show when={suffix().length === 5 && !suffixMatches()}>
                <div style={sx({ fontSize: 12, color: 'var(--error)' })}>后 5 位不匹配</div>
              </Show>
              <Show when={confirmErr()}>
                <div style={sx({ fontSize: 12, color: 'var(--error)' })}>{confirmErr()}</div>
              </Show>
            </div>
          )}
        </Show>
      </Modal>
    </div>
  );
}

function safeParse(s: string): unknown {
  try {
    return JSON.parse(s);
  } catch {
    return s;
  }
}
