/**
 * 单设备探针结果卡片：状态 pill + 时长 + JSON pretty + 错误展示 +
 * 复制 JSON 按钮 + （当 status=confirm_required 时）确认执行按钮。
 */

import { Show, createMemo, createSignal } from 'solid-js';
import { uiStore } from '@/stores/ui';

export interface ResultCardProps {
  deviceId: string;
  requestId: string;
  status: string;
  durationMs?: number;
  truncated?: boolean;
  resultJson?: unknown;
  stderr?: string;
  onConfirmClick?: () => void;
}

// stderr 默认仅显示折叠预览（前 N 字符），展开后才整体渲染避免长 trace 撑爆卡片
const STDERR_PREVIEW_LIMIT = 320;

export default function ResultCard(props: ResultCardProps) {
  const [stderrExpanded, setStderrExpanded] = createSignal(false);

  const pillClass = createMemo(() => {
    switch (props.status) {
      case 'ok':
        return 'bg-status-success-light text-status-success';
      case 'timeout':
      case 'expired':
      case 'confirm_required':
        return 'bg-status-warning-light text-status-warning';
      case 'error':
      case 'unsupported_ctx_version':
      case 'offline':
        return 'bg-status-danger-light text-status-danger';
      default:
        return 'bg-surface-secondary text-content-secondary';
    }
  });

  const jsonText = createMemo(() => {
    if (props.resultJson === undefined || props.resultJson === null) return '';
    try {
      return JSON.stringify(props.resultJson, null, 2);
    } catch {
      return String(props.resultJson);
    }
  });

  const stderrLong = createMemo(() => (props.stderr ?? '').length > STDERR_PREVIEW_LIMIT);
  const stderrDisplay = createMemo(() => {
    const s = props.stderr ?? '';
    if (stderrExpanded() || !stderrLong()) return s;
    return s.slice(0, STDERR_PREVIEW_LIMIT) + '…';
  });

  const copyJson = async () => {
    if (!jsonText()) return;
    try {
      await navigator.clipboard.writeText(jsonText());
      uiStore.toast.success('已复制');
    } catch {
      uiStore.toast.error('复制失败');
    }
  };

  return (
    <article class="rounded border border-border-hairline bg-surface-secondary p-3 space-y-2">
      <header class="flex items-center justify-between gap-2 text-xs">
        <span class="font-mono truncate" title={props.deviceId}>
          {props.deviceId}
        </span>
        <span class={`flex items-center gap-1 whitespace-nowrap rounded px-1.5 py-0.5 ${pillClass()}`}>
          <span>{props.status}</span>
          <Show when={props.truncated}>
            <span aria-hidden="true">·</span>
            <span>truncated</span>
          </Show>
          <Show when={props.durationMs !== undefined}>
            <span aria-hidden="true">·</span>
            <span>{props.durationMs}ms</span>
          </Show>
        </span>
      </header>
      <Show when={jsonText()}>
        <pre class="max-h-64 overflow-auto rounded bg-black/5 p-2 text-[11px] leading-snug">
          {jsonText()}
        </pre>
      </Show>
      <Show when={props.stderr}>
        <div class="rounded bg-status-danger/10 p-2 text-status-danger">
          <pre class="max-h-32 overflow-auto text-[11px] leading-snug">
            {stderrDisplay()}
          </pre>
          <Show when={stderrLong()}>
            <button
              type="button"
              class="mt-1 text-[11px] underline hover:no-underline"
              onClick={() => setStderrExpanded((v) => !v)}
              aria-expanded={stderrExpanded()}
            >
              {stderrExpanded() ? '收起' : '展开完整 stderr'}
            </button>
          </Show>
        </div>
      </Show>
      <div class="flex items-center gap-2">
        <Show when={jsonText()}>
          <button
            type="button"
            class="rounded border border-border-hairline px-2 py-0.5 text-xs hover:bg-surface"
            onClick={copyJson}
          >
            复制 JSON
          </button>
        </Show>
        <Show when={props.status === 'confirm_required' && props.onConfirmClick}>
          <button
            type="button"
            class="rounded bg-status-warning px-2 py-0.5 text-xs font-medium text-white"
            onClick={() => props.onConfirmClick?.()}
          >
            确认执行
          </button>
        </Show>
      </div>
    </article>
  );
}
