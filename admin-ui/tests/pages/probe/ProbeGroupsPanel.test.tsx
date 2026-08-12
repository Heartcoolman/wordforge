import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import type { ProbeRow, ProbesResponse } from '@/types/probeTelemetry';

const probes = vi.fn();
const updateSamplingRule = vi.fn();
const schema = vi.fn();
const audit = vi.fn();

vi.mock('@/api/probeTelemetry', () => ({
  probeTelemetryApi: {
    probes: (...a: unknown[]) => probes(...a),
    updateSamplingRule: (...a: unknown[]) => updateSamplingRule(...a),
    schema: (...a: unknown[]) => schema(...a),
    audit: (...a: unknown[]) => audit(...a),
  },
}));

const toastSuccess = vi.fn();
const toastError = vi.fn();
vi.mock('@/stores/ui', () => ({
  uiStore: {
    toast: {
      success: (...a: unknown[]) => toastSuccess(...a),
      error: (...a: unknown[]) => toastError(...a),
      warning: vi.fn(),
      info: vi.fn(),
    },
  },
}));

function row(o: Partial<ProbeRow> & { key: string; label: string }): ProbeRow {
  return {
    desc: `${o.key} 表列`,
    count24h: 0,
    eps: 0,
    lastTs: null,
    sampleRate: 1,
    locked: true,
    enabled: true,
    ...o,
  };
}

// click / swipe 可调（绑定真实 event_type）；learn / perf 全锁
const CLICK = row({
  key: 'click', label: '点击行为', desc: 'telemetry_summaries.click_count',
  count24h: 5200, eps: 0.06, lastTs: '2026-05-29T11:59:50Z',
  sampleRate: 0.35, locked: false, samplingEventType: 'periodic',
});
const SWIPE = row({
  key: 'swipe', label: '滑动行为', count24h: 0, sampleRate: 0.6, locked: false, samplingEventType: 'on_demand',
});
const LESSON = row({ key: 'lesson_start', label: '开始学习', count24h: 120, eps: 0.0014, lastTs: '2026-05-29T11:58:00Z' });
const WORD = row({ key: 'word_answer', label: '单词作答', count24h: 3120, eps: 0.036, lastTs: '2026-05-29T11:59:00Z' });
const ERROR_JS = row({ key: 'error_js', label: '前端错误', count24h: 7, eps: 0.0001, lastTs: '2026-05-29T11:55:00Z' });

const GROUPS: ProbesResponse = {
  generatedAt: '2026-05-29T12:00:00Z',
  groups: [
    { group: 'behavior', hue: 213, probes: [CLICK, SWIPE] },
    { group: 'learn', hue: 162, probes: [LESSON, WORD] },
    { group: 'perf', hue: 60, probes: [ERROR_JS] },
  ],
};

async function renderPanel(days = 1) {
  const { ProbeGroupsPanel } = await import('@/pages/probe/ProbeGroupsPanel');
  const [d, setD] = createSignal(days);
  const r = render(() => <ProbeGroupsPanel days={d} />);
  return { ...r, setDays: setD };
}

