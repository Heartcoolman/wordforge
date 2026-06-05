import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from './helpers/render';

// ─────────────────────────────────────────────────────────────────────────
// 配置/管理类小组件覆盖：StageSwitchPanel / AnnouncementManager /
// NotificationBell / RbacPanel / ApiKeysPanel。
// 所有 @/api/* 与 @/stores/ui 全 mock，给 Promise 默认实现，绝无真实网络。
// EChart 桩成占位 div（StageSwitchPanel 用），ConfirmDialog/SectionCard/
// Card/Empty/Spinner/Modal 均为同步纯 UI，直接用真实实现。
// ─────────────────────────────────────────────────────────────────────────

vi.mock('@/components/ui/EChart', () => ({
  EChart: (props: { option?: () => unknown }) => {
    let ok = false;
    try {
      props.option?.();
      ok = true;
    } catch {
      ok = false;
    }
    return <div data-testid="chart" data-ok={String(ok)} />;
  },
}));

// adminApi —— 涵盖五组件用到的全部方法，默认 reject 防止 createResource 卡 pending；
// 各 describe 在 beforeEach 里按需 mockResolvedValue 覆盖。
vi.mock('@/api/admin', () => ({
  adminApi: {
    amasStageDistribution: vi.fn(),
    feedbackAnnouncements: vi.fn(),
    createFeedbackAnnouncement: vi.fn(),
    updateFeedbackAnnouncement: vi.fn(),
    deleteFeedbackAnnouncement: vi.fn(),
    rbacAdmins: vi.fn(),
    createRbacAdmin: vi.fn(),
    updateRbacAdminRole: vi.fn(),
    deleteRbacAdmin: vi.fn(),
    apiKeys: vi.fn(),
    createApiKey: vi.fn(),
    rotateApiKey: vi.fn(),
    deleteApiKey: vi.fn(),
  },
}));

vi.mock('@/api/notifications', () => ({
  notificationsApi: {
    list: vi.fn(),
    markRead: vi.fn(),
    markAllRead: vi.fn(),
  },
}));

vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
import { notificationsApi } from '@/api/notifications';
import { uiStore } from '@/stores/ui';

const mockAdmin = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockNotif = notificationsApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockToast = uiStore.toast as unknown as Record<string, ReturnType<typeof vi.fn>>;

afterEach(() => vi.clearAllMocks());

// ───────────────────────────── StageSwitchPanel ─────────────────────────────
describe('StageSwitchPanel', () => {
  async function render() {
    const { StageSwitchPanel } = await import('@/pages/amas/StageSwitchPanel');
    return renderWithProviders(() => <StageSwitchPanel />);
  }

  it('空态：totalUsers=0 渲染 Empty 占位', async () => {
    mockAdmin.amasStageDistribution.mockResolvedValue({ totalUsers: 0, stages: [], trend: [] });
    await render();
    await waitFor(() => expect(screen.getByText('暂无用户状态')).toBeInTheDocument());
    expect(screen.getByText('阶段切换比例')).toBeInTheDocument();
  });

  it('有数据：渲染三态占比 + 趋势图（option 构建器执行）', async () => {
    mockAdmin.amasStageDistribution.mockResolvedValue({
      totalUsers: 1200,
      stages: [
        { stage: 'stable', users: 600, pct: 0.5, avgDecisions: 220, retention7d: 0.8, mainRoute: 'r1' },
        { stage: 'cold', users: 360, pct: 0.3, avgDecisions: 5, retention7d: 0.4, mainRoute: 'r2' },
        { stage: 'transition', users: 240, pct: 0.2, avgDecisions: 80, retention7d: 0.6, mainRoute: 'r3' },
      ],
      trend: [
        { date: '2026-06-01', cold: 0.3, transition: 0.2, stable: 0.5 },
        { date: '2026-06-02', cold: 0.25, transition: 0.2, stable: 0.55 },
      ],
    });
    await render();
    await waitFor(() => expect(screen.getByText('稳定阶段 (≥ 200 次答题)')).toBeInTheDocument());
    expect(screen.getByText('50%')).toBeInTheDocument();
    expect(screen.getByText('30%')).toBeInTheDocument();
    expect(screen.getByText('20%')).toBeInTheDocument();
    expect(screen.getByText('600 用户')).toBeInTheDocument();
    const chart = screen.getByTestId('chart');
    expect(chart).toHaveAttribute('data-ok', 'true');
  });
});

