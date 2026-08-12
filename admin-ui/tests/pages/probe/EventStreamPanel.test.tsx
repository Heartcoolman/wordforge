import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@solidjs/testing-library';
import type { StreamEvent } from '@/types/probeTelemetry';

const stream = vi.fn();
vi.mock('@/api/probeTelemetry', () => ({
  probeTelemetryApi: { stream: (...a: unknown[]) => stream(...a) },
}));

function evt(o: Partial<StreamEvent> & { id: string }): StreamEvent {
  return {
    ts: '2026-05-29T11:59:50Z',
    type: 'periodic',
    deviceId: 'dev_abcdef1234567',
    metrics: [],
    payloadRaw: '{}',
    ...o,
  };
}

// 四条事件分别命中 category() 的四个分支：periodic→behavior / word_answer→learn
// / perf_nav→perf / error_js→err
const E_BEHAVIOR = evt({
  id: 'e-behavior',
  type: 'periodic',
  device: { os: 'Android 15', model: 'Pixel 6', online: true, language: 'zh-CN' },
  metrics: [
    { key: 'actionsPerMin', value: '2' },
    { key: 'avgResponseTimeMs', value: '130' },
  ],
  payloadRaw: '{"actionsPerMin":2}',
});
const E_LEARN = evt({ id: 'e-learn', type: 'word_answer', deviceId: 'dev_zzz99999', payloadRaw: 'not-json{' });
const E_PERF = evt({
  id: 'e-perf',
  type: 'perf_nav',
  device: { online: false },
  metrics: [{ key: 'scrollDepthPct', value: '40' }],
});
const E_ERR = evt({ id: 'e-err', type: 'error_js', metrics: [{ key: 'errorCount', value: '3' }] });

const ALL = [E_BEHAVIOR, E_LEARN, E_PERF, E_ERR];

async function renderPanel() {
  const { EventStreamPanel } = await import('@/pages/probe/EventStreamPanel');
  return render(() => <EventStreamPanel />);
}

/** 覆盖滚动锚定分支：happy-dom 无布局引擎（getBoundingClientRect 恒 0），
 *  这里按 data-eid 造 top 序列——同一行的第 n 次取值可不同，用来模拟"新事件插到
 *  最前把锚定行往下推"的布局位移。 */
function stubRects(seq: Record<string, number[]>) {
  const orig = Element.prototype.getBoundingClientRect;
  const hits: Record<string, number> = {};
  const rect = (top: number) =>
    ({ top, bottom: top + 90, height: 90, width: 0, left: 0, right: 0, x: 0, y: top, toJSON: () => ({}) }) as DOMRect;
  Element.prototype.getBoundingClientRect = function (this: Element) {
    const eid = (this as HTMLElement).dataset?.eid;
    if (eid != null && seq[eid]) {
      const list = seq[eid];
      const i = hits[eid] ?? 0;
      hits[eid] = i + 1;
      return rect(list[Math.min(i, list.length - 1)]);
    }
    if (this.classList?.contains('pb-stream-body')) return rect(0);
    return orig.call(this);
  };
  return () => {
    Element.prototype.getBoundingClientRect = orig;
  };
}