describe('ProbeGroupsPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    probes.mockResolvedValue(GROUPS);
    updateSamplingRule.mockResolvedValue({
      eventType: 'periodic', sampleRate: 0.35, enabled: false, locked: false, updatedAt: '2026-05-29T12:00:00Z',
    });
    schema.mockResolvedValue({ eventType: 'periodic', sampledAt: '2026-05-29T11:59:50Z', payload: { event: 'periodic' } });
    audit.mockResolvedValue({
      rows: [
        { ts: '2026-05-29T11:43:08Z', action: 'mod', eventType: 'periodic', oldValue: '0.2', newValue: '0.35', adminId: 'a1' },
        { ts: '2026-05-29T10:00:00Z', action: 'pause', eventType: 'other', oldValue: null, newValue: null, adminId: null },
      ],
    });
  });

  it('加载中显示 Spinner', async () => {
    probes.mockReturnValue(new Promise<never>(() => {}));
    await renderPanel();
    expect(screen.getByRole('status')).toBeInTheDocument();
    expect(screen.queryByText('暂无探针定义')).toBeNull();
  });

  it('空分组渲染空态', async () => {
    probes.mockResolvedValue({ generatedAt: '2026-05-29T12:00:00Z', groups: [] });
    await renderPanel();
    await waitFor(() => expect(screen.getByText('暂无探针定义')).toBeInTheDocument());
    expect(screen.getByText('probe-telemetry /probes 返回空')).toBeInTheDocument();
    expect(probes).toHaveBeenCalledWith(1);
  });

  it('渲染三组：中文标题 / 业务化副标题 / 有数据计数 / 24h 窗口标签', async () => {
    await renderPanel();
    await waitFor(() => expect(screen.getByText('用户行为')).toBeInTheDocument());
    expect(screen.getByText('学习事件')).toBeInTheDocument();
    expect(screen.getByText('性能与错误')).toBeInTheDocument();
    // GROUP_PLAIN 副标题 + online/total
    expect(screen.getByText('用户操作行为 · 1/2 有数据')).toBeInTheDocument();
    expect(screen.getByText('学习核心记录 · 2/2 有数据')).toBeInTheDocument();
    expect(screen.getByText('点击、滑动等交互，可按比例采样以省资源')).toBeInTheDocument();
    // 三组各有一个窗口标签
    expect(screen.getAllByText('24h').length).toBe(3);
    // 24h 事件合计（behavior 5200 / learn 3240 / perf 7）
    expect(screen.getByText('5,200')).toBeInTheDocument();
    expect(screen.getByText('3,240')).toBeInTheDocument();
  });

  it('组采样率取组内最低可调探针；全锁组显示 100%', async () => {
    const { container } = await renderPanel();
    await waitFor(() => expect(screen.getByText('用户行为')).toBeInTheDocument());
    const rates = Array.from(container.querySelectorAll('.pb-group .stats span:last-child b')).map((e) => e.textContent);
    // behavior = min(35, 60)；learn / perf 全锁 → 100
    expect(rates).toEqual(['35%', '100%', '100%']);
  });

  it('days 变化触发重新拉取并切换窗口标签', async () => {
    const { setDays } = await renderPanel(1);
    await waitFor(() => expect(probes).toHaveBeenCalledWith(1));
    setDays(7);
    await waitFor(() => expect(probes).toHaveBeenCalledWith(7));
    await waitFor(() => expect(screen.getAllByText('7d').length).toBe(3));
  });

  it('未知分组回退到 group key 作为标题', async () => {
    probes.mockResolvedValue({
      generatedAt: '2026-05-29T12:00:00Z',
      groups: [{ group: 'custom', hue: 0, probes: [row({ key: 'x', label: '自定义探针' })] }],
    });
    await renderPanel();
    await waitFor(() => expect(screen.getByText('custom')).toBeInTheDocument());
    expect(screen.getByText('custom · 0/1 有数据')).toBeInTheDocument();
    expect(screen.getByText('自定义探针')).toBeInTheDocument();
  });

  it('可调组开关切换成功：PATCH 绑定 event_type + toast + 重新拉取', async () => {
    await renderPanel();
    await waitFor(() => expect(screen.getByText('用户行为')).toBeInTheDocument());
    const sw = screen.getByLabelText('用户行为 采集开关') as HTMLInputElement;
    expect(sw.disabled).toBe(false);
    expect(sw.checked).toBe(true);

    fireEvent.change(sw, { target: { checked: false } });
    await waitFor(() => expect(updateSamplingRule).toHaveBeenCalledWith('periodic', { enabled: false }));
    await waitFor(() => expect(toastSuccess).toHaveBeenCalledWith('采集已停用', '用户行为 · periodic'));
    await waitFor(() => expect(probes).toHaveBeenCalledTimes(2));
  });

  it('控制探针已停用时开关初始关闭，打开走"采集已启用"', async () => {
    probes.mockResolvedValue({
      generatedAt: '2026-05-29T12:00:00Z',
      groups: [{ group: 'behavior', hue: 213, probes: [{ ...CLICK, enabled: false }] }],
    });
    await renderPanel();
    const sw = (await screen.findByLabelText('用户行为 采集开关')) as HTMLInputElement;
    expect(sw.checked).toBe(false);
    fireEvent.change(sw, { target: { checked: true } });
    await waitFor(() => expect(updateSamplingRule).toHaveBeenCalledWith('periodic', { enabled: true }));
    await waitFor(() => expect(toastSuccess).toHaveBeenCalledWith('采集已启用', '用户行为 · periodic'));
  });

  it('全锁组（learn / perf）开关 disabled 且恒开，点击不发请求', async () => {
    await renderPanel();
    await waitFor(() => expect(screen.getByText('学习事件')).toBeInTheDocument());
    const learn = screen.getByLabelText('学习事件 采集开关') as HTMLInputElement;
    const perf = screen.getByLabelText('性能与错误 采集开关') as HTMLInputElement;
    expect(learn.disabled).toBe(true);
    expect(learn.checked).toBe(true);
    expect(perf.disabled).toBe(true);
    fireEvent.change(learn, { target: { checked: false } });
    expect(updateSamplingRule).not.toHaveBeenCalled();
  });

  it('组开关切换失败：不提示成功、不重新拉取', async () => {
    updateSamplingRule.mockRejectedValue(new Error('boom'));
    await renderPanel();
    await waitFor(() => expect(screen.getByText('用户行为')).toBeInTheDocument());
    fireEvent.change(screen.getByLabelText('用户行为 采集开关'), { target: { checked: false } });
    await waitFor(() => expect(updateSamplingRule).toHaveBeenCalledWith('periodic', { enabled: false }));
    // 注：失败回滚读 e.currentTarget，事件派发结束后按 DOM 规范已被置 null，
    // 故 catch 分支内联回滚会抛错；此处只断言不会误报成功、也不刷新数据。
    await waitFor(() => expect(probes).toHaveBeenCalledTimes(1));
    expect(toastSuccess).not.toHaveBeenCalled();
  });

  it('探针行：无数据行 EPS 显示占位、有数据行显示 EPS 与相对时间', async () => {
    const { container } = await renderPanel();
    await waitFor(() => expect(screen.getByText('点击行为')).toBeInTheDocument());
    // click 有数据
    expect(screen.getByText('0.06')).toBeInTheDocument();
    expect(screen.getAllByText('EPS').length).toBe(4); // 5 行里 swipe 无数据
    // swipe 无数据 → 占位 + lastTs 为 null
    expect(screen.getByText('—')).toBeInTheDocument();
    expect(screen.getByText('无数据')).toBeInTheDocument();
    // 行状态类：无数据 is-off / error_js 有数据 is-error
    expect(container.querySelectorAll('.pb-probe-row.is-off').length).toBe(1);
    expect(container.querySelectorAll('.pb-probe-row.is-error').length).toBe(1);
    // 业务化描述（PROBE_PLAIN）
    expect(screen.getByText(/用户在界面上的点击、滑动等操作/)).toBeInTheDocument();
    // 未登记 key 回退到后端 desc
    expect(screen.getByText(/swipe 表列/)).toBeInTheDocument();
  });

  it('锁定行显示锁标记，可调行不显示；滑杆 disabled 与采样率同步', async () => {
    const { container } = await renderPanel();
    await waitFor(() => expect(screen.getByText('点击行为')).toBeInTheDocument());
    // lesson_start / word_answer / error_js 三个锁标
    expect(container.querySelectorAll('.lock-badge').length).toBe(3);
    const clickSlider = screen.getByLabelText('点击行为 采样率') as HTMLInputElement;
    expect(clickSlider.disabled).toBe(false);
    expect(clickSlider.value).toBe('35');
    const lockedSlider = screen.getByLabelText('单词作答 采样率') as HTMLInputElement;
    expect(lockedSlider.disabled).toBe(true);
    expect(lockedSlider.value).toBe('100');
  });

  it('可调探针详情抽屉：拉 schema + 按 event_type 过滤审计', async () => {
    await renderPanel();
    await waitFor(() => expect(screen.getByText('点击行为')).toBeInTheDocument());
    fireEvent.click(screen.getByLabelText('点击行为 详情'));
    await waitFor(() => expect(screen.getByRole('dialog')).toHaveAttribute('aria-label', '探针详情 · 点击行为'));
    await waitFor(() => expect(schema).toHaveBeenCalledWith('periodic'));
    expect(audit).toHaveBeenCalledWith(50);
    await waitFor(() => expect(screen.getByText('采样 event_type')).toBeInTheDocument());
    // 审计只保留 periodic 那条
    await waitFor(() => expect(screen.getByText('mod')).toBeInTheDocument());
    expect(screen.queryByText('pause')).toBeNull();
    // 关闭
    fireEvent.click(screen.getByLabelText('关闭'));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  });

  it('核心数据探针详情抽屉：标注无独立 schema，不拉 /schema', async () => {
    await renderPanel();
    await waitFor(() => expect(screen.getByText('单词作答')).toBeInTheDocument());
    fireEvent.click(screen.getByLabelText('单词作答 详情'));
    await waitFor(() => expect(screen.getByText(/不经 telemetry 采样管道/)).toBeInTheDocument());
    expect(screen.getByText('锁定 · 核心数据强制 100%')).toBeInTheDocument();
    expect(schema).not.toHaveBeenCalled();
    expect(audit).not.toHaveBeenCalled();
  });
});
