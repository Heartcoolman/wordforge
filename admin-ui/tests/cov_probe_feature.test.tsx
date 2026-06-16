/**
 * 数据探针特性覆盖测试簇（probe_feature）
 *
 * 覆盖：
 *  - src/api/probe.ts        — probeApi.{dispatch,confirm,list,get} + connectProbeBatchStream（msw + mock fetch）
 *  - src/components/probe/ResultCard.tsx
 *  - src/components/probe/ConfirmDialog.tsx
 *  - src/pages/ProbePage.tsx — 渲染 / 模式切换 / 发送校验 / dispatch 分支 / 模板 / 回放 / 导出
 *  - src/workers/probe/api-bridge.ts — serializeAndTruncate / executeActions / handlers（mock http + 假 Worker）
 *
 * 备注：ProbePage 测试中将 ScriptEditor（CodeMirror）替换为简单 textarea，避免
 * happy-dom 下 CodeMirror 行为不确定；ScriptEditor 本身不在本簇内单测。
 */

import { describe, it, expect, vi, beforeEach, afterEach, beforeAll, afterAll } from 'vitest';
import { screen, waitFor, fireEvent, render } from '@solidjs/testing-library';
import { setupServer } from 'msw/node';
import { http, HttpResponse } from 'msw';
import { TEST_BASE_URL as BASE } from './helpers/constants';

