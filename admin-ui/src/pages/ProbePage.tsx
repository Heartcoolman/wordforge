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
import { Card } from '@/components/ui/Card';
import { HeroCard } from '@/components/ui/HeroCard';
import { Button } from '@/components/ui/Button';
import { Input, TextArea } from '@/components/ui/Input';
import { Empty } from '@/components/ui/Empty';

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
  // string 缓存中间态：避免清空时被 ||3000 立即回填
  const [timeoutMsInput, setTimeoutMsInput] = createSignal('3000');
  const [note, setNote] = createSignal('');
  const [templateKey, setTemplateKey] = createSignal('');
  const [sending, setSending] = createSignal(false);
  const [errMsg, setErrMsg] = createSignal<string | null>(null);
  const [results, setResults] = createSignal<ResultCardData[]>([]);
  const [completed, setCompleted] = createSignal<{ received: number; expected: number } | null>(null);
  const [currentBatch, setCurrentBatch] = createSignal<string | null>(null);
  const [confirmTarget, setConfirmTarget] = createSignal<ResultCardData | null>(null);
  const [recentBatches, setRecentBatches] = createSignal<ProbeExecutionRow[]>([]);
  let stopStream: (() => void) | undefined;
  // batch 防越界：旧 SSE 回调被新 handleSend 覆盖后必须直接丢弃
  let batchSeq = 0;

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

    // submit 时一次性把缓存的 string → number（clamp 到 [100,10000]）
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

      stopStream = connectProbeBatchStream(res.batchId, {
        onResult: (payload) => {
          if (myBatch !== batchSeq) return;
          // 同 requestId 的新结果覆盖旧的（confirm_required → ok 等场景）
          setResults((prev) => {
            const without = prev.filter((r) => r.requestId !== payload.requestId);
            return [...without, { ...payload, receivedAt: Date.now() }];
          });
        },
        onCompleted: (payload) => {
          if (myBatch !== batchSeq) return;
          setCompleted(payload);
          setSending(false);
          void loadRecent();
        },
        onError: (err) => {
          if (myBatch !== batchSeq) return;
          setErrMsg(`SSE 错误：${String(err)}`);
          setSending(false);
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
    setTimeoutMsInput(String(row.timeoutMs));
    setNote(row.note ?? '');
  };

  return (
    <div class="space-y-6 animate-fade-in">
      <HeroCard
        eyebrow="脱敏 + ringbuffer"
        eyebrowVariant="accent"
        title="远程探针"
        desc="在客户端 Worker 沙箱里执行 JS 表达式，通过白名单 ctx 读取诊断信息。含 D 类受控写（reload/clearCache/signOut）需二次确认。"
      />

      <div class="grid gap-6 lg:grid-cols-[1fr_300px]">
        <div class="space-y-6">
          <Card variant="elevated" padding="md">
            <div class="space-y-5">
              <h2 class="text-headline text-content">下发</h2>

              {/* Target — segmented control，与 WindowPicker 同设计语言 */}
              <div class="space-y-2.5">
                <div class="text-sm font-medium text-content-secondary">Target</div>
                <div class="inline-flex items-center gap-1 rounded-lg bg-surface-secondary p-1">
                  <For
                    each={
                      [
                        ['single', '单设备'],
                        ['multi', '多设备'],
                        ['allOnline', '全部在线'],
                      ] as const
                    }
                  >
                    {([v, label]) => (
                      <button
                        type="button"
                        onClick={() => setMode(v)}
                        class={`px-3 py-1.5 text-sm rounded-md transition-[background-color,color,box-shadow] duration-fast ease-out-expo ${
                          mode() === v
                            ? 'bg-surface-elevated text-content shadow-elevation-1'
                            : 'text-content-secondary hover:text-content'
                        }`}
                      >
                        {label}
                      </button>
                    )}
                  </For>
                </div>

                {/* 三态切换：min-h 防止 mode 切换时高度跳变，wrapper 加 fade-in */}
                <div class="min-h-[4.5rem]">
                  <Show when={mode() === 'single'}>
                    <div class="animate-fade-in">
                      <Input
                        class="font-mono text-xs"
                        value={singleDeviceId()}
                        onInput={(e) => setSingleDeviceId(e.currentTarget.value)}
                        placeholder="设备 ID"
                      />
                    </div>
                  </Show>
                  <Show when={mode() === 'multi'}>
                    <div class="animate-fade-in">
                      <TextArea
                        rows={3}
                        class="font-mono text-xs"
                        value={multiDeviceIds()}
                        onInput={(e) => setMultiDeviceIds(e.currentTarget.value)}
                        placeholder="多个 deviceId，用空格 / 逗号分隔"
                      />
                    </div>
                  </Show>
                  <Show when={mode() === 'allOnline'}>
                    <div class="flex items-start gap-2 rounded-lg border border-warning/30 bg-warning-light px-3 py-2 text-sm text-warning animate-fade-in">
                      <svg class="w-4 h-4 mt-0.5 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L3.034 16.5c-.77.833.192 2.5 1.732 2.5z" />
                      </svg>
                      <span>将下发到当前所有在线设备</span>
                    </div>
                  </Show>
                </div>
              </div>

              {/* script + 模板 */}
              <div class="space-y-2">
                <div class="flex items-center justify-between gap-2">
                  <label for="probe-template-select" class="text-sm font-medium text-content-secondary">script</label>
                  <div class="relative">
                    <select
                      id="probe-template-select"
                      aria-label="选择脚本模板"
                      value={templateKey()}
                      class="h-8 pl-2.5 pr-7 text-xs rounded-md border border-border-hairline bg-surface text-content
                             transition-[border-color,box-shadow] duration-fast ease-out-expo
                             hover:border-border cursor-pointer
                             focus-ring-soft focus:border-accent
                             appearance-none"
                      onChange={(e) => {
                        const tpl = PROBE_TEMPLATES.find((t) => t.name === e.currentTarget.value);
                        if (tpl) setScript(tpl.body);
                        setTemplateKey('');
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
                    {/* 自渲染下拉箭头，跟随 currentColor，避免 dark mode 黑色箭头 */}
                    <svg
                      aria-hidden="true"
                      class="absolute right-2 top-1/2 -translate-y-1/2 pointer-events-none w-3 h-3 text-content-secondary"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      stroke-width="2"
                    >
                      <path stroke-linecap="round" stroke-linejoin="round" d="m6 9 6 6 6-6" />
                    </svg>
                  </div>
                </div>
                <div class="rounded-lg border border-border-hairline overflow-hidden focus-within:ring-2 focus-within:ring-accent/30 focus-within:border-accent transition-[border-color,box-shadow] duration-fast">
                  <ScriptEditor value={script()} onChange={setScript} minHeightPx={200} />
                </div>
              </div>

              <div class="grid gap-3 sm:grid-cols-2">
                <Input
                  label="timeout (ms)"
                  type="number"
                  min={100}
                  max={10000}
                  class="font-mono"
                  value={timeoutMsInput()}
                  onInput={(e) => setTimeoutMsInput(e.currentTarget.value)}
                />
                <Input
                  label="note（可选）"
                  value={note()}
                  onInput={(e) => setNote(e.currentTarget.value)}
                  placeholder="排查 X 用户的 OOM"
                />
              </div>

              <div class="flex items-center gap-3 flex-wrap">
                <Button variant="primary" disabled={sending()} loading={sending()} onClick={handleSend}>
                  {sending() ? '发送中' : '发送'}
                </Button>
                <Show when={errMsg()}>
                  <div class="flex items-center gap-1.5 rounded-md border border-error/30 bg-error-light px-2.5 py-1 text-xs text-error animate-fade-in">
                    <svg class="w-3.5 h-3.5 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                      <path stroke-linecap="round" stroke-linejoin="round" d="M12 9v2m0 4h.01M5.07 19h13.86c1.54 0 2.5-1.67 1.73-3L13.73 4a2 2 0 0 0-3.46 0L3.34 16c-.77 1.33.19 3 1.73 3z" />
                    </svg>
                    <span>{errMsg()}</span>
                  </div>
                </Show>
              </div>
            </div>
          </Card>

          <Card variant="elevated" padding="md">
            <div class="space-y-3">
              <div class="flex items-center justify-between gap-2">
                <h2 class="text-headline text-content">结果</h2>
                <div class="flex items-center gap-2">
                  <Show when={currentBatch()}>
                    <span class="font-mono text-xs text-content-tertiary">
                      batch={currentBatch()!.slice(0, 8)}…
                      <Show when={completed()}>
                        {' '}· {completed()!.received}/{completed()!.expected}
                      </Show>
                    </span>
                  </Show>
                  <Show when={results().length > 0}>
                    <Button variant="ghost" size="xs" onClick={exportBatchJson}>
                      导出 JSON
                    </Button>
                    <Button variant="ghost" size="xs" onClick={() => setResults([])}>
                      清空
                    </Button>
                  </Show>
                </div>
              </div>
              <Show
                when={results().length > 0}
                fallback={<Empty title="尚无结果" description="点击发送后，实时结果会在这里以卡片网格展示" />}
              >
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
            </div>
          </Card>
        </div>

        <Card variant="elevated" padding="md" class="lg:sticky lg:top-4 lg:self-start lg:max-h-[calc(100vh-2rem)] lg:overflow-y-auto">
          <div class="space-y-3">
            <h3 class="text-sm font-semibold text-content">最近 batch</h3>
            <Show
              when={recentBatches().length > 0}
              fallback={<p class="text-xs text-content-tertiary py-2">无历史</p>}
            >
              <ul class="space-y-2">
                <For each={recentBatches()}>
                  {(row) => (
                    <li class="rounded-lg border border-border-hairline p-2.5 space-y-1.5 transition-[border-color,background-color] duration-fast hover:border-border hover:bg-surface-secondary">
                      <div class="flex items-center justify-between gap-2 text-xs">
                        <span class="font-mono text-content truncate">{row.batchId.slice(0, 8)}…</span>
                        <span class="text-content-tertiary tabular-nums shrink-0">{formatTime(row.dispatchedAt)}</span>
                      </div>
                      <Show when={row.note}>
                        <p class="text-xs text-content-secondary truncate" title={row.note ?? ''}>
                          {row.note}
                        </p>
                      </Show>
                      <Button variant="ghost" size="xs" class="w-full" onClick={() => replayBatch(row)}>
                        回放此 script
                      </Button>
                    </li>
                  )}
                </For>
              </ul>
            </Show>
          </div>
        </Card>
      </div>

      <Show when={confirmTarget()}>
        <ConfirmDialog
          open={confirmTarget() !== null}
          requestId={confirmTarget()!.requestId}
          deviceId={confirmTarget()!.deviceId}
          actionsPreview={extractActionsPreview(confirmTarget()!.resultJson)}
          onClose={() => setConfirmTarget(null)}
          onConfirmed={() => {
            // ConfirmDialog 内部已调用 probeApi.confirm(requestId, { deviceIdSuffix });
            // 这里只负责本地结果剔除 + 历史刷新（SSE 会推后续 ok/err 事件）
            const reqId = confirmTarget()!.requestId;
            setResults((prev) => prev.filter((r) => r.requestId !== reqId));
            void loadRecent();
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
    const hh = String(d.getHours()).padStart(2, '0');
    const mm = String(d.getMinutes()).padStart(2, '0');
    const hhmm = `${hh}:${mm}`;
    const now = new Date();
    const isSameDay = (a: Date, b: Date) =>
      a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();
    if (isSameDay(d, now)) return hhmm;
    const yesterday = new Date(now);
    yesterday.setDate(now.getDate() - 1);
    if (isSameDay(d, yesterday)) return `昨天 ${hhmm}`;
    const mo = String(d.getMonth() + 1).padStart(2, '0');
    const dd = String(d.getDate()).padStart(2, '0');
    return `${mo}-${dd} ${hhmm}`;
  } catch {
    return iso;
  }
}