// ─────────────────────────── AnnouncementManager ───────────────────────────
describe('AnnouncementManager', () => {
  async function render(open = true) {
    const onClose = vi.fn();
    const { AnnouncementManager } = await import('@/pages/feedback/AnnouncementManager');
    const r = renderWithProviders(() => <AnnouncementManager open={open} onClose={onClose} />);
    return { ...r, onClose };
  }

  it('open=false 时不拉取列表，不渲染表单标题', async () => {
    await render(false);
    expect(mockAdmin.feedbackAnnouncements).not.toHaveBeenCalled();
    expect(screen.queryByText('公告 / FAQ 管理')).not.toBeInTheDocument();
  });

  it('空态：渲染 Empty', async () => {
    mockAdmin.feedbackAnnouncements.mockResolvedValue({ data: [] });
    await render(true);
    await waitFor(() => expect(screen.getByText('暂无公告 / FAQ')).toBeInTheDocument());
  });

  it('列表渲染 + 切换发布状态调用 update', async () => {
    mockAdmin.feedbackAnnouncements.mockResolvedValue({
      data: [
        { id: 'a1', title: '维护通知', body: '今晚维护', kind: 'announcement', published: true },
        { id: 'a2', title: '如何登录', body: '步骤…', kind: 'faq', published: false },
      ],
    });
    mockAdmin.updateFeedbackAnnouncement.mockResolvedValue({});
    await render(true);
    await waitFor(() => expect(screen.getByText('维护通知')).toBeInTheDocument());
    expect(screen.getByText('如何登录')).toBeInTheDocument();
    expect(screen.getByText('草稿')).toBeInTheDocument();
    // 第一条 published=true → 按钮文案"下架"
    fireEvent.click(screen.getByText('下架'));
    await waitFor(() =>
      expect(mockAdmin.updateFeedbackAnnouncement).toHaveBeenCalledWith('a1', { published: false }),
    );
  });

  it('表单校验：空标题/正文触发 warning toast', async () => {
    mockAdmin.feedbackAnnouncements.mockResolvedValue({ data: [] });
    await render(true);
    await waitFor(() => expect(screen.getByText('暂无公告 / FAQ')).toBeInTheDocument());
    fireEvent.click(screen.getByText('创建'));
    await waitFor(() => expect(mockToast.warning).toHaveBeenCalled());
    expect(mockAdmin.createFeedbackAnnouncement).not.toHaveBeenCalled();
  });

  it('填写后创建：调用 createFeedbackAnnouncement', async () => {
    mockAdmin.feedbackAnnouncements.mockResolvedValue({ data: [] });
    mockAdmin.createFeedbackAnnouncement.mockResolvedValue({});
    await render(true);
    await waitFor(() => expect(screen.getByText('暂无公告 / FAQ')).toBeInTheDocument());
    fireEvent.input(screen.getByLabelText('标题'), { target: { value: '新公告' } });
    fireEvent.input(screen.getByLabelText('正文'), { target: { value: '正文内容' } });
    fireEvent.click(screen.getByText('创建'));
    await waitFor(() =>
      expect(mockAdmin.createFeedbackAnnouncement).toHaveBeenCalledWith(
        expect.objectContaining({ title: '新公告', body: '正文内容', kind: 'announcement' }),
      ),
    );
    await waitFor(() => expect(mockToast.success).toHaveBeenCalledWith('已创建'));
  });

  it('编辑：填入表单切换到"保存修改"，FAQ 分段按钮可切换', async () => {
    mockAdmin.feedbackAnnouncements.mockResolvedValue({
      data: [{ id: 'a1', title: '原标题', body: '原正文', kind: 'announcement', published: false }],
    });
    await render(true);
    await waitFor(() => expect(screen.getByText('原标题')).toBeInTheDocument());
    fireEvent.click(screen.getByText('编辑'));
    await waitFor(() => expect(screen.getByText('保存修改')).toBeInTheDocument());
    expect(screen.getByText('编辑条目')).toBeInTheDocument();
    // 切到 FAQ 分段
    fireEvent.click(screen.getByText('FAQ'));
    const faqBtn = screen.getByText('FAQ');
    await waitFor(() => expect(faqBtn).toHaveAttribute('aria-pressed', 'true'));
    // 取消编辑回到新建态
    fireEvent.click(screen.getByText('取消编辑'));
    await waitFor(() => expect(screen.getByText('新建条目')).toBeInTheDocument());
  });

  it('删除：window.confirm 通过则调 deleteFeedbackAnnouncement', async () => {
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true);
    mockAdmin.feedbackAnnouncements.mockResolvedValue({
      data: [{ id: 'a1', title: '可删除', body: 'x', kind: 'announcement', published: true }],
    });
    mockAdmin.deleteFeedbackAnnouncement.mockResolvedValue({});
    await render(true);
    await waitFor(() => expect(screen.getByText('可删除')).toBeInTheDocument());
    fireEvent.click(screen.getByText('删除'));
    await waitFor(() => expect(mockAdmin.deleteFeedbackAnnouncement).toHaveBeenCalledWith('a1'));
    confirmSpy.mockRestore();
  });

  it('加载失败时表单仍可见（list reject 不抛出渲染）', async () => {
    mockAdmin.feedbackAnnouncements.mockRejectedValue(new Error('boom'));
    await render(true);
    // resource error 不应阻塞 Modal 容器渲染
    await waitFor(() => expect(screen.getByText('公告 / FAQ 管理')).toBeInTheDocument());
  });
});

