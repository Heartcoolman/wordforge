/**
 * 远程探针 admin REPL 控制台（M1 最小版）：
 *   - 单设备目标输入框（M4 扩展 multi + all_online）
 *   - textarea script 编辑器（M5 升级为 CodeMirror）
 *   - 发送按钮 → POST /api/admin/probe → SSE 拉结果 → 显示卡片
 */

import { createSignal, For, Show, onCleanup } from 'solid-js';
import { connectProbeBatchStream, probeApi, type ProbeResultEvent } from '@/api/probe';
import ConfirmDialog from '@/components/probe/ConfirmDialog';

interface ResultCardData extends ProbeResultEvent {
  receivedAt: number;
}

export default function ProbePage() {
  const [deviceId, setDeviceId] = createSignal('');
  const [script, setScript] = createSignal(`return {
  ua: ctx.nav.ua,
  lang: ctx.nav.language,
  platform: ctx.nav.platform,
  now: ctx.time.now,
};`);
  const [timeoutMs, setTimeoutMs] = createSignal(3000);
  const [note, setNote] = createSignal('');
  const [sending, setSending] = createSignal(false);
  const [errMsg, setErrMsg] = createSignal<string | null>(null);
  const [results, setResults] = createSignal<ResultCardData[]>([]);
  const [completed, setCompleted] = createSignal<{ received: number; expected: number } | null>(null);
  const [currentBatch, setCurrentBatch] = createSignal<string | null>(null);
  const [confirmTarget, setConfirmTarget] = createSignal<ResultCardData | null>(null);
  let stopStream: (() => void) | undefined;

  onCleanup(() => stopStream?.());

  const handleSend = async () => {
    setErrMsg(null);
    setResults([]);
    setCompleted(null);
    stopStream?.();

    if (!deviceId().trim()) {
      setErrMsg('请输入 deviceId');
      return;
    }
    if (!script().trim()) {
      setErrMsg('script 不能为空');
      return;
    }

    setSending(true);
    try {
      const res = await probeApi.dispatch({
        targets: { deviceIds: [deviceId().trim()] },
        script: script(),
        timeoutMs: timeoutMs(),
        note: note().trim() || undefined,
      });
      setCurrentBatch(res.batchId);

      if (res.dispatched.length === 0) {
        setErrMsg(`目标设备离线：${res.skippedOffline.join(', ') || '(unknown)'}`);
        setSending(false);
        return;
      }

      // 订阅 batch stream
      stopStream = connectProbeBatchStream(res.batchId, {
        onResult: (payload) => {
          setResults((prev) => [...prev, { ...payload, receivedAt: Date.now() }]);
        },
        onCompleted: (payload) => {
          setCompleted(payload);
          setSending(false);
        },
        onError: (err) => {
          setErrMsg(`SSE 错误：${String(err)}`);
          setSending(false);
        },
      });
    } catch (err: any) {
      // 后端 enabled=false 时 503 + code=PROBE_DISABLED；以友好提示展示
      const code = err?.code ?? err?.data?.error?.code;
      if (code === 'PROBE_DISABLED') {
        setErrMsg('远程探针未启用。请联系系统管理员设置 PROBE_ENABLED=true 后重启服务。');
      } else {
        setErrMsg(err?.message ?? String(err));
      }
      setSending(false);
    }
  };

  return (
    <div class="space-y-6">
      <header class="space-y-1">
        <h1 class="text-2xl font-semibold">远程探针</h1>
        <p class="text-sm text-content-secondary">
          在客户端 Worker 沙箱里执行 JS 表达式，通过白名单 ctx 读取诊断信息。
          M1 阶段仅支持单设备 + 最小 ctx（nav / time）。
        </p>
      </header>

      <section class="rounded-lg border border-border-hairline bg-surface p-4 space-y-3">
        <h2 class="font-medium">下发</h2>
        <div class="grid gap-3 md:grid-cols-2">
          <label class="flex flex-col gap-1 text-sm">
            <span class="text-content-secondary">目标 deviceId</span>
            <input
              class="rounded border border-border-hairline bg-surface-secondary px-2 py-1.5 font-mono text-xs"
              value={deviceId()}
              onInput={(e) => setDeviceId(e.currentTarget.value)}
              placeholder="设备 ID（在客户端浏览器 localStorage 里）"
            />
          </label>
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
        </div>
        <label class="flex flex-col gap-1 text-sm">
          <span class="text-content-secondary">script（JS 函数体，可用 ctx 参数）</span>
          <textarea
            rows={8}
            class="rounded border border-border-hairline bg-surface-secondary px-2 py-2 font-mono text-xs"
            value={script()}
            onInput={(e) => setScript(e.currentTarget.value)}
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
          <Show when={currentBatch()}>
            <span class="font-mono text-xs text-content-tertiary">
              batch={currentBatch()}
              <Show when={completed()}>
                {' '}· 完成 {completed()!.received}/{completed()!.expected}
              </Show>
            </span>
          </Show>
        </div>
        <Show when={results().length > 0} fallback={<p class="text-sm text-content-tertiary">尚无结果</p>}>
          <div class="grid gap-3 md:grid-cols-2">
            <For each={results()}>
              {(r) => (
                <article class="rounded border border-border-hairline bg-surface-secondary p-3 space-y-2">
                  <header class="flex items-center justify-between text-xs">
                    <span class="font-mono">{r.deviceId}</span>
                    <span
                      class={
                        r.status === 'ok'
                          ? 'rounded bg-status-success-light px-1.5 py-0.5 text-status-success'
                          : r.status === 'timeout'
                          ? 'rounded bg-status-warning-light px-1.5 py-0.5 text-status-warning'
                          : 'rounded bg-status-danger-light px-1.5 py-0.5 text-status-danger'
                      }
                    >
                      {r.status}
                      {r.truncated ? ' · truncated' : ''}
                      {' · '}
                      {r.durationMs ?? '?'}ms
                    </span>
                  </header>
                  <Show when={r.resultJson !== undefined && r.resultJson !== null}>
                    <pre class="max-h-64 overflow-auto rounded bg-black/5 p-2 text-[11px] leading-snug">
                      {JSON.stringify(r.resultJson, null, 2)}
                    </pre>
                  </Show>
                  <Show when={r.stderr}>
                    <pre class="max-h-32 overflow-auto rounded bg-status-danger-light p-2 text-[11px] leading-snug text-status-danger">
                      {r.stderr}
                    </pre>
                  </Show>
                  <Show when={r.status === 'confirm_required'}>
                    <button
                      type="button"
                      class="rounded bg-status-warning px-2 py-1 text-xs font-medium text-white"
                      onClick={() => setConfirmTarget(r)}
                    >
                      确认执行
                    </button>
                  </Show>
                </article>
              )}
            </For>
          </div>
        </Show>
      </section>

      <Show when={confirmTarget()}>
        <ConfirmDialog
          open={true}
          requestId={confirmTarget()!.requestId}
          deviceId={confirmTarget()!.deviceId}
          actionsPreview={extractActionsPreview(confirmTarget()!.resultJson)}
          onClose={() => setConfirmTarget(null)}
          onConfirmed={() => {
            // 确认后等客户端重跑回 result，原卡片会被新 result 覆盖（按 requestId 去重）
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
