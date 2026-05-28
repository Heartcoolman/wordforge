import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: {
    getSettings: vi.fn(),
    updateSettings: vi.fn(),
    setMaintenance: vi.fn(),
    broadcast: vi.fn(),
    broadcastUpdate: vi.fn(),
  },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
import { uiStore } from '@/stores/ui';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockToast = uiStore.toast as unknown as Record<string, ReturnType<typeof vi.fn>>;

const baseSettings = {
  maxUsers: 100,
  registrationEnabled: true,
  maintenanceMode: false,
  defaultDailyWords: 20,
  wordbookCenterUrl: '',
  amasAutoApplyEnabled: false,
  amasAutoApplyMaxPerDay: 1,
  amasAutoApplyMinConfidence: 0.8,
};

async function renderPage() {
  const { default: Page } = await import('@/pages/SettingsPage');
  return renderWithProviders(() => <Page />);
}

describe('SettingsPage — load, save, validation, broadcast, maintenance', () => {
  beforeEach(() => vi.clearAllMocks());

  it('loads and renders settings form', async () => {
    mockApi.getSettings.mockResolvedValue(baseSettings);
    await renderPage();
    await waitFor(() => expect(screen.getByText('系统设置')).toBeInTheDocument());
    expect(screen.getByText('基本设置')).toBeInTheDocument();
    expect(screen.getByText('AMAS 调参自动化')).toBeInTheDocument();
    expect(screen.getByText('广播消息')).toBeInTheDocument();
    expect(screen.getByText('更新通知')).toBeInTheDocument();
  });

  it('shows error toast on getSettings failure', async () => {
    mockApi.getSettings.mockRejectedValue(new Error('load fail'));
    await renderPage();
    await waitFor(() => expect(mockToast.error).toHaveBeenCalled());
  });

  it('saves settings successfully', async () => {
    mockApi.getSettings.mockResolvedValue(baseSettings);
    mockApi.updateSettings.mockResolvedValue(undefined);
    await renderPage();
    await waitFor(() => expect(screen.getAllByText('保存设置')[0]).toBeInTheDocument());
    fireEvent.click(screen.getAllByText('保存设置')[0]);
    await waitFor(() => expect(mockApi.updateSettings).toHaveBeenCalled());
    expect(mockToast.success).toHaveBeenCalled();
  });

  it('shows save failure toast', async () => {
    mockApi.getSettings.mockResolvedValue(baseSettings);
    mockApi.updateSettings.mockRejectedValue(new Error('save'));
    await renderPage();
    await waitFor(() => expect(screen.getAllByText('保存设置')[0]).toBeInTheDocument());
    fireEvent.click(screen.getAllByText('保存设置')[0]);
    await waitFor(() => expect(mockToast.error).toHaveBeenCalled());
  });

  it('blocks save when maxUsers out of range', async () => {
    mockApi.getSettings.mockResolvedValue({ ...baseSettings, maxUsers: 0 });
    await renderPage();
    await waitFor(() => expect(screen.getAllByText('保存设置')[0]).toBeInTheDocument());
    fireEvent.click(screen.getAllByText('保存设置')[0]);
    await waitFor(() => expect(mockToast.warning).toHaveBeenCalled());
  });

  it('blocks save when defaultDailyWords out of range', async () => {
    mockApi.getSettings.mockResolvedValue({ ...baseSettings, defaultDailyWords: 0 });
    await renderPage();
    await waitFor(() => expect(screen.getAllByText('保存设置')[0]).toBeInTheDocument());
    fireEvent.click(screen.getAllByText('保存设置')[0]);
    await waitFor(() => expect(mockToast.warning).toHaveBeenCalled());
  });

  it('shows warning when broadcast fields empty', async () => {
    mockApi.getSettings.mockResolvedValue(baseSettings);
    await renderPage();
    await waitFor(() => expect(screen.getByText('发送广播')).toBeInTheDocument());
    fireEvent.click(screen.getByText('发送广播'));
    await waitFor(() => expect(mockToast.warning).toHaveBeenCalledWith('请填写标题和内容'));
  });

  it('sends broadcast successfully via confirm', async () => {
    mockApi.getSettings.mockResolvedValue(baseSettings);
    mockApi.broadcast.mockResolvedValue({ sent: 5 });
    await renderPage();
    await waitFor(() => expect(screen.getByText('发送广播')).toBeInTheDocument());
    fireEvent.input(screen.getByPlaceholderText('通知标题'), { target: { value: '维护通知' } });
    fireEvent.input(screen.getByPlaceholderText('通知内容'), { target: { value: '今晚 10 点维护' } });
    fireEvent.click(screen.getByText('发送广播'));
    await waitFor(() => expect(screen.getByText('确认发送广播')).toBeInTheDocument());
    fireEvent.click(screen.getByText('确认发送'));
    await waitFor(() => expect(mockApi.broadcast).toHaveBeenCalled());
  });

  it('cancels broadcast confirm dialog', async () => {
    mockApi.getSettings.mockResolvedValue(baseSettings);
    await renderPage();
    await waitFor(() => expect(screen.getByText('发送广播')).toBeInTheDocument());
    fireEvent.input(screen.getByPlaceholderText('通知标题'), { target: { value: 't' } });
    fireEvent.input(screen.getByPlaceholderText('通知内容'), { target: { value: 'm' } });
    fireEvent.click(screen.getByText('发送广播'));
    await waitFor(() => expect(screen.getByText('确认发送广播')).toBeInTheDocument());
    fireEvent.click(screen.getByText('取消'));
    await waitFor(() => expect(screen.queryByText('确认发送广播')).not.toBeInTheDocument());
  });

  it('handles broadcast failure', async () => {
    mockApi.getSettings.mockResolvedValue(baseSettings);
    mockApi.broadcast.mockRejectedValue(new Error('boom'));
    await renderPage();
    await waitFor(() => expect(screen.getByText('发送广播')).toBeInTheDocument());
    fireEvent.input(screen.getByPlaceholderText('通知标题'), { target: { value: 't' } });
    fireEvent.input(screen.getByPlaceholderText('通知内容'), { target: { value: 'm' } });
    fireEvent.click(screen.getByText('发送广播'));
    fireEvent.click(screen.getByText('确认发送'));
    await waitFor(() => expect(mockToast.error).toHaveBeenCalled());
  });

  it('sends update notification with default message', async () => {
    mockApi.getSettings.mockResolvedValue(baseSettings);
    mockApi.broadcastUpdate.mockResolvedValue(undefined);
    await renderPage();
    await waitFor(() => expect(screen.getByText('发送更新通知')).toBeInTheDocument());
    fireEvent.click(screen.getByText('发送更新通知'));
    await waitFor(() => expect(screen.getByText('确认发送更新通知')).toBeInTheDocument());
    fireEvent.click(screen.getByText('确认发送'));
    await waitFor(() => expect(mockApi.broadcastUpdate).toHaveBeenCalled());
  });

  it('handles update notification failure', async () => {
    mockApi.getSettings.mockResolvedValue(baseSettings);
    mockApi.broadcastUpdate.mockRejectedValue(new Error('fail'));
    await renderPage();
    await waitFor(() => expect(screen.getByText('发送更新通知')).toBeInTheDocument());
    fireEvent.click(screen.getByText('发送更新通知'));
    fireEvent.click(screen.getByText('确认发送'));
    await waitFor(() => expect(mockToast.error).toHaveBeenCalled());
  });

  it('toggles maintenance mode with confirm and calls setMaintenance API', async () => {
    mockApi.getSettings.mockResolvedValue(baseSettings);
    mockApi.setMaintenance.mockResolvedValue({ active: true });
    await renderPage();
    await waitFor(() => expect(screen.getByText('维护模式')).toBeInTheDocument());
    const switches = document.querySelectorAll('button[role="switch"]');
    fireEvent.click(switches[1] as HTMLButtonElement);
    await waitFor(() => expect(screen.getByText('确认开启维护模式')).toBeInTheDocument());
    fireEvent.click(screen.getByText('确认开启'));
    await waitFor(() => expect(mockApi.setMaintenance).toHaveBeenCalledWith(true));
    expect(mockToast.success).toHaveBeenCalled();
  });

  it('cancels maintenance confirm', async () => {
    mockApi.getSettings.mockResolvedValue(baseSettings);
    await renderPage();
    await waitFor(() => expect(screen.getByText('维护模式')).toBeInTheDocument());
    const switches = document.querySelectorAll('button[role="switch"]');
    fireEvent.click(switches[1]);
    await waitFor(() => expect(screen.getByText('确认开启维护模式')).toBeInTheDocument());
    fireEvent.click(screen.getByText('取消'));
    await waitFor(() => expect(screen.queryByText('确认开启维护模式')).not.toBeInTheDocument());
  });

  it('shows error toast when setMaintenance fails', async () => {
    mockApi.getSettings.mockResolvedValue(baseSettings);
    mockApi.setMaintenance.mockRejectedValue(new Error('网络错误'));
    await renderPage();
    await waitFor(() => expect(screen.getByText('维护模式')).toBeInTheDocument());
    const switches = document.querySelectorAll('button[role="switch"]');
    fireEvent.click(switches[1]);
    await waitFor(() => expect(screen.getByText('确认开启维护模式')).toBeInTheDocument());
    fireEvent.click(screen.getByText('确认开启'));
    await waitFor(() => expect(mockToast.error).toHaveBeenCalled());
  });
});

