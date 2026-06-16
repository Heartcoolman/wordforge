import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: {
    // 本地词库管理(/api/admin/wordbooks/*)
    adminWordbooksList: vi.fn(),
    adminWordbookStats: vi.fn(),
    adminWordbookWords: vi.fn(),
    adminWordbookHeatmap: vi.fn(),
    adminWordbookDistribution: vi.fn(),
    adminWordbookHistory: vi.fn(),
    adminWordbookCreate: vi.fn(),
    adminWordbookUpdate: vi.fn(),
    adminWordbookDelete: vi.fn(),
    adminWordbookAddWord: vi.fn(),
    adminWordbookRemoveWord: vi.fn(),
    adminWordbookExport: vi.fn(),
    // 远程目录面板(/api/admin/wordbook-center/*),默认折叠不渲染
    getSettings: vi.fn(),
    wbCenterBrowse: vi.fn(),
    wbCenterUpdates: vi.fn(),
    wbCenterImport: vi.fn(),
    wbCenterSync: vi.fn(),
    wbCenterPreview: vi.fn(),
    wbCenterUpload: vi.fn(),
  },
}));

vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
import { uiStore } from '@/stores/ui';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockToast = uiStore.toast as unknown as Record<string, ReturnType<typeof vi.fn>>;

const sysWb = {
  id: 'wb-sys', name: 'CET-6 高频', description: '考试核心词', type: 'system' as const,
  wordCount: 2184, activeUsers: 312, tags: ['考试', 'CET'], createdAt: '2026-01-01T00:00:00Z',
};
const userWb = {
  id: 'wb-user', name: '我的生词本', description: '', type: 'user' as const,
  userId: 'u-1', ownerEmail: 'a@b.com', wordCount: 42, activeUsers: 1, tags: [],
  createdAt: '2026-05-01T00:00:00Z',
};

function listResponse(items = [sysWb, userWb]) {
  return {
    items,
    total: items.length,
    page: 1,
    perPage: 200,
    totalPages: 1,
    counts: { all: items.length, system: 1, user: 1, totalEntries: 2226 },
  };
}

const statsResp = { wordbookId: 'wb-sys', totalWords: 2184, activeUsers: 312, avgMastery: 0.63, weeklyAnswers: 9001 };
const wordsResp = {
  data: [
    { id: 'w-1', text: 'tenacious', pronunciation: '/tɪˈneɪʃəs/', partOfSpeech: 'adj.', meaning: '坚韧的', examples: ['She was tenacious.'], appearCount: 3142, accuracy: 0.62 },
    { id: 'w-2', text: 'acquiesce', pronunciation: '/ˌækwiˈɛs/', partOfSpeech: 'v.', meaning: '默许', examples: [], appearCount: 2612, accuracy: 0.41 },
    { id: 'w-3', text: 'novel', meaning: '新颖的', examples: [], appearCount: 10, accuracy: null },
  ],
  total: 3, page: 1, perPage: 30, totalPages: 1,
};

async function renderPage() {
  const { default: Page } = await import('@/pages/WordbookCenterPage');
  return renderWithProviders(() => <Page />);
}

