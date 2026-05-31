import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
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

// ---- 默认 mock 数据 ----
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
    { id: 'e1', ts: '2026-05-29T11:59:50Z', type: 'periodic', deviceId: 'dev_abcdef1234', payloadPreview: '{"x":1}' },
    { id: 'e2', ts: '2026-05-29T11:59:40Z', type: 'word_answer', deviceId: 'dev_zzz', payloadPreview: '{"correct":true}' },
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

  it('renders compact header with default 24h segment', async () => {
    await renderPage();
    expect(screen.getByRole('heading', { name: '数据探针' })).toBeInTheDocument();
    const seg24h = screen.getByRole('button', { name: '24h' });
    expect(seg24h).toHaveAttribute('aria-pressed', 'true');
  });

  it('renders four real KPI cards with real values', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByText('活跃探针')).toBeInTheDocument());
    expect(screen.getByText('24h 上报事件')).toBeInTheDocument();
    expect(screen.getByText('队列积压')).toBeInTheDocument();
    expect(screen.getByText('采集错误率')).toBeInTheDocument();
    // 活跃探针 3 / 4
    expect(screen.getByText('3')).toBeInTheDocument();
    expect(screen.getByText('/ 4')).toBeInTheDocument();
    // 24h 事件缩写 14.7k
    expect(screen.getByText('14.7k')).toBeInTheDocument();
  });

  it('renders three probe groups', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByText('用户行为')).toBeInTheDocument());
    expect(screen.getByText('学习事件')).toBeInTheDocument();
    expect(screen.getByText('性能与错误')).toBeInTheDocument();
    // 探针行名称（探针组内 strong；word_answer 在 stream type 也出现，用 getAllByText 容忍）
    expect(screen.getByText('click')).toBeInTheDocument();
    expect(screen.getAllByText('word_answer').length).toBeGreaterThan(0);
  });

  it('locked probe slider is disabled, adjustable slider is enabled', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByLabelText('点击行为 采样率')).toBeInTheDocument());
    const clickSlider = screen.getByLabelText('点击行为 采样率') as HTMLInputElement;
    expect(clickSlider.disabled).toBe(false);
    const lockedSlider = screen.getByLabelText('单词作答 采样率') as HTMLInputElement;
    expect(lockedSlider.disabled).toBe(true);
  });

  it('dragging adjustable slider debounces then calls updateSamplingRule + toast', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByLabelText('点击行为 采样率')).toBeInTheDocument());
    const clickSlider = screen.getByLabelText('点击行为 采样率') as HTMLInputElement;
    fireEvent.input(clickSlider, { target: { value: '50' } });
    // debounce 未触发前不调用
    expect(updateSamplingRule).not.toHaveBeenCalled();
    vi.advanceTimersByTime(350);
    expect(updateSamplingRule).toHaveBeenCalledWith('periodic', { sampleRate: 0.5 });
    await waitFor(() => expect(toastSuccess).toHaveBeenCalled());
  });

  it('renders sampling rules, sinks, audit', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByRole('heading', { name: '采样级联规则' })).toBeInTheDocument());
    // sink 显示永久/不限 retention
    expect(screen.getAllByText(/永久\/不限/).length).toBeGreaterThan(0);
    // 审计行 eventType
    await waitFor(() => expect(screen.getByText('MODIFY')).toBeInTheDocument());
  });

  it('renders stream events and polls every 5s', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByText('实时事件流')).toBeInTheDocument());
    await waitFor(() => expect(stream).toHaveBeenCalledTimes(1));
    // payloadPreview 渲染
    await waitFor(() => expect(screen.getByText('{"correct":true}')).toBeInTheDocument());
    // 轮询
    vi.advanceTimersByTime(5000);
    await waitFor(() => expect(stream).toHaveBeenCalledTimes(2));
  });

  it('schema empty state shows Empty when payload null', async () => {
    schema.mockResolvedValue({ eventType: 'periodic', sampledAt: null, payload: null });
    await renderPage();
    await waitFor(() => expect(screen.getByText('暂无样本')).toBeInTheDocument());
  });

  it('stream Pause button stops polling, Resume resumes (真实暂停轮询)', async () => {
    await renderPage();
    await waitFor(() => expect(stream).toHaveBeenCalledTimes(1));
    fireEvent.click(screen.getByRole('button', { name: /Pause/ }));
    vi.advanceTimersByTime(6000);
    // 暂停后轮询不再触发
    expect(stream).toHaveBeenCalledTimes(1);
    // 恢复立即拉一次
    fireEvent.click(screen.getByRole('button', { name: /Resume/ }));
    await waitFor(() => expect(stream).toHaveBeenCalledTimes(2));
  });

  it('row kebab opens probe detail drawer (接真实 /schema)', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByLabelText('点击行为 详情')).toBeInTheDocument());
    fireEvent.click(screen.getByLabelText('点击行为 详情'));
    await waitFor(() => expect(screen.getByText('探针详情 · 点击行为')).toBeInTheDocument());
    // 抽屉展示元数据 + 绑定的 telemetry event_type
    expect(screen.getByText('采样 event_type')).toBeInTheDocument();
  });

  it('group enable toggle: behavior 可切真实生效, learn/perf 锁定 disabled', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByLabelText('用户行为 采集开关')).toBeInTheDocument());
    const behavior = screen.getByLabelText('用户行为 采集开关') as HTMLInputElement;
    const learn = screen.getByLabelText('学习事件 采集开关') as HTMLInputElement;
    const perf = screen.getByLabelText('性能与错误 采集开关') as HTMLInputElement;
    expect(behavior.disabled).toBe(false);
    expect(learn.disabled).toBe(true);
    expect(perf.disabled).toBe(true);
    // 切换 behavior → PATCH periodic enabled
    fireEvent.click(behavior);
    await waitFor(() => expect(updateSamplingRule).toHaveBeenCalledWith('periodic', { enabled: false }));
  });
});