describe('EventStreamPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    stream.mockResolvedValue({ events: ALL });
    vi.useFakeTimers({ shouldAdvanceTime: true });
  });
  afterEach(() => {
    vi.clearAllTimers();
    vi.useRealTimers();
  });

  it('首帧未返回时只显示 Spinner，不显示空态', async () => {
    stream.mockReturnValue(new Promise<never>(() => {}));
    await renderPanel();
    expect(screen.getByRole('status')).toBeInTheDocument();
    expect(screen.queryByText('暂无遥测事件')).toBeNull();
  });

  it('空事件列表渲染空态', async () => {
    stream.mockResolvedValue({ events: [] });
    await renderPanel();
    await waitFor(() => expect(screen.getByText('暂无遥测事件')).toBeInTheDocument());
    expect(screen.getByText('当前没有设备上报，或无匹配筛选')).toBeInTheDocument();
    expect(stream).toHaveBeenCalledWith(30);
  });

  it('渲染事件卡：时间 / 中文类型 / 设备摘要 / 中文指标与单位', async () => {
    await renderPanel();
    await waitFor(() => expect(screen.getByText('周期上报')).toBeInTheDocument());
    // hms 走本地时区，只断言格式
    expect(screen.getAllByText(/^\d{2}:\d{2}:\d{2}$/).length).toBe(4);
    // 两条事件共用默认 deviceId，故取全部
    expect(screen.getAllByText('设备 dev_abcd…').length).toBe(3);
    expect(screen.getByText('设备 dev_zzz9…')).toBeInTheDocument();
    expect(screen.getByText('Android 15 · Pixel 6')).toBeInTheDocument();
    expect(screen.getByText(/zh-CN/)).toBeInTheDocument();
    expect(screen.getByText('在线')).toBeInTheDocument();
    // metricLabel + metricUnit
    expect(screen.getByText('每分钟操作')).toBeInTheDocument();
    expect(screen.getByText('平均响应')).toBeInTheDocument();
    expect(screen.getByText('130 ms')).toBeInTheDocument();
    expect(screen.getByText('40%')).toBeInTheDocument();
    // 未知类型回退原值
    expect(screen.getByText('perf_nav')).toBeInTheDocument();
  });

  it('设备缺 os/model 显示"未知设备"并标离线；无 device 且无 metrics 显示无结构化字段', async () => {
    await renderPanel();
    await waitFor(() => expect(screen.getByText('未知设备')).toBeInTheDocument());
    expect(screen.getByText('离线')).toBeInTheDocument();
    // E_LEARN 既无 device 也无 metrics
    expect(screen.getByText('无可解析的结构化字段')).toBeInTheDocument();
  });

  it('头部显示类型去重计数', async () => {
    const { container } = await renderPanel();
    await waitFor(() => expect(screen.getByText('周期上报')).toBeInTheDocument());
    expect(container.querySelector('.pb-stream-head .rate b')?.textContent).toBe('4');
  });

  it('分类筛选：行为 / 学习 / 性能 / 错误 各自只留同类事件，再回全部', async () => {
    const { container } = await renderPanel();
    await waitFor(() => expect(screen.getByText('周期上报')).toBeInTheDocument());
    const rows = () => container.querySelectorAll('.pb-event').length;

    fireEvent.click(screen.getByRole('button', { name: '学习' }));
    await waitFor(() => expect(rows()).toBe(1));
    expect(screen.getByText('word_answer')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '错误' }));
    await waitFor(() => expect(container.querySelector('.pb-event.is-err')).not.toBeNull());
    expect(rows()).toBe(1);

    fireEvent.click(screen.getByRole('button', { name: '性能' }));
    await waitFor(() => expect(container.querySelector('.pb-event.is-perf')).not.toBeNull());

    fireEvent.click(screen.getByRole('button', { name: '行为' }));
    await waitFor(() => expect(rows()).toBe(1));
    expect(screen.getByText('周期上报')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '全部' }));
    await waitFor(() => expect(rows()).toBe(4));
  });

  it('筛选 chip 支持 Enter / 空格 键盘触发，aria-pressed 跟随', async () => {
    const { container } = await renderPanel();
    await waitFor(() => expect(screen.getByText('周期上报')).toBeInTheDocument());
    const learnChip = screen.getByRole('button', { name: '学习' });
    fireEvent.keyDown(learnChip, { key: 'Enter' });
    await waitFor(() => expect(learnChip).toHaveAttribute('aria-pressed', 'true'));
    expect(container.querySelectorAll('.pb-event').length).toBe(1);

    const allChip = screen.getByRole('button', { name: '全部' });
    fireEvent.keyDown(allChip, { key: ' ' });
    await waitFor(() => expect(container.querySelectorAll('.pb-event').length).toBe(4));
    // 无关按键不改变筛选
    fireEvent.keyDown(learnChip, { key: 'a' });
    expect(container.querySelectorAll('.pb-event').length).toBe(4);
  });

  it('筛选无匹配时回落空态', async () => {
    stream.mockResolvedValue({ events: [E_BEHAVIOR] });
    await renderPanel();
    await waitFor(() => expect(screen.getByText('周期上报')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '错误' }));
    await waitFor(() => expect(screen.getByText('暂无遥测事件')).toBeInTheDocument());
  });

  it('每 5 秒轮询一次', async () => {
    await renderPanel();
    await waitFor(() => expect(stream).toHaveBeenCalledTimes(1));
    vi.advanceTimersByTime(5000);
    await waitFor(() => expect(stream).toHaveBeenCalledTimes(2));
    vi.advanceTimersByTime(5000);
    await waitFor(() => expect(stream).toHaveBeenCalledTimes(3));
  });

  it('暂停后停止轮询并改文案，继续时立即拉一次', async () => {
    await renderPanel();
    await waitFor(() => expect(stream).toHaveBeenCalledTimes(1));

    const pauseBtn = screen.getByRole('button', { name: /暂停/ });
    fireEvent.click(pauseBtn);
    await waitFor(() => expect(screen.getByText(/已暂停/)).toBeInTheDocument());
    vi.advanceTimersByTime(15000);
    expect(stream).toHaveBeenCalledTimes(1);

    const resumeBtn = screen.getByRole('button', { name: /继续/ });
    expect(resumeBtn).toHaveAttribute('aria-pressed', 'true');
    fireEvent.click(resumeBtn);
    await waitFor(() => expect(stream).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(screen.getByText(/每 5 秒刷新/)).toBeInTheDocument());
  });

  it('原始 JSON 展开/收起：合法 payload 美化，非法 payload 原样', async () => {
    await renderPanel();
    await waitFor(() => expect(screen.getByText('周期上报')).toBeInTheDocument());
    const rawBtns = screen.getAllByRole('button', { name: '原始' });
    expect(rawBtns.length).toBe(4);

    fireEvent.click(rawBtns[0]);
    await waitFor(() => expect(screen.getByText(/"actionsPerMin": 2/)).toBeInTheDocument());
    expect(screen.getAllByRole('button', { name: '收起' }).length).toBe(1);

    // 第二条 payloadRaw 非法 JSON → 原样输出
    fireEvent.click(screen.getAllByRole('button', { name: '原始' })[0]);
    await waitFor(() => expect(screen.getByText('not-json{')).toBeInTheDocument());

    fireEvent.click(screen.getAllByRole('button', { name: '收起' })[0]);
    await waitFor(() => expect(screen.queryByText(/"actionsPerMin": 2/)).toBeNull());
    // 另一条仍展开，互不影响
    expect(screen.getByText('not-json{')).toBeInTheDocument();
  });

  it('刷新按 id 复用旧行引用：同 id 内容变化不重渲染，新 id 才进列表', async () => {
    stream.mockResolvedValueOnce({ events: [E_BEHAVIOR] });
    const { container } = await renderPanel();
    await waitFor(() => expect(screen.getByText('周期上报')).toBeInTheDocument());

    // 同 id 但 type 改成 error_js；另加一条新 id
    stream.mockResolvedValue({
      events: [evt({ id: 'e-new', type: 'session_start' }), { ...E_BEHAVIOR, type: 'error_js' }],
    });
    vi.advanceTimersByTime(5000);
    await waitFor(() => expect(container.querySelectorAll('.pb-event').length).toBe(2));
    expect(screen.getByText('会话开始')).toBeInTheDocument();
    // 旧行引用被复用 → 仍显示旧类型
    expect(screen.getByText('周期上报')).toBeInTheDocument();
    expect(screen.queryByText('前端错误')).toBeNull();
  });

  it('刷新时若已在顶部则钉住顶部', async () => {
    stream.mockResolvedValueOnce({ events: [E_BEHAVIOR] });
    const { container } = await renderPanel();
    await waitFor(() => expect(screen.getByText('周期上报')).toBeInTheDocument());
    const body = container.querySelector('.pb-stream-body') as HTMLDivElement;
    body.scrollTop = 5; // ≤8 视为顶部

    stream.mockResolvedValue({ events: [evt({ id: 'e-new', type: 'session_start' }), E_BEHAVIOR] });
    vi.advanceTimersByTime(5000);
    await waitFor(() => expect(screen.getByText('会话开始')).toBeInTheDocument());
    expect(body.scrollTop).toBe(0);
  });

  it('刷新时非顶部按首个可见行做滚动锚定补偿', async () => {
    // e-behavior 三次取值：firstVisibleId / 补偿前 top0 / 补偿后 top1（被新行推下 100px）
    const restore = stubRects({ 'e-behavior': [10, 10, 110], 'e-learn': [110] });
    try {
      stream.mockResolvedValueOnce({ events: [E_BEHAVIOR, E_LEARN] });
      const { container } = await renderPanel();
      await waitFor(() => expect(screen.getByText('周期上报')).toBeInTheDocument());
      const body = container.querySelector('.pb-stream-body') as HTMLDivElement;
      body.scrollTop = 100;

      stream.mockResolvedValue({ events: [evt({ id: 'e-new', type: 'session_start' }), E_BEHAVIOR, E_LEARN] });
      vi.advanceTimersByTime(5000);
      await waitFor(() => expect(screen.getByText('会话开始')).toBeInTheDocument());
      expect(body.scrollTop).toBe(200);
    } finally {
      restore();
    }
  });

  it('刷新时非顶部且无可见锚定行则保持原滚动位置', async () => {
    stream.mockResolvedValueOnce({ events: [E_BEHAVIOR] });
    const { container } = await renderPanel();
    await waitFor(() => expect(screen.getByText('周期上报')).toBeInTheDocument());
    const body = container.querySelector('.pb-stream-body') as HTMLDivElement;
    body.scrollTop = 100;

    stream.mockResolvedValue({ events: [evt({ id: 'e-new', type: 'session_start' }), E_BEHAVIOR] });
    vi.advanceTimersByTime(5000);
    await waitFor(() => expect(screen.getByText('会话开始')).toBeInTheDocument());
    // 无布局 → 找不到可见行，既不补偿也不回顶
    expect(body.scrollTop).toBe(100);
  });
});
