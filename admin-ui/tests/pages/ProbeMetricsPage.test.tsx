import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import userEvent from '@testing-library/user-event';
import { renderWithProviders } from '../helpers/render';

// ---- mock api client（全方法）----
const overview = vi.fn();
const probes = vi.fn();
const sampling = vi.fn();
const updateSamplingRule = vi.fn();
const sinks = vi.fn();
const schema = vi.fn();
const audit = vi.fn();
const stream = vi.fn();

vi.mock('@/api/probeTelemetry', () => ({
  probeTelemetryApi: {
    overview: (...a: unknown[]) => overview(...a),
    probes: (...a: unknown[]) => probes(...a),
    sampling: (...a: unknown[]) => sampling(...a),
    updateSamplingRule: (...a: unknown[]) => updateSamplingRule(...a),
    sinks: (...a: unknown[]) => sinks(...a),
    schema: (...a: unknown[]) => schema(...a),
    audit: (...a: unknown[]) => audit(...a),
    stream: (...a: unknown[]) => stream(...a),
  },
}));

const toastSuccess = vi.fn();
const toastError = vi.fn();
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: (...a: unknown[]) => toastSuccess(...a), error: (...a: unknown[]) => toastError(...a), warning: vi.fn(), info: vi.fn() } },
}));

// ---- 默认 mock 数据（与 @/types/probeTelemetry 契约对齐）----
const OVERVIEW = {
  generatedAt: '2026-05-29T12:00:00Z',
  activeProbes: { value: 3, total: 4, note: '有数据' },
  events24h: { value: 14700, deltaPct: 0.048, note: '24h' },
  queueBacklog: { value: 12, note: '未完成' },
  collectErrorRate: { value: 0.0003, note: '错误率' },
};

const PROBES = {
  generatedAt: '2026-05-29T12:00:00Z',
  groups: [
    {
      group: 'behavior',
      hue: 213,
      probes: [
        { key: 'click', label: '点击行为', desc: 'telemetry_summaries.click_count', count24h: 5200, eps: 0.06, lastTs: '2026-05-29T11:59:50Z', sampleRate: 0.35, locked: false, enabled: true, samplingEventType: 'periodic' },
      ],
    },
    {
      group: 'learn',
      hue: 162,
      probes: [
        { key: 'lesson_start', label: '开始学习', desc: 'learning_sessions 新建', count24h: 120, eps: 0.0014, lastTs: '2026-05-29T11:58:00Z', sampleRate: 1.0, locked: true, enabled: true },
        { key: 'word_answer', label: '单词作答', desc: 'learning_records 答题', count24h: 3120, eps: 0.036, lastTs: '2026-05-29T11:59:00Z', sampleRate: 1.0, locked: true, enabled: true },
      ],
    },
    {
      group: 'perf',
      hue: 60,
      probes: [
        { key: 'error_js', label: '前端错误', desc: 'telemetry_summaries.error_count', count24h: 7, eps: 0.0001, lastTs: '2026-05-29T11:55:00Z', sampleRate: 1.0, locked: true, enabled: true },
      ],
    },
  ],
};

const SAMPLING = {
  globalDefault: 0.2,
  rules: [
    { eventType: 'on_demand', sampleRate: 1.0, enabled: true, locked: true, priority: 10 },
    { eventType: 'session_start', sampleRate: 1.0, enabled: true, locked: true, priority: 10 },
    { eventType: 'periodic', sampleRate: 0.35, enabled: true, locked: false, priority: 100, target: 'click' },
    { eventType: '*', sampleRate: 0.2, enabled: true, locked: false, priority: 1000 },
  ],
};

const SINKS = {
  generatedAt: '2026-05-29T12:00:00Z',
  sinks: [
    { id: 'telemetry_events', label: '遥测事件', kind: 'sqlite_table', rowCount: 14700, lastWriteTs: '2026-05-29T11:59:50Z', retentionDays: null, lagSecs: 10 },
    { id: 'learning_records', label: '答题记录', kind: 'sqlite_table', rowCount: 3120, lastWriteTs: '2026-05-29T11:59:00Z', retentionDays: null, lagSecs: 60 },
  ],
};

const AUDIT = {
  rows: [
    { ts: '2026-05-29T11:43:08Z', action: 'mod', eventType: 'periodic', oldValue: '{"sampleRate":0.2}', newValue: '{"sampleRate":0.35}', adminId: 'admin-1' },
  ],
};

