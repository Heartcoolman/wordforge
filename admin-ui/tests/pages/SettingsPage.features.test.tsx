import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

// 重设计后的系统设置页交互:典型 section 渲染(品牌预览 / 外观 seg / provider 卡片 /
// 注册策略 seg / SIEM seg)、按 section 保存、连接测试、快照创建、维护模式切换。
vi.mock('@/api/admin', () => ({
  adminApi: {
    settingsConfig: vi.fn(),
    getSettings: vi.fn(),
    putSettingsSection: vi.fn(),
    settingsSnapshots: vi.fn(),
    createSettingsSnapshot: vi.fn(),
    restoreSettingsSnapshot: vi.fn(),
    setMaintenance: vi.fn(),
    rbacAdmins: vi.fn(),
    apiKeys: vi.fn(),
  },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
import { uiStore } from '@/stores/ui';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockToast = uiStore.toast as unknown as Record<string, ReturnType<typeof vi.fn>>;

const siteJson = {
  brand: 'WordForge', subtitle: '单词锻造', logoUrl: '',
  canonicalUrl: 'https://wordforge.app', adminUrl: 'https://admin.wordforge.app',
  cdnUrl: 'https://cdn.wordforge.app', timezone: 'Asia/Shanghai', language: 'zh-CN',
  theme: 'system', accent: '#5b6dff', seo: 'SEO',
};
const authJson = {
  jwt: { accessTtlMinutes: 30, refreshTtlDays: 30, rotation: false, algorithm: 'RS256' },
  oauth: { google: true, apple: false, wechat: true, qq: false, github: true, magicLink: true },
  registration: { policy: 'open', allowedDomains: [] },
  twoFactor: { admin: 'required', paid: 'optional', regular: 'optional' },
  lockout: { maxFailures: 5, windowMinutes: 10, lockMinutes: 30 },
  password: { minLength: 10, minZxcvbnScore: 3, historyCount: 5 },
};

function sectionsResp() {
  return {
    sections: [
      { section: 'site', json: { ...siteJson }, updatedAt: '2026-05-30T10:00:00Z' },
      { section: 'auth', json: JSON.parse(JSON.stringify(authJson)), updatedAt: '2026-05-30T10:00:00Z' },
      { section: 'audit-config', json: { retentionDays: 365, hashChain: true, siem: { target: 'datadog', url: 'https://intake' } }, updatedAt: '2026-05-30T10:00:00Z' },
      { section: 'backup-policy', json: { fullCron: '0 3 * * 0', incrementalCron: '0 */6 * * *', walArchive: true, targets: [{ name: 'primary', uri: 's3://wf-backup', retentionDays: 30 }] }, updatedAt: '2026-05-30T10:00:00Z' },
    ],
  };
}

function defaults() {
  mockApi.settingsConfig.mockResolvedValue(sectionsResp());
  mockApi.getSettings.mockResolvedValue({ maintenanceMode: false });
  mockApi.rbacAdmins.mockResolvedValue({ admins: [] });
  mockApi.apiKeys.mockResolvedValue({ keys: [] });
}

async function renderPage() {
  const { default: Page } = await import('@/pages/SettingsPage');
  return renderWithProviders(() => <Page />);
}

describe('SettingsPage — 专属控件渲染', () => {
  beforeEach(() => { vi.clearAllMocks(); defaults(); });

  it('site 板块渲染品牌预览 + 外观模式 seg + 主色调取色器', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByText('外观模式')).toBeInTheDocument());
    // 品牌预览
    expect(screen.getByText('单词锻造')).toBeInTheDocument();
    // 外观模式 seg 三档
    expect(screen.getByRole('button', { name: '跟随系统' })).toBeInTheDocument();
    // 取色器(input[type=color])
    expect(document.querySelector('input[type="color"]')).toBeTruthy();
    // 对比度 AA pill(#5b6dff 通过 AA)
    expect(screen.getByText(/对比度/)).toBeInTheDocument();
  });

  it('auth 板块渲染第三方登录 provider 卡片网格(6 个)', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByText('第三方登录')).toBeInTheDocument());
    expect(screen.getByText('Google')).toBeInTheDocument();
    expect(screen.getByText('微信')).toBeInTheDocument();
    expect(screen.getByText('魔法链接')).toBeInTheDocument();
    expect(document.querySelectorAll('.prov-card').length).toBe(6);
    // google 默认启用 → is-on;qq 未启用
    const cards = Array.from(document.querySelectorAll('.prov-card')) as HTMLElement[];
    const google = cards.find((c) => c.textContent?.includes('Google'))!;
    expect(google.className).toContain('is-on');
  });

  it('auth 注册策略以 seg 渲染,而非纯文本输入', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByText('注册策略')).toBeInTheDocument());
    expect(screen.getByRole('button', { name: '开放注册' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '仅邀请码' })).toBeInTheDocument();
  });

  it('audit SIEM 以 seg 渲染', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByText('SIEM 转发')).toBeInTheDocument());
    expect(screen.getByRole('button', { name: 'Datadog' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Splunk' })).toBeInTheDocument();
  });

  it('JWT 行不渲染 secret 轮换(诚实降级:密钥来自 env)', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByText('JWT 配置')).toBeInTheDocument());
    expect(screen.queryByRole('button', { name: '轮换' })).not.toBeInTheDocument();
    expect(screen.getByText(/JWT 签名密钥由部署期环境变量管理/)).toBeInTheDocument();
  });
});

