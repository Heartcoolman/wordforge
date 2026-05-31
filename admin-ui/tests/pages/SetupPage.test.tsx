import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: {
    setup: vi.fn(),
    checkStatus: vi.fn().mockResolvedValue({ initialized: false }),
    // 重构后:页面 onMount 会调用 setupEnvCheck() 拉取 5 项环境自检
    setupEnvCheck: vi.fn().mockResolvedValue({
      checks: [
        { key: 'db_schema', label: '数据库 schema', ok: true, detail: 'migrations 已应用' },
        { key: 'static_writable', label: 'static 可写', ok: true, detail: '/static 可写' },
        { key: 'listening_port', label: '监听端口', ok: true, detail: ':3000' },
        { key: 'signing_key', label: '签名密钥', ok: true, detail: '已配置' },
        { key: 'admin_table', label: 'admin 表', ok: true, detail: '空' },
      ],
    }),
  },
}));

vi.mock('@/lib/token', () => ({
  tokenManager: {
    getAdminToken: vi.fn(() => null),
    setAdminToken: vi.fn(),
    clearAdminToken: vi.fn(),
    getToken: vi.fn(() => null),
    isAuthenticated: vi.fn(() => false),
    needsRefresh: vi.fn(() => false),
    refreshAccessToken: vi.fn().mockResolvedValue(false),
  },
}));

vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

describe('SetupPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  async function renderPage() {
    const { default: SetupPage } = await import('@/pages/SetupPage');
    return renderWithProviders(() => <SetupPage />);
  }

  /**
   * PR redesign: SetupPage 现在是单屏布局——左侧环境自检面板 + 右侧创建超管表单卡同屏并列。
   * onMount 后跑 env-check(setupEnvCheck),自检完成后表单(step 1/2,即 step()!==3)即可见,
   * 无独立"下一步"导航。这个 helper 等环境自检 spinner 退场、表单渲染出来。
   */
  async function waitForForm() {
    await waitFor(() => {
      expect(screen.getByLabelText(/管理员邮箱/)).toBeInTheDocument();
    });
  }

  it('shows "初始化 WordForge 管理后台" heading', async () => {
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('初始化 WordForge 管理后台')).toBeInTheDocument();
    });
  });

  it('shows wizard progress chips for 3 steps', async () => {
    await renderPage();
    await waitFor(() => {
      // 3 步 chip 文案(进度条):检测 → 创建超管 → 登录
      expect(screen.getByText('检测')).toBeInTheDocument();
      expect(screen.getByText('创建超管')).toBeInTheDocument();
      expect(screen.getByText('登录')).toBeInTheDocument();
    });
  });

  it('shows email input field', async () => {
    await renderPage();
    await waitForForm();
    expect(screen.getByLabelText(/管理员邮箱/)).toBeInTheDocument();
  });

  it('shows password input field with placeholder', async () => {
    await renderPage();
    await waitForForm();
    // /^密码/ 避免匹配到"确认密码"
    const input = screen.getByLabelText(/^密码/);
    expect(input).toBeInTheDocument();
    expect(input).toHaveAttribute('placeholder', '至少 8 位，包含字母与数字');
  });

  it('shows confirm password input field', async () => {
    await renderPage();
    await waitForForm();
    expect(screen.getByLabelText(/确认密码/)).toBeInTheDocument();
  });

  it('shows "创建管理员并登录" submit button', async () => {
    await renderPage();
    await waitForForm();
    expect(screen.getByRole('button', { name: '创建管理员并登录' })).toBeInTheDocument();
  });
});
