/**
 * D 类受控写二次确认 modal：
 *  - 显示 device_id 全文 + 即将执行的 actions 列表
 *  - 要求 admin 输入该 device 最后 5 位（实时校验，不区分大小写）
 *  - 提交 → probeApi.confirm(requestId, { deviceIdSuffix })
 *
 * a11y 补齐：role=dialog + aria-modal、Escape 关闭、body scroll lock、
 * 初始焦点落在"取消"（危险确认更安全）、Tab focus trap、backdrop 点击关闭。
 */

import { createSignal, Show, onCleanup, createEffect, on } from 'solid-js';
import { probeApi } from '@/api/probe';

interface Props {
  open: boolean;
  requestId: string;
  deviceId: string;
  actionsPreview: string[];
  onClose: () => void;
  onConfirmed?: () => void;
}

const FOCUSABLE_SELECTOR =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

export default function ConfirmDialog(props: Props) {
  const [suffix, setSuffix] = createSignal('');
  const [submitting, setSubmitting] = createSignal(false);
  const [err, setErr] = createSignal<string | null>(null);

  let dialogRef: HTMLDivElement | undefined;
  let cancelBtnRef: HTMLButtonElement | undefined;
  let previouslyFocused: HTMLElement | null = null;
  let savedBodyOverflow: string | null = null;

  const expectedSuffix = () => props.deviceId.slice(-5);
  const matches = () =>
    suffix().length === expectedSuffix().length
    && suffix().toLowerCase() === expectedSuffix().toLowerCase();

  const handleSubmit = async () => {
    setErr(null);
    setSubmitting(true);
    try {
      await probeApi.confirm(props.requestId, { deviceIdSuffix: suffix() });
      setSubmitting(false);
      props.onConfirmed?.();
      props.onClose();
    } catch (e: any) {
      setErr(e?.message ?? String(e));
      setSubmitting(false);
    }
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      props.onClose();
      return;
    }
    if (e.key === 'Tab' && dialogRef) {
      const focusable = Array.from(dialogRef.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR));
      if (focusable.length === 0) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (e.shiftKey) {
        if (document.activeElement === first) {
          e.preventDefault();
          last.focus();
        }
      } else if (document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    }
  };

  // open 切换时：锁 body 滚动、聚焦取消按钮、绑定 keydown
  createEffect(on(() => props.open, (isOpen, prev) => {
    if (isOpen === prev) return;
    if (isOpen) {
      setSuffix('');
      setErr(null);
      previouslyFocused = document.activeElement as HTMLElement | null;
      savedBodyOverflow = document.body.style.overflow;
      document.body.style.overflow = 'hidden';
      document.addEventListener('keydown', handleKeyDown);
      requestAnimationFrame(() => cancelBtnRef?.focus());
    } else {
      document.removeEventListener('keydown', handleKeyDown);
      if (savedBodyOverflow !== null) {
        document.body.style.overflow = savedBodyOverflow;
        savedBodyOverflow = null;
      }
      previouslyFocused?.focus();
      previouslyFocused = null;
    }
  }));

  onCleanup(() => {
    document.removeEventListener('keydown', handleKeyDown);
    if (savedBodyOverflow !== null) {
      document.body.style.overflow = savedBodyOverflow;
      savedBodyOverflow = null;
    }
  });

  const showSuffixError = () => suffix().length === 5 && !matches();

  return (
    <Show when={props.open}>
      <div
        class="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm"
        role="dialog"
        aria-modal="true"
        aria-labelledby="confirm-title"
        ref={dialogRef}
        onClick={(e) => {
          // backdrop 点击关闭：仅当事件目标是遮罩自身时才触发
          if (e.target === e.currentTarget && !submitting()) props.onClose();
        }}
      >
        <div class="w-[440px] max-w-full rounded-lg bg-surface p-5 shadow-xl space-y-4">
          <h3 id="confirm-title" class="text-lg font-semibold">确认远程受控写</h3>
          <p class="text-sm text-content-secondary">
            即将对目标设备执行：
            <span class="font-mono ml-1 text-status-warning">
              {props.actionsPreview.join(', ') || '(empty)'}
            </span>
          </p>
          <div class="rounded bg-surface-secondary p-3 text-xs font-mono break-all">
            deviceId = {props.deviceId}
          </div>
          <label class="flex flex-col gap-1 text-sm" for="confirm-suffix">
            <span class="text-content-secondary">输入该 device 最后 5 位以确认：</span>
            <input
              id="confirm-suffix"
              type="text"
              maxLength={5}
              class="rounded border border-border-hairline bg-surface-secondary px-2 py-1.5 font-mono tracking-widest"
              value={suffix()}
              onInput={(e) => setSuffix(e.currentTarget.value)}
              placeholder="输入后 5 位"
              aria-invalid={showSuffixError() ? 'true' : 'false'}
              aria-describedby={showSuffixError() ? 'suffix-error' : undefined}
            />
            <Show when={showSuffixError()}>
              <span id="suffix-error" class="text-xs text-status-danger">后 5 位不匹配</span>
            </Show>
          </label>
          <Show when={err()}>
            <p class="text-sm text-status-danger" role="alert">{err()}</p>
          </Show>
          <div class="flex justify-end gap-2">
            <button
              ref={cancelBtnRef}
              type="button"
              class="rounded px-3 py-1.5 text-sm hover:bg-surface-secondary"
              onClick={() => props.onClose()}
              disabled={submitting()}
            >
              取消
            </button>
            <button
              type="button"
              class="rounded bg-accent px-3 py-1.5 text-sm font-medium text-white disabled:opacity-50"
              disabled={!matches() || submitting()}
              onClick={handleSubmit}
            >
              {submitting() ? '提交中…' : '确认执行'}
            </button>
          </div>
        </div>
      </div>
    </Show>
  );
}
