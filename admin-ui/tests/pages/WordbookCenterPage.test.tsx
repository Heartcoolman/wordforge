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
    // 底部远程导入面板(/api/admin/wordbook-center/*)
    getSettings: vi.fn(),
    wbCenterBrowse: vi.fn(),
    wbCenterUpdates: vi.fn(),
    wbCenterImport: vi.fn(),
    wbCenterSync: vi.fn(),
    wbCenterPreview: vi.fn(),
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
  total: 3, page: 1, perPage: 20, totalPages: 1,
};

// ImportSection 默认未配置词书中心 → 不触发远程加载报错
function stubImportSection() {
  mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: '' });
  mockApi.wbCenterBrowse.mockResolvedValue([]);
}

async function renderPage() {
  const { default: Page } = await import('@/pages/WordbookCenterPage');
  return renderWithProviders(() => <Page />);
}

describe('WordbookCenterPage — 主从布局 + 词条/统计', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    stubImportSection();
    mockApi.adminWordbookStats.mockResolvedValue(statsResp);
    mockApi.adminWordbookWords.mockResolvedValue(wordsResp);
  });

  it('左列表渲染词库并按 counts 拼副标题', async () => {
    mockApi.adminWordbooksList.mockResolvedValue(listResponse());
    await renderPage();
    // 自动选中第一项后名称同时出现在列表与详情,故用 getAllByText
    await waitFor(() => expect(screen.getAllByText('CET-6 高频').length).toBeGreaterThan(0));
    expect(screen.getByText('我的生词本')).toBeInTheDocument();
    // 副标题用 counts
    expect(screen.getByText(/官方词库 1 套/)).toBeInTheDocument();
    expect(screen.getByText(/共 2,226 条词条/)).toBeInTheDocument();
  });

  it('列表加载后自动选中第一项并拉取 stats + words', async () => {
    mockApi.adminWordbooksList.mockResolvedValue(listResponse());
    await renderPage();
    await waitFor(() => expect(mockApi.adminWordbookStats).toHaveBeenCalledWith('wb-sys'));
    await waitFor(() => expect(screen.getByText('tenacious')).toBeInTheDocument());
    // KPI 卡:平均掌握度 0.63 → 63%
    expect(screen.getByText('63')).toBeInTheDocument();
  });

  it('词条正确率按高低着色(acc-good / acc-bad / 缺省—)', async () => {
    mockApi.adminWordbooksList.mockResolvedValue(listResponse());
    await renderPage();
    await waitFor(() => expect(screen.getByText('tenacious')).toBeInTheDocument());
    const good = screen.getByText('62%');
    expect(good.className).toContain('acc-good');
    const bad = screen.getByText('41%');
    expect(bad.className).toContain('acc-bad');
    // accuracy=null → 显示 —
    expect(screen.getAllByText('—').length).toBeGreaterThan(0);
  });

  it('点击类型筛选 chip 触发带 type 的列表重查', async () => {
    mockApi.adminWordbooksList.mockResolvedValue(listResponse());
    await renderPage();
    await waitFor(() => expect(screen.getAllByText('CET-6 高频').length).toBeGreaterThan(0));
    // 过滤 chip 位于 .wb-list-search,文案带空格("用户 ");用 aria 无关的范围查找避开列表项里的"用户"角标
    const userChip = Array.from(document.querySelectorAll('.wb-list-search .chip'))
      .find((el) => el.textContent?.startsWith('用户')) as HTMLElement;
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

  it('切到词频热图 tab 拉取 heatmap', async () => {
    mockApi.adminWordbooksList.mockResolvedValue(listResponse());
    mockApi.adminWordbookHeatmap.mockResolvedValue({
      wordbookId: 'wb-sys', maxCount: 100,
      cells: [{ wordId: 'w-1', text: 'tenacious', count: 90 }, { wordId: 'w-2', text: 'acquiesce', count: 10 }],
    });
    await renderPage();
    await waitFor(() => expect(screen.getAllByText('CET-6 高频').length).toBeGreaterThan(0));
    fireEvent.click(screen.getByText('词频热图'));
    await waitFor(() => expect(mockApi.adminWordbookHeatmap).toHaveBeenCalledWith('wb-sys'));
    await waitFor(() => expect(document.querySelectorAll('.freq-grid .freq-cell').length).toBe(2));
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

  it('切到变更历史 tab 拉取 history', async () => {
    mockApi.adminWordbooksList.mockResolvedValue(listResponse());
    mockApi.adminWordbookHistory.mockResolvedValue({
      data: [{ id: 'h-1', wordbookId: 'wb-sys', action: 'create', detail: '创建词库', createdAt: new Date().toISOString() }],
      total: 1, page: 1, perPage: 100, totalPages: 1,
    });
    await renderPage();
    await waitFor(() => expect(screen.getAllByText('CET-6 高频').length).toBeGreaterThan(0));
    fireEvent.click(screen.getByText('变更历史'));
    await waitFor(() => expect(mockApi.adminWordbookHistory).toHaveBeenCalled());
    await waitFor(() => expect(screen.getByText('创建词库')).toBeInTheDocument());
    // action 标签中文化
    expect(screen.getByText('创建')).toBeInTheDocument();
  });
});

describe('WordbookCenterPage — 增删改/导出', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    stubImportSection();
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
    await waitFor(() => expect(mockToast.error).toHaveBeenCalledWith('词库名称必填'));
    expect(mockApi.adminWordbookCreate).not.toHaveBeenCalled();
  });

  it('删除词库:确认后调用 delete', async () => {
    mockApi.adminWordbookDelete.mockResolvedValue({ deleted: true });
    await renderPage();
    await waitFor(() => expect(screen.getAllByText('CET-6 高频').length).toBeGreaterThan(0));
    fireEvent.click(screen.getByText('删除'));
    await waitFor(() => expect(screen.getByText('确认删除')).toBeInTheDocument());
    fireEvent.click(screen.getByText('确认删除'));
    await waitFor(() => expect(mockApi.adminWordbookDelete).toHaveBeenCalledWith('wb-sys'));
  });

  it('添加词条:提交调用 addWord', async () => {
    mockApi.adminWordbookAddWord.mockResolvedValue({
      id: 'w-new', text: 'serene', meaning: '宁静的', examples: [], appearCount: 0, accuracy: null,
    });
    await renderPage();
    await waitFor(() => expect(screen.getAllByText('CET-6 高频').length).toBeGreaterThan(0));
    fireEvent.click(screen.getByText('添加词条'));
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
    // jsdom 无 createObjectURL,打桩避免抛错
    (URL as unknown as { createObjectURL: () => string }).createObjectURL = vi.fn(() => 'blob:x');
    (URL as unknown as { revokeObjectURL: () => void }).revokeObjectURL = vi.fn();
    await renderPage();
    await waitFor(() => expect(screen.getAllByText('CET-6 高频').length).toBeGreaterThan(0));
    // 页头导出按钮(第一个)
    fireEvent.click(screen.getAllByText('导出')[0]);
    await waitFor(() => expect(mockApi.adminWordbookExport).toHaveBeenCalledWith('wb-sys'));
  });
});