describe('SettingsPage — input handlers & AMAS toggles', () => {
  beforeEach(() => vi.clearAllMocks());

  it('updates maxUsers / defaultDailyWords / wordbookCenterUrl via input', async () => {
    mockApi.getSettings.mockResolvedValue(baseSettings);
    await renderPage();
    await waitFor(() => expect(screen.getByLabelText('最大用户数')).toBeInTheDocument());
    fireEvent.input(screen.getByLabelText('最大用户数'), { target: { value: '500' } });
    fireEvent.input(screen.getByLabelText('默认每日单词数'), { target: { value: '50' } });
    fireEvent.input(screen.getByLabelText('词书中心 URL'), { target: { value: 'https://x.example' } });
    mockApi.updateSettings.mockResolvedValue(undefined);
    fireEvent.click(screen.getAllByText('保存设置')[0]);
    await waitFor(() => expect(mockApi.updateSettings).toHaveBeenCalled());
    const payload = mockApi.updateSettings.mock.calls[0][0];
    expect(payload.maxUsers).toBe(500);
    expect(payload.defaultDailyWords).toBe(50);
    expect(payload.wordbookCenterUrl).toBe('https://x.example');
  });

  it('toggles registration switch', async () => {
    mockApi.getSettings.mockResolvedValue(baseSettings);
    await renderPage();
    await waitFor(() => expect(screen.getByText('开放注册')).toBeInTheDocument());
    const switches = document.querySelectorAll('button[role="switch"]');
    fireEvent.click(switches[0] as HTMLButtonElement);
    mockApi.updateSettings.mockResolvedValue(undefined);
    fireEvent.click(screen.getAllByText('保存设置')[0]);
    await waitFor(() => expect(mockApi.updateSettings).toHaveBeenCalled());
    expect(mockApi.updateSettings.mock.calls[0][0].registrationEnabled).toBe(false);
  });

  it('disables maintenance via switch (skips confirm, calls setMaintenance immediately)', async () => {
    mockApi.getSettings.mockResolvedValue({ ...baseSettings, maintenanceMode: true });
    mockApi.setMaintenance.mockResolvedValue({ active: false });
    await renderPage();
    await waitFor(() => expect(screen.getByText('维护模式')).toBeInTheDocument());
    const switches = document.querySelectorAll('button[role="switch"]');
    fireEvent.click(switches[1] as HTMLButtonElement);
    expect(screen.queryByText('确认开启维护模式')).not.toBeInTheDocument();
    await waitFor(() => expect(mockApi.setMaintenance).toHaveBeenCalledWith(false));
    expect(mockToast.success).toHaveBeenCalled();
  });

  it('toggles amasAutoApplyEnabled and updates numeric AMAS inputs', async () => {
    mockApi.getSettings.mockResolvedValue(baseSettings);
    await renderPage();
    await waitFor(() => expect(screen.getByLabelText('每日最多自动应用次数')).toBeInTheDocument());
    const switches = document.querySelectorAll('button[role="switch"]');
    fireEvent.click(switches[2] as HTMLButtonElement);
    fireEvent.input(screen.getByLabelText('每日最多自动应用次数'), { target: { value: '5' } });
    fireEvent.input(screen.getByLabelText('最低置信度阈值'), { target: { value: '0.95' } });
    mockApi.updateSettings.mockResolvedValue(undefined);
    fireEvent.click(screen.getAllByText('保存设置')[1]);
    await waitFor(() => expect(mockApi.updateSettings).toHaveBeenCalled());
    const p = mockApi.updateSettings.mock.calls[0][0];
    expect(p.amasAutoApplyEnabled).toBe(true);
    expect(p.amasAutoApplyMaxPerDay).toBe(5);
    expect(p.amasAutoApplyMinConfidence).toBe(0.95);
  });

  it('updateMsg textarea input is sent with broadcastUpdate', async () => {
    mockApi.getSettings.mockResolvedValue(baseSettings);
    mockApi.broadcastUpdate.mockResolvedValue(undefined);
    await renderPage();
    await waitFor(() => expect(screen.getByText('发送更新通知')).toBeInTheDocument());
    const ta = screen.getByPlaceholderText('有新版本可用，请刷新页面获取最新内容') as HTMLTextAreaElement;
    fireEvent.input(ta, { target: { value: '紧急更新' } });
    fireEvent.click(screen.getByText('发送更新通知'));
    fireEvent.click(screen.getByText('确认发送'));
    await waitFor(() => expect(mockApi.broadcastUpdate).toHaveBeenCalledWith({ message: '紧急更新' }));
  });
});