const STREAM = {
  events: [
    {
      id: 'e1', ts: '2026-05-29T11:59:50Z', type: 'periodic', deviceId: 'dev_abcdef1234',
      device: { os: 'Android 15', model: 'Pixel 6', online: true, language: 'zh-CN' },
      metrics: [{ key: 'actionsPerMin', value: '2' }],
      payloadRaw: '{"actionsPerMin":2}',
    },
    { id: 'e2', ts: '2026-05-29T11:59:40Z', type: 'word_answer', deviceId: 'dev_zzz', metrics: [], payloadRaw: '{"correct":true}' },
  ],
};

function setDefaults() {
  overview.mockResolvedValue(OVERVIEW);
  probes.mockResolvedValue(PROBES);
  sampling.mockResolvedValue(SAMPLING);
  updateSamplingRule.mockResolvedValue({ eventType: 'periodic', sampleRate: 0.5, enabled: true, locked: false, updatedAt: '2026-05-29T12:00:00Z' });
  sinks.mockResolvedValue(SINKS);
  schema.mockResolvedValue({ eventType: 'periodic', sampledAt: '2026-05-29T11:59:50Z', payload: { event: 'periodic', correct: true } });
  audit.mockResolvedValue(AUDIT);
  stream.mockResolvedValue(STREAM);
}

