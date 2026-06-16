import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

// 重设计后的系统设置页(SettingsPage)采用 Tabs + 通用 section 编辑器结构:
// 配置板块(左侧分组导航 + 键值编辑器)/ 管理员 RBAC / API 密钥 / 快照 / 维护·备份。
// 本测试覆盖:section 导航与编辑器渲染、按 section 保存(putSettingsSection)、
// 快照创建(createSettingsSnapshot)、维护模式开关(setMaintenance)、TOML 导出。
vi.mock('@/api/admin', () => ({
  adminApi: {
    settingsConfig: vi.fn(),
    settingsSnapshots: vi.fn(),
    rbacAdmins: vi.fn(),
    apiKeys: vi.fn(),
    putSettingsSection: vi.fn(),
    createSettingsSnapshot: vi.fn(),
    restoreSettingsSnapshot: vi.fn(),
    exportSettingsToml: vi.fn(),
    createRbacAdmin: vi.fn(),
    updateRbacAdminRole: vi.fn(),
    deleteRbacAdmin: vi.fn(),
    createApiKey: vi.fn(),
    rotateApiKey: vi.fn(),
    deleteApiKey: vi.fn(),
    getBackupStatus: vi.fn(),
    getVersionGate: vi.fn(),
    setVersionGate: vi.fn(),
    getSettings: vi.fn(),
    setMaintenance: vi.fn(),
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
  canonicalUrl: 'https://wordforge.app', cdnUrl: 'https://cdn.wordforge.app',
  timezone: 'Asia/Shanghai', language: 'zh-CN', theme: 'system',
};
const authJson = {
  jwt: { accessTtlMinutes: 30, refreshTtlDays: 30, rotation: false, algorithm: 'RS256' },
  oauth: { google: true, apple: false, wechat: true, qq: false, github: true, magicLink: true },
  registration: { policy: 'open', allowedDomains: [] },
};

function sectionsResp() {
  return {
    sections: [
      { section: 'site', json: { ...siteJson }, updatedAt: '2026-05-30T10:00:00Z' },
      { section: 'auth', json: JSON.parse(JSON.stringify(authJson)), updatedAt: '2026-05-30T10:00:00Z' },
      { section: 'audit-config', json: { retentionDays: 365, hashChain: true }, updatedAt: '2026-05-30T10:00:00Z' },
      { section: 'backup-policy', json: { fullCron: '0 3 * * 0', walArchive: true }, updatedAt: '2026-05-30T10:00:00Z' },
    ],
  };
}

function defaults() {
  mockApi.settingsConfig.mockResolvedValue(sectionsResp());
  mockApi.settingsSnapshots.mockResolvedValue({ snapshots: [] });
  mockApi.rbacAdmins.mockResolvedValue({ admins: [] });
  mockApi.apiKeys.mockResolvedValue({ keys: [] });
  mockApi.getBackupStatus.mockResolvedValue({ targets: [] });
  mockApi.getVersionGate.mockResolvedValue({ enabled: false, minClientVersion: null, strictModeEnabled: false });
  mockApi.getSettings.mockResolvedValue({ maintenanceMode: false });
  mockApi.setMaintenance.mockResolvedValue({ active: true });
  mockApi.setVersionGate.mockResolvedValue(undefined);
  mockApi.exportSettingsToml.mockResolvedValue('[site]\nbrand = "WordForge"\n');
}

async function renderPage() {
  const { default: Page } = await import('@/pages/SettingsPage');
  return renderWithProviders(() => <Page />);
}

describe('SettingsPage — 板块编辑器渲染', () => {
  beforeEach(() => { vi.clearAllMocks(); defaults(); });

  it('页头与 Tabs 渲染:系统设置标题 + 五个分页', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByText('系统设置')).toBeInTheDocument());
    expect(screen.getByRole('button', { name: '配置板块' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /管理员 RBAC/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /API 密钥/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /快照/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '维护 · 备份' })).toBeInTheDocument();
  });

  it('config 默认高亮首个可见 section(认证),编辑器显示「认证与登录 配置」', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByText('认证与登录 配置')).toBeInTheDocument());
    // 仅保留生效 section 后的分组标签
    expect(screen.getByText('认证 · 安全')).toBeInTheDocument();
    expect(screen.getByText('审计 · 备份')).toBeInTheDocument();
    // section 元信息回显
    expect(screen.getByText(/section: auth/)).toBeInTheDocument();
    // 仅存配置、不生效的 site / audit-config 已从配置页隐藏
    expect(screen.queryByText('站点与外观')).toBeNull();
  });

  it('认证板块顶层键(jwt/oauth/registration)以字段渲染', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByText('认证与登录 配置')).toBeInTheDocument());
    expect(screen.getByText('jwt')).toBeInTheDocument();
    expect(screen.getByText('oauth')).toBeInTheDocument();
    expect(screen.getByText('registration')).toBeInTheDocument();
  });

  it('切换到备份策略,字段按类型渲染(string→输入框、boolean→开关)', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByText('认证与登录 配置')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '备份策略' }));
    await waitFor(() => expect(screen.getByText('备份策略 配置')).toBeInTheDocument());
    expect(screen.getByText(/section: backup-policy/)).toBeInTheDocument();
    // fullCron(string) 文本输入回显当前值;walArchive(boolean) 字段标签出现
    expect(screen.getByText('walArchive')).toBeInTheDocument();
    const inputs = Array.from(document.querySelectorAll('input.input')) as HTMLInputElement[];
    expect(inputs.some((i) => i.value === '0 3 * * 0')).toBe(true);
  });
});

