import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from './helpers/render';

// ─────────────────────────────────────────────────────────────────────────
// EChart 桩：执行 props.option() 构建器（覆盖那些只在渲染图表时才跑的
// option() 闭包），把 series 数量写进 data 属性，不触碰真实 echarts canvas。
// ─────────────────────────────────────────────────────────────────────────
vi.mock('@/components/ui/EChart', () => ({
  EChart: (props: { option: () => { series?: unknown } }) => {
    let seriesLen = -1;
    try {
      const opt = props.option();
      const s = opt?.series;
      seriesLen = Array.isArray(s) ? s.length : s ? 1 : 0;
    } catch {
      seriesLen = -2;
    }
    return <div data-testid="chart" data-series-len={String(seriesLen)} />;
  },
}));

// JsonAdvancedPanel 依赖的 CodeMirror 重组件 stub 成占位，避免拉真实编辑器。
vi.mock('@/components/amas/TomlEditor', () => ({
  default: (props: { value: string; onChange?: (v: string) => void }) => (
    <textarea
      data-testid="toml-editor"
      value={props.value}
      onInput={(e) => props.onChange?.((e.currentTarget as HTMLTextAreaElement).value)}
    />
  ),
}));
vi.mock('@/components/amas/ConfigTree', () => ({
  ConfigTree: (props: { onSectionClick?: (s: string) => void }) => (
    <div data-testid="config-tree" onClick={() => props.onSectionClick?.('ensemble')} />
  ),
}));
vi.mock('@codemirror/view', () => ({ EditorView: { scrollIntoView: () => ({}) } }));

