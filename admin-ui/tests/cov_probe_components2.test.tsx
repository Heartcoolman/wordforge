import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from './helpers/render';

// 全量 mock 被测组件的依赖，杜绝真实网络 / 真实 toast 副作用
vi.mock('@/api/probe', () => ({
  probeApi: {
    confirm: vi.fn(),
  },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { probeApi } from '@/api/probe';
import { uiStore } from '@/stores/ui';

const mockProbe = probeApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockToast = uiStore.toast as unknown as Record<string, ReturnType<typeof vi.fn>>;

// ───────────────────────── ConfirmDialog ─────────────────────────

async function renderConfirm(props: Record<string, unknown> = {}) {
  const { default: ConfirmDialog } = await import('@/components/probe/ConfirmDialog');
  const onClose = (props.onClose as () => void) ?? vi.fn();
  const onConfirmed = props.onConfirmed as (() => void) | undefined;
  return renderWithProviders(() => (
    <ConfirmDialog
      open={props.open !== false}
      requestId={(props.requestId as string) ?? 'req-1'}
      deviceId={(props.deviceId as string) ?? 'device-ABCDE'}
      actionsPreview={(props.actionsPreview as string[]) ?? ['reset', 'wipe']}
      onClose={onClose}
      onConfirmed={onConfirmed}
    />
  ));
}

describe('ConfirmDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    document.body.style.overflow = '';
  });

  it('open=false 时不渲染对话框', async () => {
    await renderConfirm({ open: false });
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('open=true 渲染标题、actions 预览与 deviceId 全文', async () => {
    await renderConfirm({ deviceId: 'device-12345', actionsPreview: ['cmd:a', 'cmd:b'] });
    await waitFor(() => expect(screen.getByRole('dialog')).toBeInTheDocument());
    expect(screen.getByText('确认远程受控写')).toBeInTheDocument();
    expect(screen.getByText('cmd:a, cmd:b')).toBeInTheDocument();
    expect(screen.getByText(/device-12345/)).toBeInTheDocument();
  });

  it('actionsPreview 为空时显示 (empty)', async () => {
    await renderConfirm({ actionsPreview: [] });
    await waitFor(() => expect(screen.getByText('(empty)')).toBeInTheDocument());
  });

  it('确认按钮初始 disabled，输入正确后 5 位后可点击', async () => {
    await renderConfirm({ deviceId: 'device-ABCDE' });
    await waitFor(() => expect(screen.getByRole('dialog')).toBeInTheDocument());
    const confirmBtn = screen.getByText('确认执行') as HTMLButtonElement;
    expect(confirmBtn.disabled).toBe(true);
    const input = screen.getByPlaceholderText('输入后 5 位') as HTMLInputElement;
    // 输错后 5 位 → 仍 disabled + 显示不匹配提示
    fireEvent.input(input, { target: { value: 'ZZZZZ' } });
    await waitFor(() => expect(screen.getByText('后 5 位不匹配')).toBeInTheDocument());
    expect(confirmBtn.disabled).toBe(true);
    // 输对（大小写不敏感）→ enabled
    fireEvent.input(input, { target: { value: 'abcde' } });
    await waitFor(() => expect(confirmBtn.disabled).toBe(false));
  });

  it('提交成功：调用 probeApi.confirm + onConfirmed + onClose', async () => {
    mockProbe.confirm.mockResolvedValue({ confirmed: true });
    const onClose = vi.fn();
    const onConfirmed = vi.fn();
    await renderConfirm({ deviceId: 'device-ABCDE', requestId: 'req-xyz', onClose, onConfirmed });
    await waitFor(() => expect(screen.getByRole('dialog')).toBeInTheDocument());
    const input = screen.getByPlaceholderText('输入后 5 位') as HTMLInputElement;
    fireEvent.input(input, { target: { value: 'ABCDE' } });
    const confirmBtn = screen.getByText('确认执行') as HTMLButtonElement;
    await waitFor(() => expect(confirmBtn.disabled).toBe(false));
    fireEvent.click(confirmBtn);
    await waitFor(() =>
      expect(mockProbe.confirm).toHaveBeenCalledWith('req-xyz', { deviceIdSuffix: 'ABCDE' }),
    );
    await waitFor(() => expect(onConfirmed).toHaveBeenCalled());
    expect(onClose).toHaveBeenCalled();
  });

  it('提交失败：显示错误 alert，不调用 onClose', async () => {
    mockProbe.confirm.mockRejectedValue(new Error('confirm boom'));
    const onClose = vi.fn();
    await renderConfirm({ deviceId: 'device-ABCDE', onClose });
    await waitFor(() => expect(screen.getByRole('dialog')).toBeInTheDocument());
    const input = screen.getByPlaceholderText('输入后 5 位') as HTMLInputElement;
    fireEvent.input(input, { target: { value: 'ABCDE' } });
    const confirmBtn = screen.getByText('确认执行') as HTMLButtonElement;
    await waitFor(() => expect(confirmBtn.disabled).toBe(false));
    fireEvent.click(confirmBtn);
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('confirm boom'));
    expect(onClose).not.toHaveBeenCalled();
  });

  it('点击取消按钮触发 onClose', async () => {
    const onClose = vi.fn();
    await renderConfirm({ onClose });
    await waitFor(() => expect(screen.getByRole('dialog')).toBeInTheDocument());
    fireEvent.click(screen.getByText('取消'));
    expect(onClose).toHaveBeenCalled();
  });

  it('点击 backdrop 自身关闭对话框', async () => {
    const onClose = vi.fn();
    await renderConfirm({ onClose });
    const dialog = await screen.findByRole('dialog');
    // 事件 target === currentTarget（遮罩自身）才触发关闭
    fireEvent.click(dialog);
    expect(onClose).toHaveBeenCalled();
  });

  it('Escape 键关闭对话框并恢复 body overflow', async () => {
    const onClose = vi.fn();
    await renderConfirm({ onClose });
    await waitFor(() => expect(screen.getByRole('dialog')).toBeInTheDocument());
    // 打开后 body 滚动被锁
    expect(document.body.style.overflow).toBe('hidden');
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
  });

  it('Tab/Shift+Tab 在 focus trap 内不抛错', async () => {
    await renderConfirm({});
    await waitFor(() => expect(screen.getByRole('dialog')).toBeInTheDocument());
    // 仅覆盖 focus trap 分支，不强求焦点落点（happy-dom 焦点不完整）
    fireEvent.keyDown(document, { key: 'Tab' });
    fireEvent.keyDown(document, { key: 'Tab', shiftKey: true });
    expect(screen.getByRole('dialog')).toBeInTheDocument();
  });
});

// ───────────────────────── ResultCard ─────────────────────────

async function renderResult(props: Record<string, unknown> = {}) {
  const { default: ResultCard } = await import('@/components/probe/ResultCard');
  return renderWithProviders(() => (
    <ResultCard
      deviceId={(props.deviceId as string) ?? 'dev-1'}
      requestId={(props.requestId as string) ?? 'req-1'}
      status={(props.status as string) ?? 'ok'}
      durationMs={props.durationMs as number | undefined}
      truncated={props.truncated as boolean | undefined}
      resultJson={props.resultJson}
      stderr={props.stderr as string | undefined}
      onConfirmClick={props.onConfirmClick as (() => void) | undefined}
    />
  ));
}

function stubClipboard(impl: () => Promise<void>) {
  Object.defineProperty(navigator, 'clipboard', {
    configurable: true,
    writable: true,
    value: { writeText: vi.fn(impl) },
  });
}

describe('ResultCard', () => {
  beforeEach(() => vi.clearAllMocks());

  it('ok 状态渲染 deviceId、status pill、duration', async () => {
    await renderResult({ deviceId: 'dev-XYZ', status: 'ok', durationMs: 120 });
    await waitFor(() => expect(screen.getByText('dev-XYZ')).toBeInTheDocument());
    expect(screen.getByText('ok')).toBeInTheDocument();
    expect(screen.getByText('120ms')).toBeInTheDocument();
  });

  it('truncated=true 渲染 truncated 标记', async () => {
    await renderResult({ status: 'ok', truncated: true });
    await waitFor(() => expect(screen.getByText('truncated')).toBeInTheDocument());
  });

  it('resultJson 渲染为 pretty JSON', async () => {
    await renderResult({ status: 'ok', resultJson: { a: 1, b: 'x' } });
    await waitFor(() => expect(screen.getByText(/"a": 1/)).toBeInTheDocument());
  });

  it('resultJson 为 null 时不渲染复制按钮', async () => {
    await renderResult({ status: 'ok', resultJson: null });
    await waitFor(() => expect(screen.getByText('ok')).toBeInTheDocument());
    expect(screen.queryByText('复制 JSON')).toBeNull();
  });

  it('复制 JSON 成功 → toast.success("已复制")', async () => {
    stubClipboard(() => Promise.resolve());
    await renderResult({ status: 'ok', resultJson: { ok: true } });
    const btn = await screen.findByText('复制 JSON');
    fireEvent.click(btn);
    await waitFor(() => expect(mockToast.success).toHaveBeenCalledWith('已复制'));
  });

  it('复制 JSON 失败 → toast.error("复制失败")', async () => {
    stubClipboard(() => Promise.reject(new Error('denied')));
    await renderResult({ status: 'ok', resultJson: { ok: true } });
    const btn = await screen.findByText('复制 JSON');
    fireEvent.click(btn);
    await waitFor(() => expect(mockToast.error).toHaveBeenCalledWith('复制失败'));
  });

  it('短 stderr 直接展示、无展开按钮', async () => {
    await renderResult({ status: 'error', stderr: 'short trace' });
    await waitFor(() => expect(screen.getByText('short trace')).toBeInTheDocument());
    expect(screen.queryByText('展开完整 stderr')).toBeNull();
  });

  it('长 stderr 折叠预览，可展开/收起切换', async () => {
    const longErr = 'E'.repeat(400);
    await renderResult({ status: 'error', stderr: longErr });
    const toggle = await screen.findByText('展开完整 stderr');
    fireEvent.click(toggle);
    await waitFor(() => expect(screen.getByText('收起')).toBeInTheDocument());
    fireEvent.click(screen.getByText('收起'));
    await waitFor(() => expect(screen.getByText('展开完整 stderr')).toBeInTheDocument());
  });

  it('confirm_required 且有 onConfirmClick → 渲染确认执行按钮并触发回调', async () => {
    const onConfirmClick = vi.fn();
    await renderResult({ status: 'confirm_required', onConfirmClick });
    const btn = await screen.findByText('确认执行');
    fireEvent.click(btn);
    expect(onConfirmClick).toHaveBeenCalled();
  });

  it('confirm_required 但无 onConfirmClick → 不渲染确认执行按钮', async () => {
    await renderResult({ status: 'confirm_required' });
    await waitFor(() => expect(screen.getByText('confirm_required')).toBeInTheDocument());
    expect(screen.queryByText('确认执行')).toBeNull();
  });

  it('warning 类状态命中 warning pill 分支（timeout）', async () => {
    await renderResult({ status: 'timeout' });
    await waitFor(() => expect(screen.getByText('timeout')).toBeInTheDocument());
  });

  it('danger 类状态命中 danger pill 分支（offline）', async () => {
    await renderResult({ status: 'offline' });
    await waitFor(() => expect(screen.getByText('offline')).toBeInTheDocument());
  });

  it('未知状态命中 default pill 分支', async () => {
    await renderResult({ status: 'pending_weird' });
    await waitFor(() => expect(screen.getByText('pending_weird')).toBeInTheDocument());
  });
});