// ───────────────────────────── NotificationBell ─────────────────────────────
describe('NotificationBell', () => {
  async function render() {
    const { NotificationBell } = await import('@/components/layout/NotificationBell');
    return renderWithProviders(() => <NotificationBell />);
  }

  it('挂载即拉未读计数，>0 显示角标', async () => {
    mockNotif.list.mockImplementation((unread?: boolean) =>
      Promise.resolve(
        unread
          ? { items: [], unreadCount: 3 }
          : { items: [], unreadCount: 3 },
      ),
    );
    await render();
    await waitFor(() => expect(screen.getByText('3')).toBeInTheDocument());
    // refreshCount 用 unread=true
    expect(mockNotif.list).toHaveBeenCalledWith(true);
  });

  it('未读 > 99 显示 99+', async () => {
    mockNotif.list.mockResolvedValue({ items: [], unreadCount: 150 });
    await render();
    await waitFor(() => expect(screen.getByText('99+')).toBeInTheDocument());
  });

  it('打开面板拉全量并渲染条目，点击标记已读', async () => {
    const alert = {
      id: 'n1',
      source: 'amas',
      kind: 'soft_block',
      severity: 'error' as const,
      title: 'AMAS 拦截',
      message: '决策失败',
      count: 2,
      firstSeenAt: '2026-06-01T00:00:00Z',
      lastSeenAt: '2026-06-01T00:00:00Z',
      readAt: null,
      ackedBy: null,
    };
    mockNotif.list.mockImplementation((unread?: boolean) =>
      Promise.resolve(unread ? { items: [], unreadCount: 1 } : { items: [alert], unreadCount: 1 }),
    );
    mockNotif.markRead.mockResolvedValue({ read: true, unreadCount: 0 });
    await render();
    await waitFor(() => expect(screen.getByText('1')).toBeInTheDocument());
    // 打开面板
    fireEvent.click(screen.getByLabelText(/通知收件箱/));
    await waitFor(() => expect(screen.getByText('AMAS 拦截')).toBeInTheDocument());
    expect(screen.getByText('决策失败')).toBeInTheDocument();
    expect(screen.getByText('×2')).toBeInTheDocument();
    // 点击条目标记已读
    fireEvent.click(screen.getByText('AMAS 拦截'));
    await waitFor(() => expect(mockNotif.markRead).toHaveBeenCalledWith('n1'));
  });

  it('面板空态：暂无通知', async () => {
    mockNotif.list.mockResolvedValue({ items: [], unreadCount: 0 });
    await render();
    // 等待初次 refreshCount
    await waitFor(() => expect(mockNotif.list).toHaveBeenCalled());
    fireEvent.click(screen.getByLabelText('通知收件箱'));
    await waitFor(() => expect(screen.getByText('暂无通知')).toBeInTheDocument());
  });

  it('全部已读：调用 markAllRead 并归零角标', async () => {
    const a = {
      id: 'n1', source: 's', kind: 'k', severity: 'warning' as const,
      title: 'T', message: 'M', count: 1,
      firstSeenAt: '2026-06-01T00:00:00Z', lastSeenAt: '2026-06-01T00:00:00Z',
      readAt: null, ackedBy: null,
    };
    mockNotif.list.mockImplementation((unread?: boolean) =>
      Promise.resolve(unread ? { items: [], unreadCount: 2 } : { items: [a], unreadCount: 2 }),
    );
    mockNotif.markAllRead.mockResolvedValue({ marked: 2, unreadCount: 0 });
    await render();
    await waitFor(() => expect(screen.getByText('2')).toBeInTheDocument());
    fireEvent.click(screen.getByLabelText(/通知收件箱/));
    await waitFor(() => expect(screen.getByText('全部已读')).toBeInTheDocument());
    fireEvent.click(screen.getByText('全部已读'));
    await waitFor(() => expect(mockNotif.markAllRead).toHaveBeenCalled());
    await waitFor(() => expect(screen.queryByText('2')).not.toBeInTheDocument());
  });

  it('loadInbox 失败时 toast.error', async () => {
    mockNotif.list.mockImplementation((unread?: boolean) =>
      unread ? Promise.resolve({ items: [], unreadCount: 1 }) : Promise.reject(new Error('boom')),
    );
    await render();
    await waitFor(() => expect(screen.getByText('1')).toBeInTheDocument());
    fireEvent.click(screen.getByLabelText(/通知收件箱/));
    await waitFor(() => expect(mockToast.error).toHaveBeenCalledWith('加载通知失败'));
  });
});