describe('SettingsPage — 交互与保存', () => {
  beforeEach(() => { vi.clearAllMocks(); defaults(); });

  it('点击 provider 卡片切换 → 出现未保存,保存调用 putSettingsSection(auth)', async () => {
    mockApi.putSettingsSection.mockImplementation((section: string, json: Record<string, unknown>) =>
      Promise.resolve({ section, json, updatedAt: '2026-05-30T11:00:00Z' }));
    await renderPage();
    await waitFor(() => expect(document.querySelectorAll('.prov-card').length).toBe(6));
    const cards = Array.from(document.querySelectorAll('.prov-card')) as HTMLElement[];
    const qq = cards.find((c) => c.textContent?.includes('QQ'))!;
    fireEvent.click(qq);
    // 切换后 auth 板块标记未保存,等「全部保存」启用后再点击(disabled 按钮不触发 onClick)
    await waitFor(() => expect(screen.getByText('全部保存')).not.toBeDisabled());
    fireEvent.click(screen.getByText('全部保存'));
    await new Promise((r) => setTimeout(r, 80));
    expect(mockApi.putSettingsSection).toHaveBeenCalled();
    const [section, body] = mockApi.putSettingsSection.mock.calls[0];
    // typed section 别名:auth → auth;body 含完整 oauth 且 qq 翻转为 true
    expect(section).toBe('auth');
    expect((body as any).oauth.qq).toBe(true);
    expect((body as any).jwt.algorithm).toBe('RS256');
    expect(mockToast.success).toHaveBeenCalled();
  });

  it('site 改外观模式 seg 后保存,提交完整 site 体且 theme 翻转', async () => {
    mockApi.putSettingsSection.mockImplementation((section: string, json: Record<string, unknown>) =>
      Promise.resolve({ section, json, updatedAt: '2026-05-30T11:00:00Z' }));
    await renderPage();
    await waitFor(() => expect(screen.getByRole('button', { name: 'Dark' })).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: 'Dark' }));
    await waitFor(() => expect(screen.getByText('全部保存')).not.toBeDisabled());
    fireEvent.click(screen.getByText('全部保存'));
    await waitFor(() => expect(mockApi.putSettingsSection).toHaveBeenCalledWith('site', expect.objectContaining({ theme: 'dark', brand: 'WordForge' })));
  });

  it('保存失败时弹错误 toast', async () => {
    mockApi.putSettingsSection.mockRejectedValue(new Error('save fail'));
    await renderPage();
    await waitFor(() => expect(screen.getByRole('button', { name: 'Dark' })).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: 'Dark' }));
    await waitFor(() => expect(screen.getByText('全部保存')).not.toBeDisabled());
    fireEvent.click(screen.getByText('全部保存'));
    await waitFor(() => expect(mockToast.error).toHaveBeenCalled());
  });

  it('创建快照调用 createSettingsSnapshot', async () => {
    mockApi.createSettingsSnapshot.mockResolvedValue({ id: 7, label: null, createdAt: '2026-05-30T11:00:00Z' });
    await renderPage();
    await waitFor(() => expect(screen.getByText('创建快照')).toBeInTheDocument());
    fireEvent.click(screen.getByText('创建快照'));
    await waitFor(() => expect(mockApi.createSettingsSnapshot).toHaveBeenCalled());
    expect(mockToast.success).toHaveBeenCalled();
  });

  it('开启维护模式走二次确认后调用 setMaintenance(true)', async () => {
    mockApi.setMaintenance.mockResolvedValue({ active: true });
    await renderPage();
    await waitFor(() => expect(screen.getByText('进入维护模式')).toBeInTheDocument());
    fireEvent.click(screen.getByText('进入维护模式'));
    await waitFor(() => expect(screen.getByText('确认开启维护模式')).toBeInTheDocument());
    // 横幅按钮 + 对话框确认按钮都叫「进入维护模式」,点对话框里那个(最后一个)
    const confirmBtns = screen.getAllByText('进入维护模式');
    fireEvent.click(confirmBtns[confirmBtns.length - 1]);
    await waitFor(() => expect(mockApi.setMaintenance).toHaveBeenCalledWith(true));
  });
});