describe('WordbookCenterPage — 主从布局 + 词条/统计', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockApi.adminWordbookStats.mockResolvedValue(statsResp);
    mockApi.adminWordbookWords.mockResolvedValue(wordsResp);
  });

  it('左列表渲染词库并显示全部计数', async () => {
    mockApi.adminWordbooksList.mockResolvedValue(listResponse());
    await renderPage();
    // 自动选中第一项后名称同时出现在列表与详情,故用 getAllByText
    await waitFor(() => expect(screen.getAllByText('CET-6 高频').length).toBeGreaterThan(0));
    expect(screen.getByText('我的生词本')).toBeInTheDocument();
    // 左栏头部「全部词库 <all>」
    expect(screen.getByText('全部词库')).toBeInTheDocument();
    expect(screen.getByText('2')).toBeInTheDocument();
  });

  it('列表加载后自动选中第一项并拉取 stats + words', async () => {
    mockApi.adminWordbooksList.mockResolvedValue(listResponse());
    await renderPage();
    await waitFor(() => expect(mockApi.adminWordbookStats).toHaveBeenCalledWith('wb-sys'));
    await waitFor(() =>
      expect(mockApi.adminWordbookWords).toHaveBeenCalledWith('wb-sys', expect.objectContaining({ sort: 'frequency' })),
    );
    await waitFor(() => expect(screen.getByText('tenacious')).toBeInTheDocument());
    // KPI 卡:平均掌握 0.63 → 63%
    expect(screen.getByText('63%')).toBeInTheDocument();
  });

  it('词条正确率按高低着色(success / error / 缺省—)', async () => {
    mockApi.adminWordbooksList.mockResolvedValue(listResponse());
    await renderPage();
    await waitFor(() => expect(screen.getByText('tenacious')).toBeInTheDocument());
    // 0.62 ≥ 0.6 → success 色
    const good = screen.getByText('62%');
    expect(good.getAttribute('style')).toContain('var(--success)');
    // 0.41 < 0.45 → error 色
    const bad = screen.getByText('41%');
    expect(bad.getAttribute('style')).toContain('var(--error)');
    // accuracy=null → 显示 —
    expect(screen.getAllByText('—').length).toBeGreaterThan(0);
  });

  it('点击「用户」类型筛选 chip 触发带 type 的列表重查', async () => {
    mockApi.adminWordbooksList.mockResolvedValue(listResponse());
    await renderPage();
    await waitFor(() => expect(screen.getAllByText('CET-6 高频').length).toBeGreaterThan(0));
    // 过滤 chip 是 .badge 按钮;文案精确等于"用户",避开列表项里"用户"角标(Badge 在右栏)
    const userChip = Array.from(document.querySelectorAll('button.badge'))
      .find((el) => el.textContent?.trim() === '用户') as HTMLElement;
    expect(userChip).toBeTruthy();
    fireEvent.click(userChip);
    await waitFor(() =>
      expect(mockApi.adminWordbooksList).toHaveBeenCalledWith(expect.objectContaining({ type: 'user' })),
    );
  });

  it('搜索框输入触发带 search 的列表重查', async () => {
    mockApi.adminWordbooksList.mockResolvedValue(listResponse());
    await renderPage();
    await waitFor(() => expect(screen.getAllByText('CET-6 高频').length).toBeGreaterThan(0));
    fireEvent.input(screen.getByPlaceholderText('搜索词库名 / 标签…'), { target: { value: 'CET' } });
    await waitFor(() =>
      expect(mockApi.adminWordbooksList).toHaveBeenCalledWith(expect.objectContaining({ search: 'CET' })),
    );
  });

  it('列表为空时显示空态', async () => {
    mockApi.adminWordbooksList.mockResolvedValue(listResponse([]));
    await renderPage();
    await waitFor(() => expect(screen.getByText('暂无词库')).toBeInTheDocument());
  });

  it('切到掌握热图 tab 拉取 heatmap 并渲染格子', async () => {
    mockApi.adminWordbooksList.mockResolvedValue(listResponse());
    mockApi.adminWordbookHeatmap.mockResolvedValue({
      wordbookId: 'wb-sys', maxCount: 100,
      cells: [{ wordId: 'w-1', text: 'tenacious', count: 90 }, { wordId: 'w-2', text: 'acquiesce', count: 10 }],
    });
    await renderPage();
    await waitFor(() => expect(screen.getAllByText('CET-6 高频').length).toBeGreaterThan(0));
    fireEvent.click(screen.getByText('掌握热图'));
    await waitFor(() => expect(mockApi.adminWordbookHeatmap).toHaveBeenCalledWith('wb-sys'));
    // 每格 div 带 title="<word> · <count> 次";断言两格已渲染
    await waitFor(() =>
      expect(document.querySelectorAll('div[title*="tenacious"]').length).toBeGreaterThan(0),
    );
    expect(document.querySelectorAll('div[title*="acquiesce"]').length).toBeGreaterThan(0);
  });

  it('切到用户分布 tab 拉取 distribution 并渲染分桶', async () => {
    mockApi.adminWordbooksList.mockResolvedValue(listResponse());
    mockApi.adminWordbookDistribution.mockResolvedValue({
      wordbookId: 'wb-sys', totalUsers: 10,
      buckets: [
        { label: '0–20%', min: 0, max: 0.2, userCount: 2 },
        { label: '80–100%', min: 0.8, max: 1, userCount: 8 },
      ],
    });
    await renderPage();
    await waitFor(() => expect(screen.getAllByText('CET-6 高频').length).toBeGreaterThan(0));
    fireEvent.click(screen.getByText('用户分布'));
    await waitFor(() => expect(mockApi.adminWordbookDistribution).toHaveBeenCalledWith('wb-sys'));
    await waitFor(() => expect(screen.getByText('0–20%')).toBeInTheDocument());
    expect(screen.getByText('80–100%')).toBeInTheDocument();
  });

  it('切到变更记录 tab 拉取 history 并中文化 action', async () => {
    mockApi.adminWordbooksList.mockResolvedValue(listResponse());
    mockApi.adminWordbookHistory.mockResolvedValue({
      data: [{ id: 'h-1', wordbookId: 'wb-sys', action: 'create', detail: '创建词库', createdAt: new Date().toISOString() }],
      total: 1, page: 1, perPage: 100, totalPages: 1,
    });
    await renderPage();
    await waitFor(() => expect(screen.getAllByText('CET-6 高频').length).toBeGreaterThan(0));
    fireEvent.click(screen.getByText('变更记录'));
    await waitFor(() => expect(mockApi.adminWordbookHistory).toHaveBeenCalled());
    await waitFor(() => expect(screen.getByText('创建词库')).toBeInTheDocument());
    // action=create → ACTION_LABEL 中文化为"创建"
    expect(screen.getByText('创建')).toBeInTheDocument();
  });
});