// ───────────────────────────────── RbacPanel ─────────────────────────────────
describe('RbacPanel', () => {
  async function render() {
    const { RbacPanel } = await import('@/pages/settings/RbacPanel');
    return renderWithProviders(() => <RbacPanel />);
  }

  it('空态：暂无管理员', async () => {
    mockAdmin.rbacAdmins.mockResolvedValue({ admins: [] });
    await render();
    await waitFor(() => expect(screen.getByText('暂无管理员')).toBeInTheDocument());
  });

  it('渲染管理员列表 + 改角色调用 update', async () => {
    mockAdmin.rbacAdmins.mockResolvedValue({
      admins: [
        { id: 'u1', email: 'alice@wordforge.app', role: 'admin', createdAt: '2026-06-01T00:00:00Z', lockedUntil: null },
        {
          id: 'u2', email: 'bob@wordforge.app', role: 'super_admin', createdAt: '2026-06-01T00:00:00Z',
          lockedUntil: '2099-01-01T00:00:00Z',
        },
      ],
    });
    mockAdmin.updateRbacAdminRole.mockResolvedValue({
      id: 'u1', email: 'alice@wordforge.app', role: 'super_admin', createdAt: '2026-06-01T00:00:00Z', lockedUntil: null,
    });
    await render();
    await waitFor(() => expect(screen.getByText('alice@wordforge.app')).toBeInTheDocument());
    expect(screen.getByText('bob@wordforge.app')).toBeInTheDocument();
    // bob 锁定 → 显示"锁定至"
    expect(screen.getByText(/锁定至/)).toBeInTheDocument();
    // 改 alice 角色
    const selects = screen.getAllByRole('combobox');
    fireEvent.change(selects[0], { target: { value: 'super_admin' } });
    await waitFor(() => expect(mockAdmin.updateRbacAdminRole).toHaveBeenCalledWith('u1', 'super_admin'));
    await waitFor(() => expect(mockToast.success).toHaveBeenCalledWith('角色已更新'));
  });

  it('展开创建表单 + 校验空字段', async () => {
    mockAdmin.rbacAdmins.mockResolvedValue({ admins: [] });
    await render();
    await waitFor(() => expect(screen.getByText('暂无管理员')).toBeInTheDocument());
    fireEvent.click(screen.getByText('+ 邀请管理员'));
    await waitFor(() => expect(screen.getByText('新增管理员')).toBeInTheDocument());
    fireEvent.click(screen.getByText('创建'));
    await waitFor(() => expect(mockToast.warning).toHaveBeenCalled());
    expect(mockAdmin.createRbacAdmin).not.toHaveBeenCalled();
  });

  it('填写后创建管理员', async () => {
    mockAdmin.rbacAdmins.mockResolvedValue({ admins: [] });
    mockAdmin.createRbacAdmin.mockResolvedValue({});
    await render();
    await waitFor(() => expect(screen.getByText('暂无管理员')).toBeInTheDocument());
    fireEvent.click(screen.getByText('+ 邀请管理员'));
    await waitFor(() => expect(screen.getByPlaceholderText('email@wordforge.app')).toBeInTheDocument());
    fireEvent.input(screen.getByPlaceholderText('email@wordforge.app'), { target: { value: 'new@wordforge.app' } });
    fireEvent.input(screen.getByPlaceholderText('初始密码'), { target: { value: 'pw123456' } });
    fireEvent.click(screen.getByText('创建'));
    await waitFor(() =>
      expect(mockAdmin.createRbacAdmin).toHaveBeenCalledWith(
        expect.objectContaining({ email: 'new@wordforge.app', password: 'pw123456', role: 'admin' }),
      ),
    );
    await waitFor(() => expect(mockToast.success).toHaveBeenCalledWith('管理员已创建'));
  });

  it('删除流程：触发确认对话框并确认', async () => {
    mockAdmin.rbacAdmins.mockResolvedValue({
      admins: [{ id: 'u1', email: 'del@wordforge.app', role: 'admin', createdAt: '2026-06-01T00:00:00Z', lockedUntil: null }],
    });
    mockAdmin.deleteRbacAdmin.mockResolvedValue({ deleted: true, adminId: 'u1' });
    await render();
    await waitFor(() => expect(screen.getByText('del@wordforge.app')).toBeInTheDocument());
    fireEvent.click(screen.getByText('删除'));
    // ConfirmDialog 出现（Portal 到 body）
    await waitFor(() => expect(screen.getByText('删除管理员')).toBeInTheDocument());
    // 点确认按钮（文案"删除"，对话框内的 confirm 按钮）
    const confirmBtns = screen.getAllByText('删除');
    fireEvent.click(confirmBtns[confirmBtns.length - 1]);
    await waitFor(() => expect(mockAdmin.deleteRbacAdmin).toHaveBeenCalledWith('u1'));
    await waitFor(() => expect(mockToast.success).toHaveBeenCalledWith('管理员已删除'));
  });

  it('加载失败时 toast.error', async () => {
    mockAdmin.rbacAdmins.mockRejectedValue(new Error('fail'));
    await render();
    await waitFor(() => expect(mockToast.error).toHaveBeenCalledWith('加载管理员失败', 'fail'));
  });
});

