/**
 * D 类受控写二次确认 modal：
 *  - 显示 device_id 全文 + 即将执行的 actions 列表
 *  - 要求 admin 输入该 device 最后 5 位（实时校验，不区分大小写）
 *  - 提交 → probeApi.confirm(requestId, { deviceIdSuffix })
 */

import { createSignal, Show } from 'solid-js';
import { probeApi } from '@/api/probe';

interface Props {
  open: boolean;
  requestId: string;
  deviceId: string;
  actionsPreview: string[];
  onClose: () => void;
  onConfirmed?: () => void;
}

export default function ConfirmDialog(props: Props) {
  const [suffix, setSuffix] = createSignal('');
  const [submitting, setSubmitting] = createSignal(false);
  const [err, setErr] = createSignal<string | null>(null);

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

  return (
    <Show when={props.open}>
      <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm">
        <div class="w-[440px] max-w-full rounded-lg bg-surface p-5 shadow-xl space-y-4">
          <h3 class="text-lg font-semibold">确认远程受控写</h3>
          <p class="text-sm text-content-secondary">
            即将对目标设备执行：
            <span class="font-mono ml-1 text-status-warning">
              {props.actionsPreview.join(', ') || '(empty)'}
            </span>
          </p>
          <div class="rounded bg-surface-secondary p-3 text-xs font-mono break-all">
            deviceId = {props.deviceId}
          </div>
          <label class="flex flex-col gap-1 text-sm">
            <span class="text-content-secondary">输入该 device 最后 5 位以确认：</span>
            <input
              type="text"
              maxLength={5}
              autofocus
              class="rounded border border-border-hairline bg-surface-secondary px-2 py-1.5 font-mono tracking-widest"
              value={suffix()}
              onInput={(e) => setSuffix(e.currentTarget.value)}
              placeholder={expectedSuffix().replace(/./g, '·')}
            />
            <Show when={suffix().length === 5 && !matches()}>
              <span class="text-xs text-status-danger">后 5 位不匹配</span>
            </Show>
          </label>
          <Show when={err()}>
            <p class="text-sm text-status-danger">{err()}</p>
          </Show>
          <div class="flex justify-end gap-2">
            <button
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
