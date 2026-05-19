/**
 * 远程探针 admin REPL 控制台（M5 完整版）：
 *   - Target 三模式：单设备 / 多设备 chips / 全部在线（broadcast）
 *   - ScriptEditor（CodeMirror 6 JS mode）+ 模板下拉一键填入
 *   - SSE 实时结果卡片网格（ResultCard 组件）
 *   - [导出 batch JSON] 文件下载
 *   - 历史侧栏：最近 10 batch，点「回放此 script」回填编辑器
 *   - D 类 confirm_required 弹 ConfirmDialog
 */

import { createSignal, For, Show, onCleanup, onMount } from 'solid-js';
import { connectProbeBatchStream, probeApi, type ProbeResultEvent, type ProbeExecutionRow } from '@/api/probe';
import ConfirmDialog from '@/components/probe/ConfirmDialog';
import ResultCard from '@/components/probe/ResultCard';
import ScriptEditor from '@/components/probe/ScriptEditor';
import { PROBE_TEMPLATES } from './probe-templates';

interface ResultCardData extends ProbeResultEvent {
  receivedAt: number;
}

type TargetMode = 'single' | 'multi' | 'allOnline';

const DEFAULT_SCRIPT = `return {
  ua: ctx.nav.ua,
  lang: ctx.nav.language,
  mem: ctx.perf.memoryMB(),
};`;

