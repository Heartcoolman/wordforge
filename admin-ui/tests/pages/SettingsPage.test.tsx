import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@solidjs/testing-library';
import userEvent from '@testing-library/user-event';
import { renderWithProviders } from '../helpers/render';

// 重设计后的系统设置页:PageHead「系统设置」+ Tabs(配置板块 / RBAC / API 密钥 / 快照 / 维护·备份)。
// 配置板块为默认 tab,左侧分组 rail + 右侧 SectionEditor;维护·备份 tab 含维护模式开关 + 版本门控 + 备份状态。
// 旧版「基本设置 / 维护模式横幅 / .st-skeleton」已不存在,本套测试针对新设计契约。
vi.mock('@/api/admin', () => ({
  adminApi: {
    settingsConfig: vi.fn(),
    settingsSnapshots: vi.fn(),
    rbacAdmins: vi.fn(),
    apiKeys: vi.fn(),
    getSettings: vi.fn(),
    setMaintenance: vi.fn(),
    getBackupStatus: vi.fn(),
    getVersionGate: vi.fn(),
    setVersionGate: vi.fn(),
    putSettingsSection: vi.fn(),
    createSettingsSnapshot: vi.fn(),
    restoreSettingsSnapshot: vi.fn(),
    exportSettingsToml: vi.fn(),
  },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

export const siteJson = {
  brand: 'WordForge', subtitle: '单词锻造', logoUrl: '',
  canonicalUrl: 'https://wordforge.app', adminUrl: 'https://admin.wordforge.app',
  cdnUrl: 'https://cdn.wordforge.app', timezone: 'Asia/Shanghai', language: 'zh-CN',
  theme: 'system', accent: '#5b6dff', seo: 'SEO',
};
export const authJson = {
  jwt: { accessTtlMinutes: 30, refreshTtlDays: 30, rotation: false, algorithm: 'RS256' },
  oauth: { google: true, apple: false, wechat: true, qq: false, github: true, magicLink: true },
  registration: { policy: 'open', allowedDomains: [] },
  twoFactor: { admin: 'required', paid: 'optional', regular: 'optional' },
  lockout: { maxFailures: 5, windowMinutes: 10, lockMinutes: 30 },
  password: { minLength: 10, minZxcvbnScore: 3, historyCount: 5 },
};

export function mockSections() {
  return {
    sections: [
      { section: 'site', json: siteJson, updatedAt: '2026-05-30T10:00:00Z' },
      { section: 'auth', json: authJson, updatedAt: '2026-05-30T10:00:00Z' },
      { section: 'smtp', json: { provider: 'sendgrid', host: 'smtp.sendgrid.net', port: 587, secret: '••••••', from: 'WordForge <noreply@wordforge.app>' }, updatedAt: '2026-05-30T10:00:00Z' },
      { section: 'audit-config', json: { retentionDays: 365, hashChain: true, siem: { target: 'datadog', url: 'https://intake' } }, updatedAt: '2026-05-30T10:00:00Z' },
      { section: 'backup-policy', json: { fullCron: '0 3 * * 0', incrementalCron: '0 */6 * * *', walArchive: true, targets: [{ name: 'primary', uri: 's3://wf-backup', retentionDays: 30 }] }, updatedAt: '2026-05-30T10:00:00Z' },
    ],
  };
}

function defaults() {
  mockApi.settingsConfig.mockResolvedValue(mockSections());
  mockApi.settingsSnapshots.mockResolvedValue({ snapshots: [] });
  mockApi.rbacAdmins.mockResolvedValue({ admins: [] });
  mockApi.apiKeys.mockResolvedValue({ keys: [] });
  mockApi.getSettings.mockResolvedValue({ maintenanceMode: false });
  mockApi.setMaintenance.mockResolvedValue({ active: true });
  mockApi.getBackupStatus.mockResolvedValue({ targets: [] });
  mockApi.getVersionGate.mockResolvedValue({ enabled: false, strictModeEnabled: false });
}

async function renderPage() {
  const { default: SettingsPage } = await import('@/pages/SettingsPage');
  return renderWithProviders(() => <SettingsPage />);
}

describe('SettingsPage — 新设计结构', () => {
  beforeEach(() => { vi.clearAllMocks(); defaults(); });

  it('展示「系统设置」标题', async () => {
    await renderPage();
    expect(screen.getByText('系统设置')).toBeInTheDocument();
  });

  it('配置加载中展示 spinner 占位', async () => {
    mockApi.settingsConfig.mockReturnValue(new Promise(() => {}));
    await renderPage();
    expect(document.querySelectorAll('.spinner').length).toBeGreaterThanOrEqual(1);
  });

  it('加载后展示认证/备份板块标题（仅存配置的 site/smtp/audit 已隐藏）', async () => {
    await renderPage();
    // 「认证与登录」在左侧 rail 与 SectionEditor 卡片各出现一次 → getAllByText
    await waitFor(() => expect(screen.getAllByText('认证与登录').length).toBeGreaterThanOrEqual(1));
    expect(screen.getAllByText('备份策略').length).toBeGreaterThanOrEqual(1);
    // 仅存配置、不生效的 section 已从配置页隐藏
    expect(screen.queryByText('站点与外观')).toBeNull();
  });

  it('切到「维护 · 备份」tab 可切换维护模式并调用 setMaintenance', async () => {
    const user = userEvent.setup();
    await renderPage();
    await waitFor(() => expect(screen.getAllByText('认证与登录').length).toBeGreaterThanOrEqual(1));
    await user.click(screen.getByRole('button', { name: '维护 · 备份' }));
    // 维护模式面板渲染
    await waitFor(() => expect(screen.getAllByText('维护模式').length).toBeGreaterThanOrEqual(1));

    // getSettings 解析后开关启用,第一个 checkbox 即维护模式开关
    let toggle!: HTMLInputElement;
    await waitFor(() => {
      toggle = screen.getAllByRole('checkbox')[0] as HTMLInputElement;
      expect(toggle.disabled).toBe(false);
    });
    await user.click(toggle);
    await waitFor(() => expect(mockApi.setMaintenance).toHaveBeenCalledWith(true));
  });

  it('settingsConfig 失败时展示「加载配置失败」空态', async () => {
    mockApi.settingsConfig.mockRejectedValue(new Error('load fail'));
    await renderPage();
    await waitFor(() => expect(screen.getByText('加载配置失败')).toBeInTheDocument());
  });
});
