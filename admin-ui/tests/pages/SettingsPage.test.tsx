import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

// 重设计后的系统设置页:section 化(site/auth/…)+ RBAC + API 密钥 + 快照/回滚/导出。
// 旧版「基本设置 / AMAS 调参 / 广播迁出」表单已不存在,本套测试针对新设计契约。
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
  mockApi.getSettings.mockResolvedValue({ maintenanceMode: false });
  mockApi.rbacAdmins.mockResolvedValue({ admins: [] });
  mockApi.apiKeys.mockResolvedValue({ keys: [] });
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

  it('加载中展示 skeleton 占位', async () => {
    mockApi.settingsConfig.mockReturnValue(new Promise(() => {}));
    await renderPage();
    expect(document.querySelectorAll('.st-skeleton').length).toBeGreaterThanOrEqual(1);
  });

  it('加载后展示站点/认证板块标题', async () => {
    await renderPage();
    // 「站点与外观」在 rail 与 section card 各出现一次 → getAllByText
    await waitFor(() => expect(screen.getAllByText('站点与外观').length).toBeGreaterThanOrEqual(1));
    expect(screen.getAllByText('认证与登录').length).toBeGreaterThanOrEqual(1);
  });

  it('展示维护模式横幅', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByText('系统运行中 · 维护模式已关闭')).toBeInTheDocument());
  });

  it('settingsConfig 失败时弹错误 toast', async () => {
    mockApi.settingsConfig.mockRejectedValue(new Error('load fail'));
    const { uiStore } = await import('@/stores/ui');
    await renderPage();
    await waitFor(() => expect((uiStore.toast.error as ReturnType<typeof vi.fn>)).toHaveBeenCalled());
  });
});
