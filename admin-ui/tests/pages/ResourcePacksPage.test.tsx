import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

vi.mock('@/api/packs', () => ({
  packsApi: {
    list: vi.fn(),
    summary: vi.fn(),
    uploadVersion: vi.fn(),
    setActive: vi.fn(),
    deactivateVersion: vi.fn(),
    stats: vi.fn(),
  },
}));

vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { packsApi } from '@/api/packs';
const mockApi = packsApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const SUMMARY = {
  totalPacks: 2,
  newPacksThisMonth: 1,
  totalVersions: 5,
  versionsByChannel: { stable: 3, beta: 1, internal: 1 },
  installsToday: 120,
  installsTodaySuccess: 118,
  failureRate7d: 0.018,
  failures7dByOutcome: { verify_failed: 7, rollback: 2 },
  onlineClients: 284,
};

function pack(over: Partial<Record<string, unknown>> = {}) {
  return {
    packId: 'wordbook-en-gre',
    description: '英文 GRE 高频词库',
    createdAt: '2026-05-01T00:00:00Z',
    updatedAt: '2026-05-20T00:00:00Z',
    active: { stable: '3.2.0', beta: '3.3.0-beta.2', internal: null },
    totalInstalls: 21438,
    outcomes7d: { installed: 3812, verify_failed: 31, rollback: 7 },
    versions: [
      {
        packId: 'wordbook-en-gre',
        version: '3.3.0-beta.2',
        sha256: '8b3a7c92d1efaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa4a5d',
        signature: 'sig',
        signatureAlg: 'ed25519',
        sizeBytes: 341 * 1024,
        minAppVersion: '0.7.0',
        channel: 'beta',
        payloadPath: 'static/packs/wordbook-en-gre/3.3.0-beta.2/payload.json',
        publishedAt: '2026-05-27T14:30:00Z',
        deactivatedAt: null,
      },
      {
        packId: 'wordbook-en-gre',
        version: '3.2.0',
        sha256: 'a01c5d8e33faaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa92b7',
        signature: 'sig',
        signatureAlg: 'ed25519',
        sizeBytes: 328 * 1024,
        minAppVersion: '0.6.5',
        channel: 'stable',
        payloadPath: 'static/packs/wordbook-en-gre/3.2.0/payload.json',
        publishedAt: '2026-05-23T09:00:00Z',
        deactivatedAt: null,
      },
      {
        packId: 'wordbook-en-gre',
        version: '3.1.4',
        sha256: '7d2eb449c1abaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa0f3d',
        signature: 'sig',
        signatureAlg: 'ed25519',
        sizeBytes: 316 * 1024,
        minAppVersion: '0.6.0',
        channel: 'stable',
        payloadPath: 'static/packs/wordbook-en-gre/3.1.4/payload.json',
        publishedAt: '2026-05-01T16:45:00Z',
        deactivatedAt: null,
      },
    ],
    ...over,
  };
}