describe('SettingsPage — 交互与保存', () => {
  beforeEach(() => { vi.clearAllMocks(); defaults(); });

  it('未改动时保存按钮禁用,改字段后启用并调用 putSettingsSection(backup-policy)', async () => {
    mockApi.putSettingsSection.mockImplementation((section: string, json: Record<string, unknown>) =>
      Promise.resolve({ section, json, updatedAt: '2026-05-30T11:00:00Z' }));
    await renderPage();
    await waitFor(() => expect(screen.getByText('认证与登录 配置')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '备份策略' }));
    await waitFor(() => expect(screen.getByText('备份策略 配置')).toBeInTheDocument());
    const saveBtn = screen.getByRole('button', { name: '保存配置' }) as HTMLButtonElement;
    expect(saveBtn).toBeDisabled();
    // 改 fullCron 文本字段
    const inputs = Array.from(document.querySelectorAll('input.input')) as HTMLInputElement[];
    const cronInput = inputs.find((i) => i.value === '0 3 * * 0')!;
    fireEvent.input(cronInput, { target: { value: '0 4 * * 0' } });
    // 出现未保存徽标 + 保存按钮启用
    await waitFor(() => expect(screen.getByText('未保存')).toBeInTheDocument());
    await waitFor(() => expect(screen.getByRole('button', { name: '保存配置' })).not.toBeDisabled());
    fireEvent.click(screen.getByRole('button', { name: '保存配置' }));
    await waitFor(() => expect(mockApi.putSettingsSection).toHaveBeenCalledWith('backup-policy', expect.objectContaining({ fullCron: '0 4 * * 0' })));
    expect(mockToast.success).toHaveBeenCalled();
  });

  it('保存失败时弹错误 toast', async () => {
    mockApi.putSettingsSection.mockRejectedValue(new Error('save fail'));
    await renderPage();
    await waitFor(() => expect(screen.getByText('认证与登录 配置')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '备份策略' }));
    await waitFor(() => expect(screen.getByText('备份策略 配置')).toBeInTheDocument());
    const inputs = Array.from(document.querySelectorAll('input.input')) as HTMLInputElement[];
    const cronInput = inputs.find((i) => i.value === '0 3 * * 0')!;
    fireEvent.input(cronInput, { target: { value: '0 4 * * 0' } });
    await waitFor(() => expect(screen.getByRole('button', { name: '保存配置' })).not.toBeDisabled());
    fireEvent.click(screen.getByRole('button', { name: '保存配置' }));
    await waitFor(() => expect(mockToast.error).toHaveBeenCalled());
  });

  it('快照分页创建快照调用 createSettingsSnapshot', async () => {
    mockApi.createSettingsSnapshot.mockResolvedValue({ id: 7, label: null, createdAt: '2026-05-30T11:00:00Z' });
    await renderPage();
    await waitFor(() => expect(screen.getByText('系统设置')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: /快照/ }));
    // 快照面板的「创建快照」按钮
    await waitFor(() => expect(screen.getByRole('button', { name: '创建快照' })).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '创建快照' }));
    await waitFor(() => expect(mockApi.createSettingsSnapshot).toHaveBeenCalled());
    expect(mockToast.success).toHaveBeenCalled();
  });

  it('维护分页切换维护开关调用 setMaintenance(true)', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByText('系统设置')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '维护 · 备份' }));
    // 维护模式面板:等待 sub 文案渲染,确认面板已挂载
    await waitFor(() => expect(screen.getByText('开启后非管理员将看到维护页')).toBeInTheDocument());
    // 维护模式 Switch = 该面板内第一个 .switch checkbox;getSettings 初值加载后 disabled 解除
    await waitFor(() => {
      const sw = document.querySelector('.switch input[type="checkbox"]') as HTMLInputElement | null;
      expect(sw).toBeTruthy();
      expect(sw!.disabled).toBe(false);
    });
    const sw = document.querySelector('.switch input[type="checkbox"]') as HTMLInputElement;
    fireEvent.change(sw, { target: { checked: true } });
    await waitFor(() => expect(mockApi.setMaintenance).toHaveBeenCalledWith(true));
    expect(mockToast.warning).toHaveBeenCalled();
  });

  it('点「导出 TOML」调用 exportSettingsToml 并提示成功', async () => {
    // 该路径会 URL.createObjectURL 触发下载,happy-dom 无此 API → 打桩
    Object.defineProperty(URL, 'createObjectURL', { configurable: true, value: vi.fn(() => 'blob:x') });
    Object.defineProperty(URL, 'revokeObjectURL', { configurable: true, value: vi.fn() });
    await renderPage();
    await waitFor(() => expect(screen.getByRole('button', { name: '导出 TOML' })).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '导出 TOML' }));
    await waitFor(() => expect(mockApi.exportSettingsToml).toHaveBeenCalled());
    expect(mockToast.success).toHaveBeenCalled();
  });
});
