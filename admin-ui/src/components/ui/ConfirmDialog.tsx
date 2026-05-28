import { type JSX, Show } from 'solid-js';
import { Modal } from './Modal';
import { Button } from './Button';

type ConfirmVariant = 'default' | 'danger' | 'success' | 'warning';

const variantToButton: Record<ConfirmVariant, 'primary' | 'danger' | 'success' | 'warning'> = {
  default: 'primary',
  danger: 'danger',
  success: 'success',
  warning: 'warning',
};

interface ConfirmDialogProps {
  open: boolean;
  title: string;
  message?: JSX.Element;
  /** 可选附加内容，渲染在 message 下、按钮上（如输入框等） */
  children?: JSX.Element;
  confirmText?: string;
  cancelText?: string;
  variant?: ConfirmVariant;
  onConfirm: () => void;
  onCancel: () => void;
  loading?: boolean;
}

export function ConfirmDialog(props: ConfirmDialogProps) {
  // 危险或加载中的弹窗禁用遮罩点击关闭，避免误触；其他场景仍允许
  const closeOnBackdrop = () => !(props.loading || props.variant === 'danger');

  return (
    <Modal
      open={props.open}
      onClose={() => { if (!props.loading) props.onCancel(); }}
      title={props.title}
      size="sm"
      hideClose
      closeOnBackdrop={closeOnBackdrop()}
    >
      <Show when={props.message}>
        <div class="text-sm text-content-secondary mb-4">{props.message}</div>
      </Show>
      <Show when={props.children}>
        <div class="mb-3">{props.children}</div>
      </Show>
      <div class="flex justify-end gap-2">
        <Button
          size="sm"
          variant="ghost"
          disabled={props.loading}
          onClick={props.onCancel}
        >
          {props.cancelText ?? '取消'}
        </Button>
        <Button
          size="sm"
          variant={variantToButton[props.variant ?? 'default']}
          loading={props.loading}
          onClick={props.onConfirm}
        >
          {props.confirmText ?? '确认'}
        </Button>
      </div>
    </Modal>
  );
}