const server = setupServer();
beforeAll(() => server.listen({ onUnhandledRequest: 'bypass' }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

// ─────────────────────────────────────────────────────────────────────────────
// 全局 mock：token / ui store
// ─────────────────────────────────────────────────────────────────────────────
vi.mock('@/lib/token', () => ({
  tokenManager: {
    getToken: () => 'user-token',
    getAdminToken: () => 'fake-admin-token',
    clearTokens: vi.fn(),
    clearAdminToken: vi.fn(),
    needsRefresh: () => false,
    refreshAccessToken: vi.fn(),
  },
}));
vi.mock('@/lib/device', () => ({
  getDeviceId: () => 'dev-self',
  getDevicePlatform: () => 'web',
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { uiStore } from '@/stores/ui';
const mockToast = uiStore.toast as unknown as Record<string, ReturnType<typeof vi.fn>>;

// ═════════════════════════════════════════════════════════════════════════════
// 1. probe.ts —— probeApi（msw）
// ═════════════════════════════════════════════════════════════════════════════
describe('probeApi (probe.ts)', () => {
  it('dispatch posts targets + script and returns batch payload', async () => {
    const { probeApi } = await import('@/api/probe');
    let body: Record<string, unknown> = {};
    server.use(
      http.post(`${BASE}/api/admin/probe`, async ({ request }) => {
        body = (await request.json()) as Record<string, unknown>;
        return HttpResponse.json({
          success: true,
          data: { batchId: 'b-1', dispatched: [{ deviceId: 'd-1', requestId: 'r-1' }], skippedOffline: [] },
        });
      }),
    );
    const res = await probeApi.dispatch({ targets: { deviceIds: ['d-1'] }, script: 'return 1;', timeoutMs: 3000 });
    expect(body).toMatchObject({ targets: { deviceIds: ['d-1'] }, script: 'return 1;', timeoutMs: 3000 });
    expect(res.batchId).toBe('b-1');
    expect(res.dispatched).toHaveLength(1);
  });

  it('confirm posts deviceIdSuffix and url-encodes requestId', async () => {
    const { probeApi } = await import('@/api/probe');
    let body: Record<string, unknown> = {};
    let hitUrl = '';
    server.use(
      http.post(`${BASE}/api/admin/probe/:rid/confirm`, async ({ request }) => {
        hitUrl = request.url;
        body = (await request.json()) as Record<string, unknown>;
        return HttpResponse.json({ success: true, data: { confirmed: true } });
      }),
    );
    const res = await probeApi.confirm('req/1', { deviceIdSuffix: '12345' });
    expect(body).toEqual({ deviceIdSuffix: '12345' });
    expect(hitUrl).toContain('req%2F1');
    expect(res.confirmed).toBe(true);
  });

  it('list forwards query params and returns rows', async () => {
    const { probeApi } = await import('@/api/probe');
    server.use(
      http.get(`${BASE}/api/admin/probe`, ({ request }) => {
        const url = new URL(request.url);
        expect(url.searchParams.get('limit')).toBe('10');
        return HttpResponse.json({ success: true, data: { rows: [], total: 0 } });
      }),
    );
    const res = await probeApi.list({ limit: 10 });
    expect(res).toEqual({ rows: [], total: 0 });
  });

  it('get fetches a single execution row by id', async () => {
    const { probeApi } = await import('@/api/probe');
    server.use(
      http.get(`${BASE}/api/admin/probe/by-id/:rid`, () =>
        HttpResponse.json({ success: true, data: { id: 'r-9', batchId: 'b-1' } })),
    );
    const res = await probeApi.get('r-9');
    expect(res).toMatchObject({ id: 'r-9' });
  });
});

// ═════════════════════════════════════════════════════════════════════════════
// 2. connectProbeBatchStream —— mock fetch 返回 SSE body
// ═════════════════════════════════════════════════════════════════════════════
function sseStreamResponse(chunks: string[], ok = true, status = 200): Response {
  const encoder = new TextEncoder();
  const body = ok
    ? new ReadableStream<Uint8Array>({
        start(controller) {
          for (const c of chunks) controller.enqueue(encoder.encode(c));
          controller.close();
        },
      })
    : null;
  return { ok, status, body } as unknown as Response;
}

describe('connectProbeBatchStream', () => {
  let originalFetch: typeof globalThis.fetch;
  beforeEach(() => {
    originalFetch = globalThis.fetch;
  });
  afterEach(() => {
    globalThis.fetch = originalFetch;
  });

  it('parses result + completed events and invokes callbacks', async () => {
    const { connectProbeBatchStream } = await import('@/api/probe');
    const resultEvt = JSON.stringify({ deviceId: 'd-1', requestId: 'r-1', batchId: 'b-1', status: 'ok' });
    const completedEvt = JSON.stringify({ received: 1, expected: 1 });
    const chunks = [
      `event: result\ndata: ${resultEvt}\n\n`,
      `event: completed\ndata: ${completedEvt}\n\n`,
    ];
    globalThis.fetch = vi.fn().mockResolvedValue(sseStreamResponse(chunks)) as unknown as typeof fetch;

    const onResult = vi.fn();
    const onCompleted = vi.fn();
    const onError = vi.fn();
    const stop = connectProbeBatchStream('b-1', { onResult, onCompleted, onError });

    await waitFor(() => expect(onCompleted).toHaveBeenCalled());
    expect(onResult).toHaveBeenCalledWith(expect.objectContaining({ requestId: 'r-1', status: 'ok' }));
    expect(onCompleted).toHaveBeenCalledWith({ received: 1, expected: 1 });
    expect(onError).not.toHaveBeenCalled();
    stop();
  });

  it('ignores malformed event data without throwing', async () => {
    const { connectProbeBatchStream } = await import('@/api/probe');
    const chunks = [
      'event: result\ndata: {not-json\n\n',
      `event: completed\ndata: ${JSON.stringify({ received: 0, expected: 0 })}\n\n`,
    ];
    globalThis.fetch = vi.fn().mockResolvedValue(sseStreamResponse(chunks)) as unknown as typeof fetch;
    const onResult = vi.fn();
    const onCompleted = vi.fn();
    connectProbeBatchStream('b-1', { onResult, onCompleted });
    await waitFor(() => expect(onCompleted).toHaveBeenCalled());
    expect(onResult).not.toHaveBeenCalled();
  });

  it('reports error on non-ok HTTP response', async () => {
    const { connectProbeBatchStream } = await import('@/api/probe');
    globalThis.fetch = vi.fn().mockResolvedValue(sseStreamResponse([], false, 500)) as unknown as typeof fetch;
    const onError = vi.fn();
    connectProbeBatchStream('b-err', { onError });
    await waitFor(() => expect(onError).toHaveBeenCalled());
    expect((onError.mock.calls[0][0] as Error).message).toContain('500');
  });

  it('reports error when fetch rejects', async () => {
    const { connectProbeBatchStream } = await import('@/api/probe');
    globalThis.fetch = vi.fn().mockRejectedValue(new Error('boom')) as unknown as typeof fetch;
    const onError = vi.fn();
    connectProbeBatchStream('b-boom', { onError });
    await waitFor(() => expect(onError).toHaveBeenCalledWith(expect.any(Error)));
  });

  it('stop() returns a function and aborts cleanly', async () => {
    const { connectProbeBatchStream } = await import('@/api/probe');
    globalThis.fetch = vi.fn().mockResolvedValue(sseStreamResponse([])) as unknown as typeof fetch;
    const stop = connectProbeBatchStream('b-x', {});
    expect(typeof stop).toBe('function');
    stop(); // abort before/after — 不应抛错
  });
});

// ═════════════════════════════════════════════════════════════════════════════
// 3. ResultCard 组件
// ═════════════════════════════════════════════════════════════════════════════
describe('ResultCard', () => {
  beforeEach(() => vi.clearAllMocks());

  async function renderCard(props: Record<string, unknown>) {
    const { default: ResultCard } = await import('@/components/probe/ResultCard');
    return render(() => <ResultCard {...(props as any)} />);
  }

  it('renders ok status pill, deviceId, duration and json', async () => {
    await renderCard({
      deviceId: 'dev-abc', requestId: 'r-1', status: 'ok',
      durationMs: 42, resultJson: { hello: 'world' },
    });
    expect(screen.getByText('dev-abc')).toBeInTheDocument();
    expect(screen.getByText('ok')).toBeInTheDocument();
    expect(screen.getByText('42ms')).toBeInTheDocument();
    expect(screen.getByText(/"hello": "world"/)).toBeInTheDocument();
  });

  it('shows truncated marker when truncated', async () => {
    await renderCard({ deviceId: 'd', requestId: 'r', status: 'ok', truncated: true });
    expect(screen.getByText('truncated')).toBeInTheDocument();
  });

  it('copies json to clipboard and toasts success', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
    await renderCard({ deviceId: 'd', requestId: 'r', status: 'ok', resultJson: { a: 1 } });
    fireEvent.click(screen.getByText('复制 JSON'));
    await waitFor(() => expect(writeText).toHaveBeenCalled());
    await waitFor(() => expect(mockToast.success).toHaveBeenCalledWith('已复制'));
  });

  it('toasts error when clipboard write fails', async () => {
    const writeText = vi.fn().mockRejectedValue(new Error('denied'));
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
    await renderCard({ deviceId: 'd', requestId: 'r', status: 'ok', resultJson: { a: 1 } });
    fireEvent.click(screen.getByText('复制 JSON'));
    await waitFor(() => expect(mockToast.error).toHaveBeenCalledWith('复制失败'));
  });

  it('does not render copy button without json', async () => {
    await renderCard({ deviceId: 'd', requestId: 'r', status: 'timeout' });
    expect(screen.queryByText('复制 JSON')).not.toBeInTheDocument();
  });

  it('renders stderr with expand/collapse toggle for long traces', async () => {
    const longErr = 'E'.repeat(400);
    await renderCard({ deviceId: 'd', requestId: 'r', status: 'error', stderr: longErr });
    expect(screen.getByText('展开完整 stderr')).toBeInTheDocument();
    fireEvent.click(screen.getByText('展开完整 stderr'));
    await waitFor(() => expect(screen.getByText('收起')).toBeInTheDocument());
    fireEvent.click(screen.getByText('收起'));
    await waitFor(() => expect(screen.getByText('展开完整 stderr')).toBeInTheDocument());
  });

  it('short stderr has no expand toggle', async () => {
    await renderCard({ deviceId: 'd', requestId: 'r', status: 'error', stderr: 'short err' });
    expect(screen.queryByText('展开完整 stderr')).not.toBeInTheDocument();
  });

  it('renders confirm button when confirm_required and calls onConfirmClick', async () => {
    const onConfirmClick = vi.fn();
    await renderCard({ deviceId: 'd', requestId: 'r', status: 'confirm_required', onConfirmClick });
    const btn = screen.getByText('确认执行');
    fireEvent.click(btn);
    expect(onConfirmClick).toHaveBeenCalled();
  });

  it('falls back to String() when resultJson is unserializable', async () => {
    const circular: Record<string, unknown> = {};
    circular.self = circular;
    await renderCard({ deviceId: 'd', requestId: 'r', status: 'ok', resultJson: circular });
    // JSON.stringify 抛错 → String(circular) → "[object Object]"
    expect(screen.getByText('[object Object]')).toBeInTheDocument();
  });
});

// ═════════════════════════════════════════════════════════════════════════════
// 4. ConfirmDialog 组件
// ═════════════════════════════════════════════════════════════════════════════
describe('ConfirmDialog', () => {
  beforeEach(() => vi.clearAllMocks());

  const confirmMock = vi.fn();
  async function renderDialog(props: Partial<Record<string, unknown>> = {}) {
    vi.doMock('@/api/probe', () => ({ probeApi: { confirm: confirmMock } }));
    vi.resetModules();
    const { default: ConfirmDialog } = await import('@/components/probe/ConfirmDialog');
    const onClose = vi.fn();
    const onConfirmed = vi.fn();
    const utils = render(() => (
      <ConfirmDialog
        open
        requestId="req-1"
        deviceId="dev-abc12345"
        actionsPreview={['reload', 'clearCache']}
        onClose={onClose}
        onConfirmed={onConfirmed}
        {...(props as any)}
      />
    ));
    return { ...utils, onClose, onConfirmed };
  }

  afterEach(() => {
    vi.doUnmock('@/api/probe');
    vi.resetModules();
  });

  it('renders deviceId and actions preview', async () => {
    await renderDialog();
    expect(screen.getByText(/dev-abc12345/)).toBeInTheDocument();
    expect(screen.getByText('reload, clearCache')).toBeInTheDocument();
    expect(screen.getByText('确认远程受控写')).toBeInTheDocument();
  });

  it('renders empty actions placeholder', async () => {
    await renderDialog({ actionsPreview: [] });
    expect(screen.getByText('(empty)')).toBeInTheDocument();
  });

  it('confirm button disabled until suffix matches', async () => {
    await renderDialog();
    const confirmBtn = screen.getByText('确认执行') as HTMLButtonElement;
    expect(confirmBtn.disabled).toBe(true);
    const input = screen.getByPlaceholderText('输入后 5 位') as HTMLInputElement;
    fireEvent.input(input, { target: { value: '99999' } });
    await waitFor(() => expect(screen.getByText('后 5 位不匹配')).toBeInTheDocument());
    expect(confirmBtn.disabled).toBe(true);
    // 正确后 5 位（大小写不敏感）
    fireEvent.input(input, { target: { value: 'C12345' } }); // maxLength=5 → 实际取 'C1234'? 用真实后5位
    fireEvent.input(input, { target: { value: '12345' } });
    await waitFor(() => expect(confirmBtn.disabled).toBe(false));
  });

  it('submits confirm and triggers onConfirmed + onClose on success', async () => {
    confirmMock.mockResolvedValue({ confirmed: true });
    const { onClose, onConfirmed } = await renderDialog();
    const input = screen.getByPlaceholderText('输入后 5 位') as HTMLInputElement;
    fireEvent.input(input, { target: { value: '12345' } });
    await waitFor(() => expect((screen.getByText('确认执行') as HTMLButtonElement).disabled).toBe(false));
    fireEvent.click(screen.getByText('确认执行'));
    await waitFor(() => expect(confirmMock).toHaveBeenCalledWith('req-1', { deviceIdSuffix: '12345' }));
    await waitFor(() => expect(onConfirmed).toHaveBeenCalled());
    await waitFor(() => expect(onClose).toHaveBeenCalled());
  });

  it('shows error message when confirm rejects', async () => {
    confirmMock.mockRejectedValue({ message: '后端拒绝' });
    await renderDialog();
    const input = screen.getByPlaceholderText('输入后 5 位') as HTMLInputElement;
    fireEvent.input(input, { target: { value: '12345' } });
    await waitFor(() => expect((screen.getByText('确认执行') as HTMLButtonElement).disabled).toBe(false));
    fireEvent.click(screen.getByText('确认执行'));
    await waitFor(() => expect(screen.getByText('后端拒绝')).toBeInTheDocument());
  });

  it('clicking 取消 invokes onClose', async () => {
    const { onClose } = await renderDialog();
    fireEvent.click(screen.getByText('取消'));
    expect(onClose).toHaveBeenCalled();
  });

  it('Escape key closes the dialog', async () => {
    const { onClose } = await renderDialog();
    fireEvent.keyDown(document, { key: 'Escape' });
    await waitFor(() => expect(onClose).toHaveBeenCalled());
  });

  it('does not render content when open=false', async () => {
    await renderDialog({ open: false });
    expect(screen.queryByText('确认远程受控写')).not.toBeInTheDocument();
  });
});

// ═════════════════════════════════════════════════════════════════════════════
// 5. ProbePage 页面（重设计后自包含：内置 ResultCard + wf <Modal> 二次确认，
//    脚本编辑器为纯 <textarea>，模板为 badge 按钮，下发按钮文案「下发探针」）
// ═════════════════════════════════════════════════════════════════════════════
describe('ProbePage', () => {
  const dispatch = vi.fn();
  const list = vi.fn();
  const confirm = vi.fn();
  const connectStream = vi.fn();
  const stopStream = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    list.mockResolvedValue({ rows: [], total: 0 });
    connectStream.mockReturnValue(stopStream);
  });

  afterEach(() => {
    vi.doUnmock('@/api/probe');
    vi.resetModules();
  });

  async function renderPage() {
    vi.doMock('@/api/probe', () => ({
      probeApi: { dispatch, list, confirm, get: vi.fn() },
      connectProbeBatchStream: connectStream,
    }));
    vi.resetModules();
    const { default: ProbePage } = await import('@/pages/ProbePage');
    return render(() => <ProbePage />);
  }

  // 脚本编辑器是「诊断脚本」Field 里唯一无 placeholder 的 textarea；
  // 多设备 textarea 带 placeholder，据此区分（任意 mode 下都可定位）。
  function scriptEditor(): HTMLTextAreaElement {
    const tas = Array.from(document.querySelectorAll('textarea.textarea')) as HTMLTextAreaElement[];
    const el = tas.find((t) => !t.placeholder);
    if (!el) throw new Error('script editor textarea not found');
    return el;
  }

  it('renders header and default sections', async () => {
    await renderPage();
    expect(await screen.findByText('远程探针')).toBeInTheDocument();
    expect(screen.getByText('下发')).toBeInTheDocument();
    expect(screen.getByText('结果')).toBeInTheDocument();
    expect(screen.getByText('尚无结果')).toBeInTheDocument();
    // 默认 single 模式 → 设备 ID 输入框 + 下发按钮
    expect(screen.getByPlaceholderText('设备 ID')).toBeInTheDocument();
    expect(screen.getByText('下发探针')).toBeInTheDocument();
  });

  it('shows "无历史" when loadRecent returns empty', async () => {
    await renderPage();
    await waitFor(() => expect(list).toHaveBeenCalled());
    expect(screen.getByText('无历史')).toBeInTheDocument();
  });

  it('switches target modes (multi / allOnline / back to single)', async () => {
    await renderPage();
    await screen.findByText('远程探针');
    fireEvent.click(screen.getByText('多设备'));
    await waitFor(() =>
      expect(screen.getByPlaceholderText('多个 deviceId，用空格 / 逗号分隔')).toBeInTheDocument());
    fireEvent.click(screen.getByText('全部在线'));
    await waitFor(() => expect(screen.getByText('将下发到当前所有在线设备')).toBeInTheDocument());
    fireEvent.click(screen.getByText('单设备'));
    await waitFor(() => expect(screen.getByPlaceholderText('设备 ID')).toBeInTheDocument());
  });

  it('validates empty deviceId before dispatch', async () => {
    await renderPage();
    await screen.findByText('远程探针');
    fireEvent.click(screen.getByText('下发探针'));
    await waitFor(() => expect(screen.getByText('请填写 deviceId')).toBeInTheDocument());
    expect(dispatch).not.toHaveBeenCalled();
  });

  it('validates empty script before dispatch', async () => {
    await renderPage();
    await screen.findByText('远程探针');
    fireEvent.input(screen.getByPlaceholderText('设备 ID'), { target: { value: 'd-1' } });
    fireEvent.input(scriptEditor(), { target: { value: '   ' } });
    fireEvent.click(screen.getByText('下发探针'));
    await waitFor(() => expect(screen.getByText('script 不能为空')).toBeInTheDocument());
    expect(dispatch).not.toHaveBeenCalled();
  });

  it('dispatches and opens SSE stream; result + completed render', async () => {
    dispatch.mockResolvedValue({
      batchId: 'batch-9999',
      dispatched: [{ deviceId: 'd-1', requestId: 'r-1' }],
      skippedOffline: [],
    });
    await renderPage();
    await screen.findByText('远程探针');
    fireEvent.input(screen.getByPlaceholderText('设备 ID'), { target: { value: 'd-1' } });
    fireEvent.click(screen.getByText('下发探针'));
    await waitFor(() => expect(dispatch).toHaveBeenCalled());
    await waitFor(() => expect(connectStream).toHaveBeenCalledWith('batch-9999', expect.any(Object)));

    // 驱动 SSE 回调
    const cbs = connectStream.mock.calls[0][1];
    cbs.onResult({ deviceId: 'd-1', requestId: 'r-1', batchId: 'batch-9999', status: 'ok', resultJson: { x: 1 } });
    await waitFor(() => expect(screen.getByText('ok')).toBeInTheDocument());
    cbs.onCompleted({ received: 1, expected: 1 });
    await waitFor(() => expect(screen.getByText(/1\/1/)).toBeInTheDocument());
    // 导出 + 清空 出现
    expect(screen.getByText('导出 JSON')).toBeInTheDocument();
    expect(screen.getByText('清空')).toBeInTheDocument();
  });

  it('shows all-offline error when dispatched is empty', async () => {
    dispatch.mockResolvedValue({ batchId: 'b-2', dispatched: [], skippedOffline: ['d-9'] });
    await renderPage();
    await screen.findByText('远程探针');
    fireEvent.input(screen.getByPlaceholderText('设备 ID'), { target: { value: 'd-9' } });
    fireEvent.click(screen.getByText('下发探针'));
    await waitFor(() => expect(screen.getByText(/目标设备全部离线/)).toBeInTheDocument());
    expect(connectStream).not.toHaveBeenCalled();
  });

  it('shows PROBE_DISABLED friendly message', async () => {
    dispatch.mockRejectedValue({ code: 'PROBE_DISABLED' });
    await renderPage();
    await screen.findByText('远程探针');
    fireEvent.input(screen.getByPlaceholderText('设备 ID'), { target: { value: 'd-1' } });
    fireEvent.click(screen.getByText('下发探针'));
    await waitFor(() => expect(screen.getByText(/远程探针未启用/)).toBeInTheDocument());
  });

  it('shows generic error message on dispatch rejection', async () => {
    dispatch.mockRejectedValue({ message: '内部错误 500' });
    await renderPage();
    await screen.findByText('远程探针');
    fireEvent.input(screen.getByPlaceholderText('设备 ID'), { target: { value: 'd-1' } });
    fireEvent.click(screen.getByText('下发探针'));
    await waitFor(() => expect(screen.getByText('内部错误 500')).toBeInTheDocument());
  });

  it('multi mode parses space/comma separated ids', async () => {
    dispatch.mockResolvedValue({ batchId: 'b-3', dispatched: [{ deviceId: 'a', requestId: 'r' }], skippedOffline: [] });
    await renderPage();
    await screen.findByText('远程探针');
    fireEvent.click(screen.getByText('多设备'));
    const ta = await screen.findByPlaceholderText('多个 deviceId，用空格 / 逗号分隔');
    fireEvent.input(ta, { target: { value: 'a, b  c' } });
    fireEvent.click(screen.getByText('下发探针'));
    await waitFor(() => expect(dispatch).toHaveBeenCalled());
    expect(dispatch.mock.calls[0][0].targets).toEqual({ deviceIds: ['a', 'b', 'c'] });
  });

  it('multi mode with no ids → validation error', async () => {
    await renderPage();
    await screen.findByText('远程探针');
    fireEvent.click(screen.getByText('多设备'));
    fireEvent.click(screen.getByText('下发探针'));
    await waitFor(() => expect(screen.getByText('请填写 deviceId')).toBeInTheDocument());
  });

  it('allOnline mode dispatches with allOnline target', async () => {
    dispatch.mockResolvedValue({ batchId: 'b-4', dispatched: [{ deviceId: 'x', requestId: 'r' }], skippedOffline: [] });
    await renderPage();
    await screen.findByText('远程探针');
    fireEvent.click(screen.getByText('全部在线'));
    fireEvent.click(screen.getByText('下发探针'));
    await waitFor(() => expect(dispatch).toHaveBeenCalled());
    expect(dispatch.mock.calls[0][0].targets).toEqual({ allOnline: true });
  });

  it('clamps timeout below 100 to 100', async () => {
    dispatch.mockResolvedValue({ batchId: 'b-5', dispatched: [{ deviceId: 'x', requestId: 'r' }], skippedOffline: [] });
    await renderPage();
    await screen.findByText('远程探针');
    fireEvent.input(screen.getByPlaceholderText('设备 ID'), { target: { value: 'd-1' } });
    const timeoutInput = document.querySelector('input[type="number"]') as HTMLInputElement;
    fireEvent.input(timeoutInput, { target: { value: '5' } });
    fireEvent.click(screen.getByText('下发探针'));
    await waitFor(() => expect(dispatch).toHaveBeenCalled());
    expect(dispatch.mock.calls[0][0].timeoutMs).toBe(100);
  });

  it('template button fills the script editor', async () => {
    await renderPage();
    await screen.findByText('远程探针');
    // 模板现为 badge 按钮，点击「设备指纹」即把模板 body 填入脚本编辑器
    fireEvent.click(screen.getByText('设备指纹'));
    await waitFor(() => expect(scriptEditor().value).toContain('ctx.nav.ua'));
  });

  it('recent batches render and replay fills editor + note', async () => {
    list.mockResolvedValue({
      rows: [
        {
          id: 'r-1', batchId: 'batch-abcdef12', deviceId: 'd-1', adminId: 'a', adminUsername: 'admin',
          scriptBody: 'return 7;', scriptSha256: 's', hasCmdCall: false, note: '排查 OOM',
          timeoutMs: 5000, status: 'ok', resultJson: null, stderr: null, durationMs: 1,
          truncated: false, dispatchedAt: '2026-06-04T10:00:00Z', confirmedAt: null, completedAt: null,
        },
      ],
      total: 1,
    });
    await renderPage();
    // 最近 batch 表格：note + 「回放」操作按钮
    await waitFor(() => expect(screen.getByText('排查 OOM')).toBeInTheDocument());
    const replayBtn = screen.getByText('回放');
    fireEvent.click(replayBtn);
    await waitFor(() => expect(scriptEditor().value).toBe('return 7;'));
  });

  it('exportBatchJson triggers a blob download', async () => {
    dispatch.mockResolvedValue({ batchId: 'b-exp', dispatched: [{ deviceId: 'd', requestId: 'r' }], skippedOffline: [] });
    const createObjectURL = vi.fn(() => 'blob:fake');
    const revokeObjectURL = vi.fn();
    Object.defineProperty(URL, 'createObjectURL', { configurable: true, value: createObjectURL });
    Object.defineProperty(URL, 'revokeObjectURL', { configurable: true, value: revokeObjectURL });
    const clickSpy = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {});
    await renderPage();
    await screen.findByText('远程探针');
    fireEvent.input(screen.getByPlaceholderText('设备 ID'), { target: { value: 'd-1' } });
    fireEvent.click(screen.getByText('下发探针'));
    await waitFor(() => expect(connectStream).toHaveBeenCalled());
    const cbs = connectStream.mock.calls[0][1];
    cbs.onResult({ deviceId: 'd', requestId: 'r', batchId: 'b-exp', status: 'ok', resultJson: { a: 1 } });
    await waitFor(() => expect(screen.getByText('导出 JSON')).toBeInTheDocument());
    fireEvent.click(screen.getByText('导出 JSON'));
    expect(createObjectURL).toHaveBeenCalled();
    expect(clickSpy).toHaveBeenCalled();
    expect(revokeObjectURL).toHaveBeenCalled();
    clickSpy.mockRestore();
  });

  it('clear button empties result grid', async () => {
    dispatch.mockResolvedValue({ batchId: 'b-clr', dispatched: [{ deviceId: 'd', requestId: 'r' }], skippedOffline: [] });
    await renderPage();
    await screen.findByText('远程探针');
    fireEvent.input(screen.getByPlaceholderText('设备 ID'), { target: { value: 'd-1' } });
    fireEvent.click(screen.getByText('下发探针'));
    await waitFor(() => expect(connectStream).toHaveBeenCalled());
    const cbs = connectStream.mock.calls[0][1];
    cbs.onResult({ deviceId: 'd', requestId: 'r', batchId: 'b-clr', status: 'ok', resultJson: { a: 1 } });
    // onCompleted 将 sending 置回 false，否则清空后会落到 <Loading> 而非空态
    cbs.onCompleted({ received: 1, expected: 1 });
    await waitFor(() => expect(screen.getByText('清空')).toBeInTheDocument());
    fireEvent.click(screen.getByText('清空'));
    await waitFor(() => expect(screen.getByText('尚无结果')).toBeInTheDocument());
  });

  it('SSE onError falls back to polling and warns the user', async () => {
    // 重设计：onError 不再渲染「SSE 错误」文案，而是 toast.warning + 切换轮询。
    list.mockResolvedValue({ rows: [], total: 0 });
    dispatch.mockResolvedValue({ batchId: 'b-sse', dispatched: [{ deviceId: 'd', requestId: 'r' }], skippedOffline: [] });
    await renderPage();
    await screen.findByText('远程探针');
    fireEvent.input(screen.getByPlaceholderText('设备 ID'), { target: { value: 'd-1' } });
    fireEvent.click(screen.getByText('下发探针'));
    await waitFor(() => expect(connectStream).toHaveBeenCalled());
    const cbs = connectStream.mock.calls[0][1];
    cbs.onError(new Error('connection lost'));
    // 轮询兜底会再次调用 probeApi.list（带 batchId）
    await waitFor(() => expect(mockToast.warning).toHaveBeenCalled());
    await waitFor(() =>
      expect(list.mock.calls.some((c) => c[0]?.batchId === 'b-sse')).toBe(true));
  });

  it('confirm_required result opens the confirm modal', async () => {
    dispatch.mockResolvedValue({ batchId: 'b-cd', dispatched: [{ deviceId: 'dev-xyz999', requestId: 'r-cd' }], skippedOffline: [] });
    await renderPage();
    await screen.findByText('远程探针');
    fireEvent.input(screen.getByPlaceholderText('设备 ID'), { target: { value: 'dev-xyz999' } });
    fireEvent.click(screen.getByText('下发探针'));
    await waitFor(() => expect(connectStream).toHaveBeenCalled());
    const cbs = connectStream.mock.calls[0][1];
    cbs.onResult({
      deviceId: 'dev-xyz999', requestId: 'r-cd', batchId: 'b-cd', status: 'confirm_required',
      resultJson: { _actions: [{ type: 'reload' }, { type: 'signOut' }] },
    });
    // 结果卡片出现「确认执行」按钮 → 点击打开 wf <Modal>
    await waitFor(() => expect(screen.getByText('确认执行')).toBeInTheDocument());
    fireEvent.click(screen.getByText('确认执行'));
    await waitFor(() => expect(screen.getByText('确认远程受控写')).toBeInTheDocument());
    // 预览应包含从 _actions 提取的 type
    expect(screen.getByText('reload, signOut')).toBeInTheDocument();
  });
});

// ═════════════════════════════════════════════════════════════════════════════
// 6. api-bridge.ts —— serializeAndTruncate / executeActions / handlers
// ═════════════════════════════════════════════════════════════════════════════
describe('api-bridge (workers/probe)', () => {
  const post = vi.fn();
  const connectSseStream = vi.fn();
  const stopSse = vi.fn();
  const collectCtxSnapshot = vi.fn();

  let lastWorker: FakeWorker | undefined;
  class FakeWorker {
    onmessage: ((ev: MessageEvent) => void) | null = null;
    onerror: ((ev: any) => void) | null = null;
    posted: unknown[] = [];
    terminated = false;
    constructor() {
      lastWorker = this;
    }
    postMessage(msg: unknown) {
      this.posted.push(msg);
    }
    terminate() {
      this.terminated = true;
    }
    // 测试驱动：模拟 worker 返回结果
    emit(data: unknown) {
      this.onmessage?.({ data } as MessageEvent);
    }
  }

  // vitest 下 `new URL('./runner.worker.ts', import.meta.url)` 会抛 Invalid URL，
  // 在 new Worker(...) 前就中断；包一层让相对 worker 路径退化为合法 dummy URL，
  // 使 runScript 能真正构造 FakeWorker（http / ctx 均已 mock，URL 无其他用途）。
  const RealURL = globalThis.URL;
  beforeEach(() => {
    vi.clearAllMocks();
    lastWorker = undefined;
    post.mockResolvedValue(undefined);
    connectSseStream.mockReturnValue(stopSse);
    collectCtxSnapshot.mockResolvedValue({ fake: 'snapshot' });
    (globalThis as any).Worker = FakeWorker as unknown as typeof Worker;
    (globalThis as any).URL = function PatchedURL(input: string, base?: string) {
      try {
        return new RealURL(input, base as string);
      } catch {
        return new RealURL('http://localhost/runner.worker.js');
      }
    } as unknown as typeof URL;
    (globalThis as any).URL.createObjectURL = RealURL.createObjectURL?.bind(RealURL);
    (globalThis as any).URL.revokeObjectURL = RealURL.revokeObjectURL?.bind(RealURL);
    vi.doMock('@/api/http', () => ({ api: { post }, connectSseStream }));
    vi.doMock('@/workers/probe/ctx-factory', () => ({ collectCtxSnapshot }));
    vi.resetModules();
  });

  afterEach(() => {
    (globalThis as any).URL = RealURL;
    vi.doUnmock('@/api/http');
    vi.doUnmock('@/workers/probe/ctx-factory');
    vi.resetModules();
  });

  it('serializeAndTruncate returns json untruncated for small payloads', async () => {
    const mod = await import('@/workers/probe/api-bridge');
    const r = mod.serializeAndTruncate({ a: 1 });
    expect(r.truncated).toBe(false);
    expect(r.json).toBe('{"a":1}');
  });

  it('serializeAndTruncate truncates payloads over 256KB', async () => {
    const mod = await import('@/workers/probe/api-bridge');
    const big = { s: 'x'.repeat(300 * 1024) };
    const r = mod.serializeAndTruncate(big);
    expect(r.truncated).toBe(true);
    expect(r.json.length).toBe(256 * 1024);
  });

  it('serializeAndTruncate handles circular (unserializable) values', async () => {
    const mod = await import('@/workers/probe/api-bridge');
    const c: Record<string, unknown> = {};
    c.self = c;
    const r = mod.serializeAndTruncate(c);
    expect(r.truncated).toBe(false);
    expect(r.json).toContain('unserializable');
  });

  it('serializeAndTruncate handles undefined (JSON.stringify → undefined)', async () => {
    const mod = await import('@/workers/probe/api-bridge');
    const r = mod.serializeAndTruncate(undefined);
    expect(r.truncated).toBe(false);
    expect(r.json).toContain('unserializable');
  });

  it('startProbeBridge connects once; stopProbeBridge stops', async () => {
    const mod = await import('@/workers/probe/api-bridge');
    mod.startProbeBridge();
    mod.startProbeBridge(); // 二次调用应早退（已连）
    expect(connectSseStream).toHaveBeenCalledTimes(1);
    mod.stopProbeBridge();
    expect(stopSse).toHaveBeenCalledTimes(1);
  });

  it('handleProbeRequest posts unsupported_ctx_version when ctxVersion mismatches', async () => {
    const mod = await import('@/workers/probe/api-bridge');
    mod.startProbeBridge();
    const cbs = connectSseStream.mock.calls[0][0];
    await cbs.onProbeRequest({ requestId: 'r-1', batchId: 'b', scriptB64: '', timeoutMs: 1000, ctxVersion: 99 });
    expect(post).toHaveBeenCalledWith('/api/probe/results', expect.objectContaining({
      requestId: 'r-1', status: 'unsupported_ctx_version',
    }));
  });

  it('handleProbeRequest runs script and posts ok result', async () => {
    const mod = await import('@/workers/probe/api-bridge');
    mod.startProbeBridge();
    const cbs = connectSseStream.mock.calls[0][0];
    const scriptB64 = btoa('return {ok:1};');
    const p = cbs.onProbeRequest({ requestId: 'r-2', batchId: 'b', scriptB64, timeoutMs: 1000, ctxVersion: 1 });
    // 等待 collectCtxSnapshot + worker spawn
    await vi.waitFor(() => expect(lastWorker).toBeDefined());
    expect(lastWorker!.posted[0]).toMatchObject({ script: 'return {ok:1};', confirmed: false });
    lastWorker!.emit({ ok: true, result: { ok: 1 }, actions: [], durationMs: 5 });
    await p;
    await vi.waitFor(() =>
      expect(post).toHaveBeenCalledWith('/api/probe/results', expect.objectContaining({ requestId: 'r-2', status: 'ok' })));
  });

  it('handleProbeRequest posts error on base64 decode failure', async () => {
    const mod = await import('@/workers/probe/api-bridge');
    mod.startProbeBridge();
    const cbs = connectSseStream.mock.calls[0][0];
    // atob 在 happy-dom 下对非法字符抛错
    await cbs.onProbeRequest({ requestId: 'r-bad', batchId: 'b', scriptB64: '@@@not-base64@@@', timeoutMs: 1000, ctxVersion: 1 });
    await vi.waitFor(() =>
      expect(post).toHaveBeenCalledWith('/api/probe/results', expect.objectContaining({
        requestId: 'r-bad', status: 'error',
      })));
  });

  it('runScript emits confirm_required when unconfirmed script has actions', async () => {
    const mod = await import('@/workers/probe/api-bridge');
    mod.startProbeBridge();
    const cbs = connectSseStream.mock.calls[0][0];
    const scriptB64 = btoa('ctx.cmd.reload();');
    const p = cbs.onProbeRequest({ requestId: 'r-cmd', batchId: 'b', scriptB64, timeoutMs: 1000, ctxVersion: 1 });
    await vi.waitFor(() => expect(lastWorker).toBeDefined());
    lastWorker!.emit({ ok: true, result: {}, actions: [{ type: 'reload' }], durationMs: 3 });
    await p;
    await vi.waitFor(() =>
      expect(post).toHaveBeenCalledWith('/api/probe/results', expect.objectContaining({
        requestId: 'r-cmd', status: 'confirm_required',
      })));
    // pending confirm 缓存里应有该 requestId
    expect(mod._testGetPendingConfirms().has('r-cmd')).toBe(true);
  });

  it('worker error path posts error result', async () => {
    const mod = await import('@/workers/probe/api-bridge');
    mod.startProbeBridge();
    const cbs = connectSseStream.mock.calls[0][0];
    const scriptB64 = btoa('return 1;');
    const p = cbs.onProbeRequest({ requestId: 'r-err', batchId: 'b', scriptB64, timeoutMs: 1000, ctxVersion: 1 });
    await vi.waitFor(() => expect(lastWorker).toBeDefined());
    lastWorker!.onerror?.({ message: 'worker crashed' });
    await p;
    await vi.waitFor(() =>
      expect(post).toHaveBeenCalledWith('/api/probe/results', expect.objectContaining({
        requestId: 'r-err', status: 'error',
      })));
  });

  it('worker not-ok result posts error with stderr', async () => {
    const mod = await import('@/workers/probe/api-bridge');
    mod.startProbeBridge();
    const cbs = connectSseStream.mock.calls[0][0];
    const scriptB64 = btoa('throw 1;');
    const p = cbs.onProbeRequest({ requestId: 'r-ng', batchId: 'b', scriptB64, timeoutMs: 1000, ctxVersion: 1 });
    await vi.waitFor(() => expect(lastWorker).toBeDefined());
    lastWorker!.emit({ ok: false, stderr: 'ReferenceError', durationMs: 2 });
    await p;
    await vi.waitFor(() =>
      expect(post).toHaveBeenCalledWith('/api/probe/results', expect.objectContaining({
        requestId: 'r-ng', status: 'error', stderr: 'ReferenceError',
      })));
  });

  it('handleProbeConfirm posts cache-miss error for unknown requestId', async () => {
    const mod = await import('@/workers/probe/api-bridge');
    mod.startProbeBridge();
    const cbs = connectSseStream.mock.calls[0][0];
    await cbs.onProbeConfirm({ requestId: 'nope', confirmToken: 't' });
    expect(post).toHaveBeenCalledWith('/api/probe/results', expect.objectContaining({
      requestId: 'nope', status: 'error',
    }));
  });

  it('executeActions clearCache clears storages and caches', async () => {
    const mod = await import('@/workers/probe/api-bridge');
    const cacheDelete = vi.fn().mockResolvedValue(true);
    (globalThis as any).caches = {
      keys: vi.fn().mockResolvedValue(['c1', 'c2']),
      delete: cacheDelete,
    };
    localStorage.setItem('k', 'v');
    sessionStorage.setItem('k', 'v');
    await mod.executeActions([{ type: 'clearCache' } as any]);
    expect(localStorage.getItem('k')).toBeNull();
    expect(cacheDelete).toHaveBeenCalledTimes(2);
    delete (globalThis as any).caches;
  });

  it('executeActions reload calls window.location.reload', async () => {
    const mod = await import('@/workers/probe/api-bridge');
    const reload = window.location.reload as unknown as ReturnType<typeof vi.fn>;
    reload.mockClear?.();
    await mod.executeActions([{ type: 'reload' } as any]);
    expect(reload).toHaveBeenCalled();
  });

  it('executeActions signOut short-circuits before reload', async () => {
    const mod = await import('@/workers/probe/api-bridge');
    const assign = window.location.assign as unknown as ReturnType<typeof vi.fn>;
    const reload = window.location.reload as unknown as ReturnType<typeof vi.fn>;
    assign.mockClear?.();
    reload.mockClear?.();
    await mod.executeActions([{ type: 'reload' } as any, { type: 'signOut' } as any]);
    expect(assign).toHaveBeenCalledWith('/admin/login');
    // signOut 短路 → reload 不应被调用
    expect(reload).not.toHaveBeenCalled();
  });
});