describe('ResourcePacksPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // 给每个 mock 安全默认值，避免 createResource / 交互触发 unhandled rejection
    mockApi.summary.mockResolvedValue(SUMMARY);
    mockApi.list.mockResolvedValue([]);
    mockApi.uploadVersion.mockResolvedValue({
      packId: 'wordbook-en-gre', version: '9.9.9', sha256: 'x', signature: 's', sizeBytes: 1, channel: 'beta',
    });
    mockApi.setActive.mockResolvedValue({
      packId: 'wordbook-en-gre', channel: 'stable', version: '3.1.4', activated: true, audienceClients: 284,
    });
    mockApi.deactivateVersion.mockResolvedValue({ deactivated: true });
    mockApi.stats.mockResolvedValue({ packId: 'wordbook-en-gre', stats: [] });
  });

  async function renderPage() {
    const { default: Page } = await import('@/pages/ResourcePacksPage');
    return renderWithProviders(() => <Page />);
  }

  it('renders page title "资源包"', async () => {
    mockApi.list.mockResolvedValue([]);
    await renderPage();
    await waitFor(() =>
      expect(screen.getByRole('heading', { name: '资源包' })).toBeInTheDocument(),
    );
  });

  it('renders KPI stat cards from summary', async () => {
    // 重设计后 KPI 卡来自 summary 端点（资源包总数 / 版本总数 / 今日下载 / 失败率 (7d)）。
    mockApi.list.mockResolvedValue([pack()]);
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('资源包总数')).toBeInTheDocument();
      expect(screen.getByText('版本总数')).toBeInTheDocument();
      expect(screen.getByText('今日下载')).toBeInTheDocument();
      expect(screen.getByText('失败率 (7d)')).toBeInTheDocument();
    });
    // 失败率 1.8%（summary 异步落定后）—— StatCard 渲染为 .card 容器
    await waitFor(() => {
      const failCard = screen.getByText('失败率 (7d)').closest('.card')!;
      expect(failCard).toHaveTextContent('1.8');
    });
  });

  it('shows empty state when API returns no packs', async () => {
    mockApi.list.mockResolvedValue([]);
    await renderPage();
    await waitFor(() => expect(screen.getByText('暂无资源包')).toBeInTheDocument());
  });

  it('renders pack card with description and packId, version table inline', async () => {
    // 重设计：pack 以卡片呈现，卡头含 description + packId，版本表内联渲染全部三态版本。
    mockApi.list.mockResolvedValue([pack()]);
    await renderPage();
    await waitFor(() => expect(screen.getByText('英文 GRE 高频词库')).toBeInTheDocument());
    // packId 出现在卡头副标题（与总下载同一行）
    expect(screen.getByText(/wordbook-en-gre/)).toBeInTheDocument();
    // 版本表渲染全部三态版本（td.mono 内为精确版本号文本）
    expect(screen.getByText('3.3.0-beta.2')).toBeInTheDocument();
    expect(screen.getByText('3.2.0')).toBeInTheDocument();
    expect(screen.getByText('3.1.4')).toBeInTheDocument();
  });

  it('renders channel filter tabs (全部/Stable/Beta/Internal)', async () => {
    // 重设计：Tabs 渲染为 <button class="tab">（非 role=tab）。
    mockApi.list.mockResolvedValue([pack()]);
    await renderPage();
    await waitFor(() => {
      expect(screen.getByRole('button', { name: /全部/ })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /Stable/ })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /Beta/ })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /Internal/ })).toBeInTheDocument();
    });
  });

  it('filters pack list by channel tab', async () => {
    // 重设计：通道 tab 过滤的是「该通道有激活版本的 pack」列表。
    const onlyStable = pack({
      packId: 'amas-presets-default',
      description: 'AMAS 默认参数预设包',
      active: { stable: '1.2.3', beta: null, internal: null },
      versions: [
        { ...pack().versions[1], packId: 'amas-presets-default', version: '1.2.3', channel: 'stable' },
      ],
    });
    mockApi.list.mockResolvedValue([pack(), onlyStable]);
    await renderPage();
    await waitFor(() => expect(screen.getByText('英文 GRE 高频词库')).toBeInTheDocument());
    expect(screen.getByText('AMAS 默认参数预设包')).toBeInTheDocument();

    // 切到 Beta：只有 wordbook-en-gre（有 beta 激活）保留，amas（无 beta）被过滤
    fireEvent.click(screen.getByRole('button', { name: /Beta/ }));
    await waitFor(() => {
      expect(screen.getByText('英文 GRE 高频词库')).toBeInTheDocument();
      expect(screen.queryByText('AMAS 默认参数预设包')).not.toBeInTheDocument();
    });
  });

  it('opens upload modal with version field and submit button', async () => {
    // 重设计：上传由弹窗完成，含 pack_id / 版本号 输入 + 「上传并签名」按钮（真实 XHR 进度）。
    mockApi.list.mockResolvedValue([pack()]);
    await renderPage();
    // 页头与每张 pack 卡片都有「上传新版」按钮 → 取第一个（页头）触发
    await waitFor(() =>
      expect(screen.getAllByRole('button', { name: '上传新版' }).length).toBeGreaterThan(0),
    );
    fireEvent.click(screen.getAllByRole('button', { name: '上传新版' })[0]);

    await waitFor(() => {
      expect(screen.getByRole('heading', { name: '上传新版资源包' })).toBeInTheDocument();
      expect(screen.getByText(/版本号/)).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /上传并签名/ })).toBeInTheDocument();
    });
  });

  it('shows install outcomes (installed/verify_failed/rollback) — 真实三态，无第四态', async () => {
    mockApi.list.mockResolvedValue([pack()]);
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('installed')).toBeInTheDocument();
      expect(screen.getByText('verify_failed')).toBeInTheDocument();
      expect(screen.getByText('rollback')).toBeInTheDocument();
    });
    // 杜撰的旧设计四态不应出现
    expect(screen.queryByText('signature_mismatch')).not.toBeInTheDocument();
    expect(screen.queryByText('size_too_large')).not.toBeInTheDocument();
  });

  it('opens activation modal with online client audience and confirms broadcast', async () => {
    mockApi.list.mockResolvedValue([pack()]);
    mockApi.setActive.mockResolvedValue({
      packId: 'wordbook-en-gre', channel: 'stable', version: '3.1.4', activated: true, audienceClients: 284,
    });
    await renderPage();
    // 3.1.4 stable 已被替换（active 是 3.2.0）→ 有「激活」按钮
    await waitFor(() => expect(screen.getByText('3.1.4')).toBeInTheDocument());

    const activateBtns = screen.getAllByRole('button', { name: '激活' });
    fireEvent.click(activateBtns[0]);
    await waitFor(() => expect(screen.getByRole('heading', { name: '切换激活版本？' })).toBeInTheDocument());
    // 受众展示在线客户端数（summary.onlineClients）
    expect(screen.getByText('284')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /切换并广播/ }));
    await waitFor(() => expect(mockApi.setActive).toHaveBeenCalledWith('wordbook-en-gre', 'stable', '3.1.4'));
  });

  it('opens stats modal aggregating version × outcome', async () => {
    mockApi.list.mockResolvedValue([pack()]);
    mockApi.stats.mockResolvedValue({
      packId: 'wordbook-en-gre',
      stats: [
        { version: '3.2.0', outcome: 'installed', count: 1200 },
        { version: '3.2.0', outcome: 'rollback', count: 4 },
      ],
    });
    await renderPage();
    await waitFor(() => expect(screen.getAllByText('统计').length).toBeGreaterThan(0));
    fireEvent.click(screen.getAllByRole('button', { name: '统计' })[0]);
    await waitFor(() => {
      expect(screen.getByRole('heading', { name: '安装统计' })).toBeInTheDocument();
      expect(mockApi.stats).toHaveBeenCalledWith('wordbook-en-gre');
    });
    await waitFor(() => expect(screen.getByText('1,200')).toBeInTheDocument());
  });

  it('triggers soft-delete confirm on 停用', async () => {
    // 重设计：行内软删除按钮文案为「停用」，确认弹窗标题「确认停用版本？」，确认按钮「确认停用」。
    mockApi.list.mockResolvedValue([pack()]);
    mockApi.deactivateVersion.mockResolvedValue({ deactivated: true });
    await renderPage();
    await waitFor(() => expect(screen.getAllByText('停用').length).toBeGreaterThan(0));
    fireEvent.click(screen.getAllByRole('button', { name: '停用' })[0]);
    await waitFor(() => expect(screen.getByRole('heading', { name: '确认停用版本？' })).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '确认停用' }));
    await waitFor(() => expect(mockApi.deactivateVersion).toHaveBeenCalled());
  });
});