describe('WordbookCenterPage — 增删改/导出', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockApi.adminWordbookStats.mockResolvedValue(statsResp);
    mockApi.adminWordbookWords.mockResolvedValue(wordsResp);
    mockApi.adminWordbooksList.mockResolvedValue(listResponse());
  });

  it('新建词库:打开 modal 提交调用 create', async () => {
    mockApi.adminWordbookCreate.mockResolvedValue({ id: 'wb-new', name: '新词库', description: '', type: 'system' });
    await renderPage();
    await waitFor(() => expect(screen.getAllByText('CET-6 高频').length).toBeGreaterThan(0));
    fireEvent.click(screen.getByText('新建词库'));
    await waitFor(() => expect(screen.getByPlaceholderText('如 CET-6 高频')).toBeInTheDocument());
    fireEvent.input(screen.getByPlaceholderText('如 CET-6 高频'), { target: { value: '新词库' } });
    fireEvent.click(screen.getByText('创建'));
    await waitFor(() =>
      expect(mockApi.adminWordbookCreate).toHaveBeenCalledWith(expect.objectContaining({ name: '新词库' })),
    );
    await waitFor(() => expect(mockToast.success).toHaveBeenCalled());
  });

  it('新建词库:名称为空时拦截并提示', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getAllByText('CET-6 高频').length).toBeGreaterThan(0));
    fireEvent.click(screen.getByText('新建词库'));
    await waitFor(() => expect(screen.getByText('创建')).toBeInTheDocument());
    fireEvent.click(screen.getByText('创建'));
    await waitFor(() => expect(mockToast.warning).toHaveBeenCalledWith('请填写名称'));
    expect(mockApi.adminWordbookCreate).not.toHaveBeenCalled();
  });

  it('删除词库:确认后调用 delete', async () => {
    mockApi.adminWordbookDelete.mockResolvedValue({ deleted: true });
    await renderPage();
    await waitFor(() => expect(screen.getAllByText('CET-6 高频').length).toBeGreaterThan(0));
    // 详情头部「删除」按钮(此时唯一可见的删除文案)打开确认框
    fireEvent.click(screen.getByText('删除'));
    await waitFor(() => expect(screen.getByText('删除词库')).toBeInTheDocument());
    // Confirm footer 的确认按钮文案为 confirmText="删除";取最后一个匹配项(modal 内的按钮)
    const delButtons = screen.getAllByText('删除');
    fireEvent.click(delButtons[delButtons.length - 1]);
    await waitFor(() => expect(mockApi.adminWordbookDelete).toHaveBeenCalledWith('wb-sys'));
  });

  it('添加词条:提交调用 addWord', async () => {
    mockApi.adminWordbookAddWord.mockResolvedValue({
      id: 'w-new', text: 'serene', meaning: '宁静的', examples: [], appearCount: 0, accuracy: null,
    });
    await renderPage();
    await waitFor(() => expect(screen.getAllByText('CET-6 高频').length).toBeGreaterThan(0));
    // 详情头部「添加词条」按钮(取首个)
    fireEvent.click(screen.getAllByText('添加词条')[0]);
    await waitFor(() => expect(screen.getByPlaceholderText('如 tenacious')).toBeInTheDocument());
    fireEvent.input(screen.getByPlaceholderText('如 tenacious'), { target: { value: 'serene' } });
    fireEvent.input(screen.getByPlaceholderText('坚韧的 / 顽强的'), { target: { value: '宁静的' } });
    fireEvent.click(screen.getByText('添加'));
    await waitFor(() =>
      expect(mockApi.adminWordbookAddWord).toHaveBeenCalledWith('wb-sys', expect.objectContaining({ text: 'serene', meaning: '宁静的' })),
    );
  });

  it('导出词库:调用 export', async () => {
    mockApi.adminWordbookExport.mockResolvedValue({
      id: 'wb-sys', name: 'CET-6 高频', description: '', type: 'system', version: '', words: [],
    });
    // happy-dom 无 createObjectURL,打桩避免抛错
    Object.defineProperty(URL, 'createObjectURL', { configurable: true, value: vi.fn(() => 'blob:x') });
    Object.defineProperty(URL, 'revokeObjectURL', { configurable: true, value: vi.fn() });
    await renderPage();
    await waitFor(() => expect(screen.getAllByText('CET-6 高频').length).toBeGreaterThan(0));
    // 详情头部「导出」按钮
    fireEvent.click(screen.getAllByText('导出')[0]);
    await waitFor(() => expect(mockApi.adminWordbookExport).toHaveBeenCalledWith('wb-sys'));
  });
});