vi.mock('@/api/admin', () => ({
  adminApi: {
    amasDecisionHistogram: vi.fn(),
    amasEloScatter: vi.fn(),
    amasFatigueTimeseries: vi.fn(),
    amasMdmHeatmap: vi.fn(),
    amasAlgorithmDistribution: vi.fn(),
    amasStageDistribution: vi.fn(),
    amasStateTransitions: vi.fn(),
    amasLearningClusters: vi.fn(),
    amasGetCanary: vi.fn(),
    amasListVersions: vi.fn(),
    amasSetCanaryExt: vi.fn(),
    amasDisableCanary: vi.fn(),
    amasConfigDiffImpact: vi.fn(),
    amasSerializeToml: vi.fn(),
    amasParseToml: vi.fn(),
  },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

// 给所有 api 方法一个安全默认 Promise，避免未显式 mock 的调用 reject。
function defaultApis() {
  for (const k of Object.keys(mockApi)) mockApi[k].mockResolvedValue(null as never);
}

// ─────────────────────────────── DecisionHistogramPanel ───────────────────────────────
describe('DecisionHistogramPanel — option() + 空/有数据', () => {
  beforeEach(() => { vi.clearAllMocks(); defaultApis(); });

  async function render(days = 7) {
    const { DecisionHistogramPanel } = await import('@/pages/amas/DecisionHistogramPanel');
    return renderWithProviders(() => <DecisionHistogramPanel days={() => days} />);
  }

  it('有桶数据时构建 1 个 bar series 并显示 P50/P95', async () => {
    mockApi.amasDecisionHistogram.mockResolvedValue({
      buckets: [{ label: '1-5', count: 12 }, { label: '6-10', count: 7 }],
      totalUsers: 19, p50: 4, p95: 9,
    });
    await render(14);
    const chart = await screen.findByTestId('chart');
    expect(chart.getAttribute('data-series-len')).toBe('1');
    expect(screen.getByText('4 题 / 窗口')).toBeInTheDocument();
    expect(screen.getByText('9 题 / 窗口')).toBeInTheDocument();
    expect(screen.getByText(/14d/)).toBeInTheDocument();
  });

  it('totalUsers=0 走空态 暂无答题记录', async () => {
    mockApi.amasDecisionHistogram.mockResolvedValue({ buckets: [], totalUsers: 0, p50: 0, p95: 0 });
    await render();
    await waitFor(() => expect(screen.getByText('暂无答题记录')).toBeInTheDocument());
    expect(screen.queryByTestId('chart')).not.toBeInTheDocument();
  });
});

// ─────────────────────────────── FatigueTimeseriesPanel ───────────────────────────────
describe('FatigueTimeseriesPanel — 2 series + 空态', () => {
  beforeEach(() => { vi.clearAllMocks(); defaultApis(); });

  async function render(days = 7) {
    const { FatigueTimeseriesPanel } = await import('@/pages/amas/FatigueTimeseriesPanel');
    return renderWithProviders(() => <FatigueTimeseriesPanel days={() => days} />);
  }

  it('有 points 构建平均强度+峰值 2 series 并显示阈值/触发统计', async () => {
    mockApi.amasFatigueTimeseries.mockResolvedValue({
      points: [
        { date: '2026-05-01', avgFatigue: 0.321, peakFatigue: 0.62, triggerCount: 3 },
        { date: '2026-05-02', avgFatigue: 0.28, peakFatigue: 0.55, triggerCount: 1 },
      ],
      avgIntensity: 0.3, totalTriggers: 4, threshold: 0.5,
    });
    await render();
    const chart = await screen.findByTestId('chart');
    expect(chart.getAttribute('data-series-len')).toBe('2');
    expect(screen.getByText('0.30')).toBeInTheDocument();
    expect(screen.getByText('查看 patch →')).toBeInTheDocument();
  });

  it('空 points 走空态 暂无疲劳采样', async () => {
    mockApi.amasFatigueTimeseries.mockResolvedValue({ points: [], avgIntensity: 0, totalTriggers: 0, threshold: 0 });
    await render();
    await waitFor(() => expect(screen.getByText('暂无疲劳采样')).toBeInTheDocument());
  });
});

// ─────────────────────────────── EloScatterPanel ───────────────────────────────
describe('EloScatterPanel — 散点 dot 颜色分支 + 空态', () => {
  beforeEach(() => { vi.clearAllMocks(); defaultApis(); });

  async function render() {
    const { EloScatterPanel } = await import('@/pages/amas/EloScatterPanel');
    return renderWithProviders(() => <EloScatterPanel />);
  }

  it('多点命中 dotColor 三分支（>20/±20/<-20）并渲染 dot', async () => {
    mockApi.amasEloScatter.mockResolvedValue({
      points: [
        { elo: 900, decisions: 10, deltaElo: 50 },   // success
        { elo: 1500, decisions: 200, deltaElo: 0 },  // accent
        { elo: 2100, decisions: 9000, deltaElo: -40 }, // error
      ],
      total: 3, meanElo: 1500,
    });
    const { container } = await render();
    await waitFor(() => expect(container.querySelectorAll('.dot').length).toBe(3));
    expect(screen.getByText(/均值 ELO 1500/)).toBeInTheDocument();
  });

  it('total=0 走空态 暂无 ELO 数据', async () => {
    mockApi.amasEloScatter.mockResolvedValue({ points: [], total: 0, meanElo: 0 });
    await render();
    await waitFor(() => expect(screen.getByText('暂无 ELO 数据')).toBeInTheDocument());
  });
});

// ─────────────────────────────── MdmHeatmapPanel ───────────────────────────────
describe('MdmHeatmapPanel — 热图 cell level 分支 + 空态', () => {
  beforeEach(() => { vi.clearAllMocks(); defaultApis(); });

  async function render(days = 7) {
    const { MdmHeatmapPanel } = await import('@/pages/amas/MdmHeatmapPanel');
    return renderWithProviders(() => <MdmHeatmapPanel days={() => days} />);
  }

  it('有 days/cells 渲染热格（含 -1 空白 + 满热度），显示峰值', async () => {
    mockApi.amasMdmHeatmap.mockResolvedValue({
      days: ['2026-05-01', '2026-05-02'],
      bandCount: 3,
      cells: [
        [-1, 0, 0.5],   // -1 → 空白 ; 0 → level0 ; 0.5 → level3
        [0.99, 1, -1],  // ≈满热度
      ],
      peak: 0.99,
    });
    const { container } = await render(7);
    await waitFor(() => expect(container.querySelectorAll('.heat-grid .h').length).toBe(6));
    expect(screen.getByText('0.99')).toBeInTheDocument();
    expect(screen.getByText(/7d × 14 难度段/)).toBeInTheDocument();
  });

  it('days 为空走空态 暂无 MDM 状态', async () => {
    mockApi.amasMdmHeatmap.mockResolvedValue({ days: [], bandCount: 0, cells: [], peak: 0 });
    await render();
    await waitFor(() => expect(screen.getByText('暂无 MDM 状态')).toBeInTheDocument());
  });
});

// ─────────────────────────────── AlgorithmDonut ───────────────────────────────
describe('AlgorithmDonut — segments/total/topName + 空态', () => {
  beforeEach(() => { vi.clearAllMocks(); defaultApis(); });

  async function render(days = 7) {
    const { AlgorithmDonut } = await import('@/pages/amas/AlgorithmDonut');
    return renderWithProviders(() => <AlgorithmDonut days={() => days} />);
  }

  it('有分布渲染 donut + legend + fmtShort（K/M 缩写）', async () => {
    mockApi.amasAlgorithmDistribution.mockResolvedValue([
      { algorithm: 'mdm', pct: 0.5, count: 1_200_000 },
      { algorithm: 'ensemble', pct: 0.3, count: 3000 },
      { algorithm: 'swd', pct: 0.2, count: 500 },
    ]);
    const { container } = await render(30);
    await waitFor(() => expect(container.querySelector('.donut')).toBeTruthy());
    // segments：背景圈 1 + 3 段
    expect(container.querySelectorAll('.donut circle').length).toBe(4);
    expect(container.querySelectorAll('.legend .row-l').length).toBe(3);
    // 1,203,500 → "1.2M"
    expect(screen.getAllByText('1.2M').length).toBeGreaterThan(0);
    expect(screen.getByText(/30d decisions/)).toBeInTheDocument();
    // topName = MDM（algoMeta label）
    expect(screen.getByText(/MDM 主导/)).toBeInTheDocument();
  });

  it('空分布走空态 暂无决策路由数据 + topName 为 —', async () => {
    mockApi.amasAlgorithmDistribution.mockResolvedValue([]);
    await render();
    await waitFor(() => expect(screen.getByText('暂无决策路由数据')).toBeInTheDocument());
    expect(screen.getByText(/— 主导/)).toBeInTheDocument();
  });
});

// ─────────────────────────────── UserStatePanel ───────────────────────────────
describe('UserStatePanel — 4 resource 全态', () => {
  beforeEach(() => { vi.clearAllMocks(); defaultApis(); });

  async function render() {
    const { UserStatePanel } = await import('@/pages/amas/UserStatePanel');
    return renderWithProviders(() => <UserStatePanel />);
  }

  it('全有数据：3 阶段卡 + 直方图 + 流转 + 聚类', async () => {
    mockApi.amasStageDistribution.mockResolvedValue({
      totalUsers: 100,
      stages: [
        { stage: 'cold', users: 30, pct: 0.3, avgDecisions: 5, retention7d: 0.4, mainRoute: 'heuristic' },
        { stage: 'transition', users: 40, pct: 0.4, avgDecisions: 80, retention7d: 0.55, mainRoute: 'mdm' },
        { stage: 'stable', users: 30, pct: 0.3, avgDecisions: 300, retention7d: 0.7, mainRoute: '' },
      ],
      trend: [],
    });
    mockApi.amasDecisionHistogram.mockResolvedValue({
      buckets: [{ label: '1-5', count: 10 }, { label: '6+', count: 4 }],
      totalUsers: 14, p50: 4, p95: 9,
    });
    mockApi.amasStateTransitions.mockResolvedValue({
      transitions: [
        { from: 'cold', to: 'transition', count: 5 },
        { from: 'new', to: 'cold', count: 2 },
        { from: 'foo', to: 'bar', count: 1 }, // 命中 FLOW_META 兜底
      ],
    });
    mockApi.amasLearningClusters.mockResolvedValue({
      clusters: [
        { label: '快准型', count: 20, pct: 0.5, avgResponseMs: 1200, errorRate: 0.1, recordsPerActiveDay: 12 },
        { label: '慢稳型', count: 20, pct: 0.5, avgResponseMs: 3000, errorRate: 0.3, recordsPerActiveDay: 4 },
      ],
    });
    const { container } = await render();
    await waitFor(() => expect(container.querySelectorAll('.us-stage').length).toBe(3));
    expect(screen.getByText('冷启动')).toBeInTheDocument();
    expect(screen.getByText('过渡期')).toBeInTheDocument();
    expect(screen.getByText('稳定阶段')).toBeInTheDocument();
    // 直方图列
    await waitFor(() => expect(container.querySelectorAll('.us-hist .col').length).toBe(2));
    // 流转 3 行（含兜底）
    await waitFor(() => expect(container.querySelectorAll('.flow-row').length).toBe(3));
    expect(screen.getByText('冷 → 过渡')).toBeInTheDocument();
    expect(screen.getByText('foo → bar')).toBeInTheDocument();
    // 聚类 2 行
    await waitFor(() => expect(container.querySelectorAll('.cluster-row').length).toBe(2));
    expect(screen.getByText('快准型')).toBeInTheDocument();
  });

  it('全空：阶段空态 + 直方图空态 + 流转空态 + 聚类空态', async () => {
    mockApi.amasStageDistribution.mockResolvedValue({ totalUsers: 0, stages: [], trend: [] });
    mockApi.amasDecisionHistogram.mockResolvedValue({ buckets: [], totalUsers: 0, p50: 0, p95: 0 });
    mockApi.amasStateTransitions.mockResolvedValue({ transitions: [] });
    mockApi.amasLearningClusters.mockResolvedValue({ clusters: [] });
    await render();
    await waitFor(() => expect(screen.getByText('暂无用户状态')).toBeInTheDocument());
    expect(screen.getByText('暂无答题记录')).toBeInTheDocument();
    expect(screen.getByText('24h 内无状态切换')).toBeInTheDocument();
    expect(screen.getByText('样本不足')).toBeInTheDocument();
  });
});

// ─────────────────────────────── PatchCanaryCard ───────────────────────────────
describe('PatchCanaryCard — 状态/扩量/回滚/提升交互', () => {
  beforeEach(() => { vi.clearAllMocks(); defaultApis(); });

  const base = {
    id: 1, suggestionId: 42, versionHash: 'abcdef0123456789', cohortLo: 0, cohortHi: 30,
    baselineMetricsJson: '{}', startedAt: '2026-05-01T00:00:00Z', updatedAt: '2026-05-01T00:00:00Z',
    liveReward: 0.82, liveAnomalyRate: 0.012, baselineReward: 0.75,
  };

  async function render(c: Record<string, unknown>, handlers: Record<string, ReturnType<typeof vi.fn>> = {}) {
    const { PatchCanaryCard } = await import('@/pages/amas-advisor/PatchCanaryCard');
    return renderWithProviders(() => (
      <PatchCanaryCard
        c={c as never}
        steps={[10, 30, 50]}
        busy={false}
        onScale={handlers.onScale ?? vi.fn()}
        onRollback={handlers.onRollback ?? vi.fn()}
        onPromote={handlers.onPromote ?? vi.fn()}
      />
    ));
  }

  it('active 且 percent<100：渲染回滚/扩量/直接100% 按钮，点击触发回调', async () => {
    const onScale = vi.fn(); const onRollback = vi.fn();
    await render({ ...base, percent: 20, status: 'active' }, { onScale, onRollback });
    expect(screen.getByText('灰度中')).toBeInTheDocument();
    expect(screen.getByText('灰度 20%')).toBeInTheDocument();
    // reward delta = 0.82-0.75 = +0.07
    expect(screen.getByText('+0.07')).toBeInTheDocument();
    fireEvent.click(screen.getByText('回滚'));
    expect(onRollback).toHaveBeenCalled();
    // nextStep = 30（首个 > 20）
    fireEvent.click(screen.getByText('扩量到 30%'));
    expect(onScale).toHaveBeenCalledWith(30);
    fireEvent.click(screen.getByText('直接 100% 生效'));
    expect(onScale).toHaveBeenCalledWith(100);
  });

  it('active 且 percent>=100：渲染提升为 stable 按钮', async () => {
    const onPromote = vi.fn();
    await render({ ...base, percent: 100, status: 'active', liveReward: 0.7, baselineReward: 0.75 }, { onPromote });
    // reward delta 负 → -0.05
    expect(screen.getByText('-0.05')).toBeInTheDocument();
    expect(screen.queryByText('回滚')).toBeInTheDocument();
    fireEvent.click(screen.getByText('提升为 stable'));
    expect(onPromote).toHaveBeenCalled();
    expect(screen.queryByText(/扩量到/)).not.toBeInTheDocument();
  });

  it('rolled_back 状态：不渲染操作 footer', async () => {
    await render({ ...base, percent: 30, status: 'rolled_back' });
    expect(screen.getByText('已回滚')).toBeInTheDocument();
    expect(screen.queryByText('回滚')).not.toBeInTheDocument();
  });

  it('effective 状态：标签 已生效', async () => {
    await render({ ...base, percent: 100, status: 'effective' });
    expect(screen.getByText('已生效')).toBeInTheDocument();
  });
});

// ─────────────────────────────── InlineVersionList ───────────────────────────────
describe('InlineVersionList — 列表/空/相对时间/source 标签', () => {
  beforeEach(() => { vi.clearAllMocks(); defaultApis(); });

  async function render(onOpen = vi.fn()) {
    const { InlineVersionList } = await import('@/components/amas/InlineVersionList');
    return renderWithProviders(() => <InlineVersionList onOpenFullList={onOpen} />);
  }

  it('有版本：渲染 hash / 备注 / source 标签 + header 跳转回调', async () => {
    const now = Date.now();
    mockApi.amasListVersions.mockResolvedValue([
      { versionHash: 'aabbccddeeff', source: 'manual', createdAt: new Date(now - 30_000).toISOString(), note: '调权重', authorAdminId: 1 },
      { versionHash: '112233445566', source: 'llm_auto', createdAt: new Date(now - 7200_000).toISOString(), note: null, authorAdminId: 1 },
    ]);
    const onOpen = vi.fn();
    await render(onOpen);
    await waitFor(() => expect(screen.getByText(/vaabbcc/)).toBeInTheDocument());
    expect(screen.getByText('刚刚')).toBeInTheDocument(); // <60s
    expect(screen.getByText('调权重')).toBeInTheDocument();
    expect(screen.getByText('(无备注)')).toBeInTheDocument(); // note=null fallback
    expect(screen.getByText(/LLM 自动/)).toBeInTheDocument();
    fireEvent.click(screen.getByText('最近 10 次 →'));
    expect(onOpen).toHaveBeenCalled();
  });

  it('空列表走 尚无历史版本', async () => {
    mockApi.amasListVersions.mockResolvedValue([]);
    await render();
    await waitFor(() => expect(screen.getByText('尚无历史版本')).toBeInTheDocument());
  });
});

// ─────────────────────────────── DiffSummary ───────────────────────────────
describe('DiffSummary — 无改动/有改动/影响估算', () => {
  beforeEach(() => { vi.clearAllMocks(); defaultApis(); });

  async function render(baseline: Record<string, unknown>, config: Record<string, unknown>, errors: unknown[] = []) {
    const { DiffSummary } = await import('@/components/amas/DiffSummary');
    return renderWithProviders(() => (
      <DiffSummary baseline={baseline} config={config} errors={errors as never} />
    ));
  }

  it('baseline==config：显示 0 处改动 + 无修改文案', async () => {
    await render({ featureFlags: { ensembleEnabled: true } }, { featureFlags: { ensembleEnabled: true } });
    await waitFor(() => expect(screen.getByText('0 处改动')).toBeInTheDocument());
    expect(screen.getByText(/无修改/)).toBeInTheDocument();
  });

  it('有 1 处改动：渲染差异行 + 验证通过 badge + diff-impact 估算', async () => {
    mockApi.amasConfigDiffImpact.mockResolvedValue({
      fields: [
        { path: 'featureFlags.ensembleEnabled', from: true, to: false, relChange: null, inWhitelist: true,
          impacts: [{ metric: 'accuracy', deltaLowPt: 0.4, deltaHighPt: 1.2, direction: 'up' }],
          confidence: 'medium' },
      ],
      telemetrySampleSize: 1234, confidence: 'medium', method: 'sandbox',
    });
    await render(
      { featureFlags: { ensembleEnabled: true } },
      { featureFlags: { ensembleEnabled: false } },
    );
    await waitFor(() => expect(screen.getByText('1 处改动')).toBeInTheDocument());
    expect(screen.getByText('验证通过')).toBeInTheDocument();
    // path 出现在差异表
    expect(screen.getByText('featureFlags.ensembleEnabled')).toBeInTheDocument();
    // diff-impact 估算文案：准确率 +0.4~1.2pt
    await waitFor(() => expect(screen.getByText(/准确率 \+0\.4~1\.2pt/)).toBeInTheDocument());
    expect(screen.getByText(/估算样本 1,234/)).toBeInTheDocument();
  });

  it('有改动且有错误：badge 显示错误数，影响列回退 sensitivity 文案', async () => {
    // impact 返回空 fields → 命中 fallback 文案分支
    mockApi.amasConfigDiffImpact.mockResolvedValue({
      fields: [], telemetrySampleSize: 0, confidence: 'low', method: 'sandbox',
    });
    await render(
      { featureFlags: { ensembleEnabled: true } },
      { featureFlags: { ensembleEnabled: false } },
      [{ path: 'featureFlags.ensembleEnabled', message: '非法' }],
    );
    await waitFor(() => expect(screen.getByText('1 处改动')).toBeInTheDocument());
    expect(screen.getByText('1 错')).toBeInTheDocument();
  });
});

// ─────────────────────────────── CanaryCard ───────────────────────────────
describe('CanaryCard — active/空 + 设置交互', () => {
  beforeEach(() => { vi.clearAllMocks(); defaultApis(); });

  async function render() {
    const { CanaryCard } = await import('@/pages/amas/CanaryCard');
    return renderWithProviders(() => <CanaryCard />);
  }

  it('无 active canary：显示 stable 兜底，版本下拉填充', async () => {
    mockApi.amasGetCanary.mockResolvedValue({ canary: null });
    mockApi.amasListVersions.mockResolvedValue([
      { versionHash: 'aabbccddeeff', source: 'manual', createdAt: '2026-05-01T00:00:00Z', note: 'x', authorAdminId: 1 },
    ]);
    await render();
    await waitFor(() => expect(screen.getByText(/所有用户走 stable 通道/)).toBeInTheDocument());
    // 设置区出现
    expect(screen.getByText('设置新灰度')).toBeInTheDocument();
    expect(screen.getByLabelText('灰度比例')).toBeInTheDocument();
  });

  it('有 active canary：显示生效中 badge + 目标版本 + 停用按钮', async () => {
    mockApi.amasGetCanary.mockResolvedValue({
      canary: {
        id: 1, versionHash: 'deadbeefcafebabe', percent: 25,
        forceUserIds: ['u1', 'u2'], createdAt: '2026-05-01T00:00:00Z', createdBy: 'admin',
      },
    });
    mockApi.amasListVersions.mockResolvedValue([]);
    await render();
    await waitFor(() => expect(screen.getByText(/生效中 · 25%/)).toBeInTheDocument());
    expect(screen.getByText('停用灰度')).toBeInTheDocument();
    // 目标版本 hash 前 12 位
    expect(screen.getByText('deadbeefcafe')).toBeInTheDocument();
  });

  it('未选版本点启用 → toast.error 请选择目标版本', async () => {
    mockApi.amasGetCanary.mockResolvedValue({ canary: null });
    mockApi.amasListVersions.mockResolvedValue([]);
    const { uiStore } = await import('@/stores/ui');
    await render();
    await waitFor(() => expect(screen.getByText('启用 / 替换灰度')).toBeInTheDocument());
    fireEvent.click(screen.getByText('启用 / 替换灰度'));
    expect((uiStore.toast.error as ReturnType<typeof vi.fn>)).toHaveBeenCalledWith('请选择目标版本');
  });
});

// ─────────────────────────────── JsonAdvancedPanel ───────────────────────────────
describe('JsonAdvancedPanel — 三栏布局 + TOML 序列化/编辑/应用', () => {
  beforeEach(() => { vi.clearAllMocks(); defaultApis(); });

  async function render(props: Partial<Record<string, unknown>> = {}) {
    const { JsonAdvancedPanel } = await import('@/pages/amas/JsonAdvancedPanel');
    return renderWithProviders(() => (
      <JsonAdvancedPanel
        config={(props.config as Record<string, unknown>) ?? { a: 1 }}
        baseline={(props.baseline as Record<string, unknown>) ?? { a: 1 }}
        errors={(props.errors as never) ?? []}
        onChange={(props.onChange as (n: Record<string, unknown>) => void) ?? (() => {})}
        onOpenVersionDrawer={(props.onOpenVersionDrawer as () => void) ?? (() => {})}
      />
    ));
  }

  it('mount 后序列化 config → TOML 写入编辑器，工具栏显示行数', async () => {
    mockApi.amasSerializeToml.mockResolvedValue({ toml: 'a = 1\nb = 2\n' });
    mockApi.amasGetCanary.mockResolvedValue({ canary: null });
    mockApi.amasListVersions.mockResolvedValue([]);
    const { container } = await render();
    await waitFor(() => expect(mockApi.amasSerializeToml).toHaveBeenCalled());
    const editor = await screen.findByTestId('toml-editor');
    await waitFor(() => expect((editor as HTMLTextAreaElement).value).toContain('a = 1'));
    // 三栏：tree + editor + 右栏 canary/version
    expect(container.querySelector('[data-testid="config-tree"]')).toBeTruthy();
    expect(screen.getByText(/amas_config.toml/)).toBeInTheDocument();
  });

  it('编辑后变 dirty：显示 已编辑 badge + 应用按钮可用；应用调用 parseToml + onChange', async () => {
    mockApi.amasSerializeToml.mockResolvedValue({ toml: 'a = 1\n' });
    mockApi.amasParseToml.mockResolvedValue({ a: 2 });
    mockApi.amasGetCanary.mockResolvedValue({ canary: null });
    mockApi.amasListVersions.mockResolvedValue([]);
    const onChange = vi.fn();
    await render({ onChange });
    const editor = await screen.findByTestId('toml-editor');
    await waitFor(() => expect((editor as HTMLTextAreaElement).value).toContain('a = 1'));
    fireEvent.input(editor, { target: { value: 'a = 2\n' } });
    await waitFor(() => expect(screen.getByText('已编辑')).toBeInTheDocument());
    fireEvent.click(screen.getByText('应用 ⌘S'));
    await waitFor(() => expect(mockApi.amasParseToml).toHaveBeenCalledWith('a = 2\n'));
    await waitFor(() => expect(onChange).toHaveBeenCalledWith({ a: 2 }));
  });

  it('应用解析失败：显示错误 alert', async () => {
    mockApi.amasSerializeToml.mockResolvedValue({ toml: 'a = 1\n' });
    mockApi.amasParseToml.mockRejectedValue(new Error('TOML 语法错误'));
    mockApi.amasGetCanary.mockResolvedValue({ canary: null });
    mockApi.amasListVersions.mockResolvedValue([]);
    await render();
    const editor = await screen.findByTestId('toml-editor');
    await waitFor(() => expect((editor as HTMLTextAreaElement).value).toContain('a = 1'));
    fireEvent.input(editor, { target: { value: 'bad toml' } });
    await waitFor(() => expect(screen.getByText('应用 ⌘S')).toBeInTheDocument());
    fireEvent.click(screen.getByText('应用 ⌘S'));
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('TOML 语法错误'));
  });
});