export default function ProbePage() {
  const [mode, setMode] = createSignal<TargetMode>('single');
  const [singleDeviceId, setSingleDeviceId] = createSignal('');
  const [multiDeviceIds, setMultiDeviceIds] = createSignal('');
  const [script, setScript] = createSignal(DEFAULT_SCRIPT);
  const [timeoutMs, setTimeoutMs] = createSignal(3000);
  const [note, setNote] = createSignal('');
  const [sending, setSending] = createSignal(false);
  const [errMsg, setErrMsg] = createSignal<string | null>(null);
  const [results, setResults] = createSignal<ResultCardData[]>([]);
  const [completed, setCompleted] = createSignal<{ received: number; expected: number } | null>(null);
  const [currentBatch, setCurrentBatch] = createSignal<string | null>(null);
  const [confirmTarget, setConfirmTarget] = createSignal<ResultCardData | null>(null);
  const [recentBatches, setRecentBatches] = createSignal<ProbeExecutionRow[]>([]);
  let stopStream: (() => void) | undefined;

  onMount(() => void loadRecent());
  onCleanup(() => stopStream?.());

  const loadRecent = async () => {
    try {
      const { rows } = await probeApi.list({ limit: 10 });
      // 每 batch 只保留第一条（按 dispatched_at desc，第一条代表 batch 起始）
      const seen = new Set<string>();
      const dedup = rows.filter((r) => {
        if (seen.has(r.batchId)) return false;
        seen.add(r.batchId);
        return true;
      });
      setRecentBatches(dedup);
    } catch {
      // 后端 disabled 时无 list；安静忽略，dispatch 时会展示 PROBE_DISABLED 提示
    }
  };

  const resolvedTargets = () => {
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

  const handleSend = async () => {
    setErrMsg(null);
    setResults([]);
    setCompleted(null);
    stopStream?.();

    const targets = resolvedTargets();
    if (!targets) {
      setErrMsg('请填写 deviceId');
      return;
    }
    if (!script().trim()) {
      setErrMsg('script 不能为空');
      return;
    }

    setSending(true);
    try {
      const res = await probeApi.dispatch({
        targets,
        script: script(),
        timeoutMs: timeoutMs(),
        note: note().trim() || undefined,
      });
      setCurrentBatch(res.batchId);

      if (res.dispatched.length === 0) {
        setErrMsg(`目标设备全部离线：${res.skippedOffline.join(', ') || '(none online)'}`);
        setSending(false);
        return;
      }

      stopStream = connectProbeBatchStream(res.batchId, {
        onResult: (payload) => {
          // 同 requestId 的新结果覆盖旧的（confirm_required → ok 等场景）
          setResults((prev) => {
            const without = prev.filter((r) => r.requestId !== payload.requestId);
            return [...without, { ...payload, receivedAt: Date.now() }];
          });
        },
        onCompleted: (payload) => {
          setCompleted(payload);
          setSending(false);
          void loadRecent();
        },
        onError: (err) => {
          setErrMsg(`SSE 错误：${String(err)}`);
          setSending(false);
        },
      });
    } catch (err: any) {
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
    const data = {
      batchId: currentBatch(),
      script: script(),
      results: results(),
    };
    const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `probe-${currentBatch() ?? 'batch'}.json`;
    a.click();
    URL.revokeObjectURL(url);
  };

  const replayBatch = (row: ProbeExecutionRow) => {
    setScript(row.scriptBody);
    setTimeoutMs(row.timeoutMs);
    setNote(row.note ?? '');
  };

  return (
    <div class="space-y-6">
      <header class="space-y-1">
        <h1 class="text-2xl font-semibold">远程探针</h1>
        <p class="text-sm text-content-secondary">
          在客户端 Worker 沙箱里执行 JS 表达式，通过白名单 ctx 读取诊断信息。
          含 D 类受控写（reload/clearCache/signOut）需二次确认。
        </p>
      </header>

      <div class="grid gap-6 md:grid-cols-[1fr_280px]">
        <div class="space-y-6">
          <section class="rounded-lg border border-border-hairline bg-surface p-4 space-y-3">
            <h2 class="font-medium">下发</h2>

            <div class="space-y-2">
              <div class="text-sm text-content-secondary">Target</div>
              <div class="flex flex-wrap items-center gap-3 text-sm">
                <For each={[['single', '单设备'], ['multi', '多设备'], ['allOnline', '全部在线']] as const}>
                  {([v, label]) => (
                    <label class="flex items-center gap-1.5 cursor-pointer">
                      <input
                        type="radio"
                        name="target-mode"
                        checked={mode() === v}
                        onChange={() => setMode(v)}
                      />
                      <span>{label}</span>
                    </label>
                  )}
                </For>
              </div>
              <Show when={mode() === 'single'}>
                <input
                  class="w-full rounded border border-border-hairline bg-surface-secondary px-2 py-1.5 font-mono text-xs"
                  value={singleDeviceId()}
                  onInput={(e) => setSingleDeviceId(e.currentTarget.value)}
                  placeholder="设备 ID"
                />
              </Show>
              <Show when={mode() === 'multi'}>
                <textarea
                  rows={3}
                  class="w-full rounded border border-border-hairline bg-surface-secondary px-2 py-1.5 font-mono text-xs"
                  value={multiDeviceIds()}
                  onInput={(e) => setMultiDeviceIds(e.currentTarget.value)}
                  placeholder="多个 deviceId，用空格 / 逗号分隔"
                />
              </Show>
              <Show when={mode() === 'allOnline'}>
                <p class="rounded bg-status-warning-light px-2 py-1.5 text-xs text-status-warning">
                  ⚠ 将下发到当前所有在线设备
                </p>
              </Show>
            </div>

            <div class="space-y-1">
              <div class="flex items-center justify-between text-sm">
                <span class="text-content-secondary">script</span>
                <select
                  class="text-xs rounded border border-border-hairline bg-surface-secondary px-2 py-1"
                  onChange={(e) => {
                    const tpl = PROBE_TEMPLATES.find((t) => t.name === e.currentTarget.value);
                    if (tpl) setScript(tpl.body);
                    e.currentTarget.value = '';
                  }}
                >
                  <option value="">📋 模板</option>
                  <For each={PROBE_TEMPLATES}>
                    {(tpl) => (
                      <option value={tpl.name} title={tpl.description}>
                        {tpl.name}
                      </option>
                    )}
                  </For>
                </select>
              </div>
              <ScriptEditor value={script()} onChange={setScript} minHeightPx={200} />
            </div>

            <div class="grid gap-3 sm:grid-cols-2">
              <label class="flex flex-col gap-1 text-sm">
                <span class="text-content-secondary">timeout (ms)</span>
                <input
                  type="number"
                  min={100}
                  max={10000}
                  class="rounded border border-border-hairline bg-surface-secondary px-2 py-1.5 font-mono text-xs"
                  value={timeoutMs()}
                  onInput={(e) => setTimeoutMs(Number(e.currentTarget.value) || 3000)}
                />
              </label>
              <label class="flex flex-col gap-1 text-sm">
                <span class="text-content-secondary">note（可选）</span>
                <input
                  class="rounded border border-border-hairline bg-surface-secondary px-2 py-1.5 text-xs"
                  value={note()}
                  onInput={(e) => setNote(e.currentTarget.value)}
                  placeholder="排查 X 用户的 OOM"
                />
              </label>
            </div>

            <div class="flex items-center gap-3">
              <button
                type="button"
                class="rounded bg-accent px-3 py-1.5 text-sm font-medium text-white disabled:opacity-50"
                disabled={sending()}
                onClick={handleSend}
              >
                {sending() ? '发送中…' : '发送'}
              </button>
              <Show when={errMsg()}>
                <span class="text-sm text-status-danger">{errMsg()}</span>
              </Show>
            </div>
          </section>

          <section class="rounded-lg border border-border-hairline bg-surface p-4 space-y-3">
            <div class="flex items-baseline justify-between">
              <h2 class="font-medium">结果</h2>
              <div class="flex items-center gap-2">
                <Show when={currentBatch()}>
                  <span class="font-mono text-xs text-content-tertiary">
                    batch={currentBatch()}
                    <Show when={completed()}>
                      {' '}· 完成 {completed()!.received}/{completed()!.expected}
                    </Show>
                  </span>
                </Show>
                <Show when={results().length > 0}>
                  <button
                    type="button"
                    class="rounded border border-border-hairline px-2 py-0.5 text-xs hover:bg-surface-secondary"
                    onClick={exportBatchJson}
                  >
                    导出 JSON
                  </button>
                </Show>
              </div>
            </div>
            <Show when={results().length > 0} fallback={<p class="text-sm text-content-tertiary">尚无结果</p>}>
              <div class="grid gap-3 md:grid-cols-2">
                <For each={results()}>
                  {(r) => (
                    <ResultCard
                      deviceId={r.deviceId}
                      requestId={r.requestId}
                      status={r.status}
                      durationMs={r.durationMs}
                      truncated={r.truncated}
                      resultJson={r.resultJson}
                      stderr={r.stderr}
                      onConfirmClick={() => setConfirmTarget(r)}
                    />
                  )}
                </For>
              </div>
            </Show>
          </section>
        </div>

        <aside class="rounded-lg border border-border-hairline bg-surface p-4 space-y-2 md:sticky md:top-4 md:self-start">
          <h3 class="text-sm font-medium">最近 batch</h3>
          <Show when={recentBatches().length > 0} fallback={<p class="text-xs text-content-tertiary">无历史</p>}>
            <ul class="space-y-2 text-xs">
              <For each={recentBatches()}>
                {(row) => (
                  <li class="rounded border border-border-hairline p-2 space-y-1">
                    <div class="flex justify-between gap-2">
                      <span class="font-mono truncate">{row.batchId.slice(0, 8)}…</span>
                      <span class="text-content-tertiary">{formatTime(row.dispatchedAt)}</span>
                    </div>
                    <Show when={row.note}>
                      <p class="text-content-secondary truncate" title={row.note ?? ''}>
                        {row.note}
                      </p>
                    </Show>
                    <button
                      type="button"
                      class="w-full rounded border border-border-hairline px-2 py-0.5 hover:bg-surface-secondary"
                      onClick={() => replayBatch(row)}
                    >
                      回放此 script
                    </button>
                  </li>
                )}
              </For>
            </ul>
          </Show>
        </aside>
      </div>

      <Show when={confirmTarget()}>
        <ConfirmDialog
          open={true}
          requestId={confirmTarget()!.requestId}
          deviceId={confirmTarget()!.deviceId}
          actionsPreview={extractActionsPreview(confirmTarget()!.resultJson)}
          onClose={() => setConfirmTarget(null)}
          onConfirmed={() => {
            setResults((prev) => prev.filter((r) => r.requestId !== confirmTarget()!.requestId));
          }}
        />
      </Show>
    </div>
  );
}

function extractActionsPreview(resultJson: unknown): string[] {
  if (resultJson && typeof resultJson === 'object' && '_actions' in resultJson) {
    const actions = (resultJson as { _actions: Array<{ type: string }> })._actions;
    if (Array.isArray(actions)) return actions.map((a) => a.type);
  }
  return [];
}

function formatTime(iso: string): string {
  try {
    const d = new Date(iso);
    return `${d.getHours()}:${String(d.getMinutes()).padStart(2, '0')}`;
  } catch {
    return iso;
  }
}