describe('ProbeMetricsPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setDefaults();
    vi.useFakeTimers({ shouldAdvanceTime: true });
  });
  afterEach(() => {
    vi.runOnlyPendingTimers();
    vi.useRealTimers();
  });

  async function renderPage() {
    const { default: Page } = await import('@/pages/ProbeMetricsPage');
    return renderWithProviders(() => <Page />);
  }

  it('renders page header with default 24h window segment active', async () => {
    await renderPage();
    expect(screen.getByRole('heading', { level: 1, name: '数据探针' })).toBeInTheDocument();
    // 时间窗 Seg：24h / 7d / 30d，默认 24h 高亮（active class）
    const seg24h = screen.getByRole('button', { name: '24h' });
    expect(seg24h.className).toContain('active');
    expect(screen.getByRole('button', { name: '7d' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '30d' })).toBeInTheDocument();
  });

  it('renders KPI strip with real derived metrics', async () => {
    await renderPage();
    // KPI 卡片标签（新版派生指标）
    await waitFor(() => expect(screen.getByText('事件/秒')).toBeInTheDocument());
    expect(screen.getByText('采集错误率')).toBeInTheDocument();
    expect(screen.getByText('活跃规则')).toBeInTheDocument();
    expect(screen.getByText('丢弃率')).toBeInTheDocument();
    expect(screen.getByText('Sink 延迟')).toBeInTheDocument();
    // 活跃规则 = 4 条 enabled 且 sampleRate>0
    expect(screen.getByText('4')).toBeInTheDocument();
    // 丢弃率 = (1 - globalDefault 0.2) * 100 = 80.0
    expect(screen.getByText('80.0')).toBeInTheDocument();
    // 采集错误率 0.0003 * 100 = 0.03
    expect(screen.getByText('0.03')).toBeInTheDocument();
  });

  it('renders three probe groups with their probe labels', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByText('用户行为')).toBeInTheDocument());
    expect(screen.getByText('学习事件')).toBeInTheDocument();
    expect(screen.getByText('性能与错误')).toBeInTheDocument();
    // 探针行可读化中文 label
    expect(screen.getByText('点击行为')).toBeInTheDocument();
    expect(screen.getByText('单词作答')).toBeInTheDocument();
    expect(screen.getByText('前端错误')).toBeInTheDocument();
  });

  it('group switch: behavior toggleable (PATCH periodic), learn/perf locked disabled', async () => {
    const { container } = await renderPage();
    await waitFor(() => expect(screen.getByText('用户行为')).toBeInTheDocument());

    // 找到三个采样组卡片头部内的 Switch checkbox。
    // GroupCard 头部 .switch input 顺序：behavior / learn / perf。
    const groupSwitches = (): HTMLInputElement[] => {
      const titles = ['用户行为', '学习事件', '性能与错误'];
      return titles.map((t) => {
        const span = Array.from(container.querySelectorAll('span')).find((s) => s.textContent === t)!;
        // 组卡片 = 标题向上找到带 border 的根；Switch 在同卡片头部内
        const card = span.closest('div[style*="border-radius"]') ?? span.parentElement!.parentElement!.parentElement!;
        return card.querySelector('label.switch input[type="checkbox"]') as HTMLInputElement;
      });
    };
    const [behavior, learn, perf] = groupSwitches();
    expect(behavior.disabled).toBe(false);
    expect(learn.disabled).toBe(true);
    expect(perf.disabled).toBe(true);

    // 切换 behavior 组开关 → PATCH 绑定的 periodic enabled=false
    fireEvent.change(behavior, { target: { checked: false } });
    await waitFor(() => expect(updateSamplingRule).toHaveBeenCalledWith('periodic', { enabled: false }));
    await waitFor(() => expect(toastSuccess).toHaveBeenCalled());
  });

  it('renders sampling cascade rules, sinks and audit trail', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByRole('heading', { name: '采样级联规则' })).toBeInTheDocument());
    expect(screen.getByRole('heading', { name: '写入 Sink' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: '最近改动' })).toBeInTheDocument();
    // sink 保留显示「永久/不限」
    await waitFor(() => expect(screen.getAllByText(/永久\/不限/).length).toBeGreaterThan(0));
    // 审计动作 mod → 中文「修改」（行内动词 + badge）
    await waitFor(() => expect(screen.getAllByText('修改').length).toBeGreaterThan(0));
  });

  it('editing an unlocked sampling rule saves new rate via updateSamplingRule', async () => {
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    await renderPage();
    await waitFor(() => expect(screen.getByRole('heading', { name: '采样级联规则' })).toBeInTheDocument());

    // periodic 规则未锁 → 行内有「编辑」按钮
    const editBtns = await screen.findAllByRole('button', { name: '编辑' });
    await user.click(editBtns[0]);

    // 编辑态出现 number 输入框，aria-label="periodic 采样率"
    const input = await screen.findByLabelText('periodic 采样率') as HTMLInputElement;
    await user.clear(input);
    await user.type(input, '50');

    await user.click(screen.getByRole('button', { name: '保存' }));
    await waitFor(() => expect(updateSamplingRule).toHaveBeenCalledWith('periodic', { sampleRate: 0.5 }));
    await waitFor(() => expect(toastSuccess).toHaveBeenCalled());
  });

  it('renders live event stream humanized and polls every 3s', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByRole('heading', { name: '实时事件流' })).toBeInTheDocument());
    await waitFor(() => expect(stream).toHaveBeenCalledTimes(1));
    // humanize：中文事件类型 + 设备在线 + 指标中文标签
    await waitFor(() => expect(screen.getAllByText('周期上报').length).toBeGreaterThan(0));
    // 设备在线状态与指标中文标签均与前缀/数值同 span，用 regex 匹配
    expect(screen.getByText(/在线/)).toBeInTheDocument();
    expect(screen.getByText(/每分钟操作/)).toBeInTheDocument();
    // 轮询：3 秒后再拉一次
    vi.advanceTimersByTime(3000);
    await waitFor(() => expect(stream).toHaveBeenCalledTimes(2));
  });

  it('pause stops polling, resume immediately refetches', async () => {
    await renderPage();
    await waitFor(() => expect(stream).toHaveBeenCalledTimes(1));
    fireEvent.click(screen.getByRole('button', { name: /暂停/ }));
    vi.advanceTimersByTime(6000);
    // 暂停后轮询不再触发
    expect(stream).toHaveBeenCalledTimes(1);
    // 恢复立即拉一次
    fireEvent.click(screen.getByRole('button', { name: /继续/ }));
    await waitFor(() => expect(stream).toHaveBeenCalledTimes(2));
  });

  it('event card raw toggle expands payload JSON', async () => {
    await renderPage();
    await waitFor(() => expect(stream).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(screen.getAllByText('周期上报').length).toBeGreaterThan(0));
    // 每条事件有「原始」展开按钮
    const rawBtns = screen.getAllByRole('button', { name: '原始' });
    expect(rawBtns.length).toBeGreaterThan(0);
    fireEvent.click(rawBtns[0]);
    // 展开后按钮变「收起」，并渲染 prettified payload
    await waitFor(() => expect(screen.getAllByRole('button', { name: '收起' }).length).toBeGreaterThan(0));
    expect(screen.getByText(/actionsPerMin/)).toBeInTheDocument();
  });

  it('schema drawer opens via 查看 Schema and renders sampled payload', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByRole('heading', { name: '数据样本' })).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: /查看 Schema/ }));
    // 抽屉标题：数据样本 · periodic
    await waitFor(() => expect(screen.getByRole('heading', { name: '数据样本 · periodic' })).toBeInTheDocument());
    await waitFor(() => expect(schema).toHaveBeenCalledWith('periodic'));
  });

  it('schema drawer shows empty state when payload is null', async () => {
    schema.mockResolvedValue({ eventType: 'periodic', sampledAt: null, payload: null });
    await renderPage();
    await waitFor(() => expect(screen.getByRole('heading', { name: '数据样本' })).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: /查看 Schema/ }));
    await waitFor(() => expect(screen.getByText('暂无样本')).toBeInTheDocument());
  });
});
