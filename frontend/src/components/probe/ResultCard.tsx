/**
 * 单设备探针结果卡片：状态 pill + 时长 + JSON pretty + 错误展示 +
 * 复制 JSON 按钮 + （当 status=confirm_required 时）确认执行按钮。
 */

import { Show, createMemo } from 'solid-js';

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

export default function ResultCard(props: ResultCardProps) {
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

  const copyJson = async () => {
    if (!jsonText()) return;
    try {
      await navigator.clipboard.writeText(jsonText());
    } catch {
      /* 浏览器拒绝 → ignore */
    }
  };

  return (
    <article class="rounded border border-border-hairline bg-surface-secondary p-3 space-y-2">
      <header class="flex items-center justify-between gap-2 text-xs">
        <span class="font-mono truncate" title={props.deviceId}>
          {props.deviceId}
        </span>
        <span class={`rounded px-1.5 py-0.5 ${pillClass()}`}>
          {props.status}
          {props.truncated ? ' · truncated' : ''}
          {props.durationMs !== undefined ? ` · ${props.durationMs}ms` : ''}
        </span>
      </header>
      <Show when={jsonText()}>
        <pre class="max-h-64 overflow-auto rounded bg-black/5 p-2 text-[11px] leading-snug">
          {jsonText()}
        </pre>
      </Show>
      <Show when={props.stderr}>
        <pre class="max-h-32 overflow-auto rounded bg-status-danger-light p-2 text-[11px] leading-snug text-status-danger">
          {props.stderr}
        </pre>
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