// ──────────────────────────────── ApiKeysPanel ────────────────────────────────
describe('ApiKeysPanel', () => {
  async function render(onExpiringChange?: (n: number) => void) {
    const { ApiKeysPanel } = await import('@/pages/settings/ApiKeysPanel');
    return renderWithProviders(() => <ApiKeysPanel onExpiringChange={onExpiringChange} />);
  }

  it('空态：暂无 API 密钥', async () => {
    mockAdmin.apiKeys.mockResolvedValue({ keys: [] });
    await render();
    await waitFor(() => expect(screen.getByText('暂无 API 密钥')).toBeInTheDocument());
  });

  it('渲染密钥列表 + 过期着色 + onExpiringChange 回传', async () => {
    const onExpiring = vi.fn();
    const soon = new Date(Date.now() + 5 * 86400000).toISOString();
    mockAdmin.apiKeys.mockResolvedValue({
      keys: [
        { id: '1', name: 'ci', scope: 'read', prefix: 'key_ab', createdAt: '2026-06-01T00:00:00Z', createdBy: null, expiresAt: null, lastUsedAt: null, revokedAt: null },
        { id: '2', name: 'soon-key', scope: 'write', prefix: 'key_cd', createdAt: '2026-06-01T00:00:00Z', createdBy: null, expiresAt: soon, lastUsedAt: null, revokedAt: null },
        { id: '3', name: 'revoked-key', scope: 'admin', prefix: 'key_ef', createdAt: '2026-06-01T00:00:00Z', createdBy: null, expiresAt: null, lastUsedAt: null, revokedAt: '2026-06-02T00:00:00Z' },
      ],
    });
    await render(onExpiring);
    await waitFor(() => expect(screen.getByText('ci')).toBeInTheDocument());
    expect(screen.getByText('soon-key')).toBeInTheDocument();
    expect(screen.getByText('永不过期')).toBeInTheDocument();
    expect(screen.getByText('5 天后过期')).toBeInTheDocument();
    expect(screen.getByText('已撤销')).toBeInTheDocument();
    // 即将过期数量 = 1（soon-key warn）
    await waitFor(() => expect(onExpiring).toHaveBeenCalledWith(1));
  });

  it('展开创建表单 + 空名校验', async () => {
    mockAdmin.apiKeys.mockResolvedValue({ keys: [] });
    await render();
    await waitFor(() => expect(screen.getByText('暂无 API 密钥')).toBeInTheDocument());
    fireEvent.click(screen.getByText('+ 生成新密钥'));
    await waitFor(() => expect(screen.getByText('生成新密钥')).toBeInTheDocument());
    fireEvent.click(screen.getByText('生成'));
    await waitFor(() => expect(mockToast.warning).toHaveBeenCalledWith('请填写密钥名称'));
    expect(mockAdmin.createApiKey).not.toHaveBeenCalled();
  });

  it('填写后生成密钥：展示明文 reveal', async () => {
    mockAdmin.apiKeys.mockResolvedValue({ keys: [] });
    mockAdmin.createApiKey.mockResolvedValue({ key: {}, plaintext: 'sk_live_secret_xyz', message: 'ok' });
    await render();
    await waitFor(() => expect(screen.getByText('暂无 API 密钥')).toBeInTheDocument());
    fireEvent.click(screen.getByText('+ 生成新密钥'));
    await waitFor(() => expect(screen.getByPlaceholderText(/密钥名称/)).toBeInTheDocument());
    fireEvent.input(screen.getByPlaceholderText(/密钥名称/), { target: { value: 'github-ci' } });
    fireEvent.click(screen.getByText('生成'));
    await waitFor(() =>
      expect(mockAdmin.createApiKey).toHaveBeenCalledWith(expect.objectContaining({ name: 'github-ci', scope: 'read' })),
    );
    await waitFor(() => expect(screen.getByText('sk_live_secret_xyz')).toBeInTheDocument());
    await waitFor(() => expect(mockToast.success).toHaveBeenCalledWith('密钥已生成，请立即复制'));
  });

  it('轮换密钥：调用 rotateApiKey 并展示新明文', async () => {
    mockAdmin.apiKeys.mockResolvedValue({
      keys: [{ id: '1', name: ' rot ', scope: 'read', prefix: 'key_ab', createdAt: '2026-06-01T00:00:00Z', createdBy: null, expiresAt: null, lastUsedAt: null, revokedAt: null }],
    });
    mockAdmin.rotateApiKey.mockResolvedValue({ key: {}, plaintext: 'sk_rotated_new', message: 'ok' });
    await render();
    await waitFor(() => expect(screen.getByTitle('轮换')).toBeInTheDocument());
    fireEvent.click(screen.getByTitle('轮换'));
    await waitFor(() => expect(mockAdmin.rotateApiKey).toHaveBeenCalledWith('1'));
    await waitFor(() => expect(screen.getByText('sk_rotated_new')).toBeInTheDocument());
  });

  it('撤销密钥：确认对话框 + deleteApiKey', async () => {
    mockAdmin.apiKeys.mockResolvedValue({
      keys: [{ id: '1', name: 'kill-me', scope: 'read', prefix: 'key_ab', createdAt: '2026-06-01T00:00:00Z', createdBy: null, expiresAt: null, lastUsedAt: null, revokedAt: null }],
    });
    mockAdmin.deleteApiKey.mockResolvedValue({ revoked: true, keyId: 1 });
    await render();
    await waitFor(() => expect(screen.getByTitle('撤销')).toBeInTheDocument());
    fireEvent.click(screen.getByTitle('撤销'));
    await waitFor(() => expect(screen.getByText('撤销 API 密钥')).toBeInTheDocument());
    // 确认按钮文案为 "撤销"（取 confirm 按钮，最后一个匹配项）
    const revokeMatches = screen.getAllByText('撤销');
    fireEvent.click(revokeMatches[revokeMatches.length - 1]);
    await waitFor(() => expect(mockAdmin.deleteApiKey).toHaveBeenCalledWith('1'));
    await waitFor(() => expect(mockToast.success).toHaveBeenCalledWith('密钥已撤销'));
  });

  it('加载失败时 toast.error', async () => {
    mockAdmin.apiKeys.mockRejectedValue(new Error('netfail'));
    await render();
    await waitFor(() => expect(mockToast.error).toHaveBeenCalledWith('加载 API 密钥失败', 'netfail'));
  });
});
