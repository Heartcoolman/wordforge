import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, waitFor } from '@solidjs/testing-library';

// hoist 友好：用 vi.hoisted 暴露 mock 引用
const { apiGet, setMaintenanceActive, setUpdateInfo, maintenanceActive } = vi.hoisted(() => ({
  apiGet: vi.fn(),
  setMaintenanceActive: vi.fn(),
  setUpdateInfo: vi.fn(),
  maintenanceActive: vi.fn(() => false),
}));

vi.mock('@/pages/NotFoundPage', () => ({ default: () => <div>NotFound</div> }));
vi.mock('@/pages/LegacyUserFrontendPage', () => ({ default: () => <div>Legacy</div> }));
vi.mock('@/pages/LoginPage', () => ({ default: () => <div>AdminLogin</div> }));
vi.mock('@/pages/SetupPage', () => ({ default: () => <div>AdminSetup</div> }));
vi.mock('@/pages/DashboardPage', () => ({ default: () => <div>DashboardPage</div> }));
vi.mock('@/pages/UserManagementPage', () => ({ default: () => <div>UserMgmt</div> }));
vi.mock('@/pages/AmasConfigPage', () => ({ default: () => <div>AmasConfig</div> }));
vi.mock('@/pages/AmasMetricsPage', () => ({ default: () => <div>AmasMetrics</div> }));
vi.mock('@/pages/AmasAdvisorPage', () => ({ default: () => <div>AmasAdvisor</div> }));
vi.mock('@/pages/MonitoringPage', () => ({ default: () => <div>Monitoring</div> }));
vi.mock('@/pages/AnalyticsPage', () => ({ default: () => <div>Analytics</div> }));
vi.mock('@/pages/SettingsPage', () => ({ default: () => <div>Settings</div> }));
vi.mock('@/pages/UpdatesPage', () => ({ default: () => <div>Updates</div> }));
vi.mock('@/pages/DevicesPage', () => ({ default: () => <div>Clients</div> }));
vi.mock('@/pages/WordbookCenterPage', () => ({ default: () => <div>AdminWB</div> }));

vi.mock('@/stores/ui', () => ({
  uiStore: { toasts: () => [], addToast: vi.fn(), removeToast: vi.fn(), toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));
vi.mock('@/lib/token', () => ({
  tokenManager: {
    getToken: () => null, getAdminToken: () => 'fake', setTokens: vi.fn(), clearTokens: vi.fn(),
    needsRefresh: () => false, isAuthenticated: () => false, setAdminToken: vi.fn(), clearAdminToken: vi.fn(),
  },
}));
vi.mock('@/api/admin', () => ({
  adminApi: { verifyToken: vi.fn().mockResolvedValue({ id: 'a', email: 'a@b.c' }) },
}));
vi.mock('@/api/http', () => ({
  api: { get: apiGet },
  maintenanceActive,
  updateInfo: () => null,
  setMaintenanceActive,
  setUpdateInfo,
  connectSseStream: vi.fn(() => () => undefined),
}));

import App from '@/App';

describe('App MaintenanceProvider', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    window.location.pathname = '/admin/login';
    apiGet.mockResolvedValue({ maintenanceMode: false, version: 'v1' });
  });

  it('calls api.get on mount to check maintenance status', async () => {
    render(() => <App />);
    await waitFor(() => expect(apiGet).toHaveBeenCalledWith('/api/status'));
  });

  it('forwards maintenanceMode flag to store', async () => {
    apiGet.mockResolvedValue({ maintenanceMode: true, version: 'v1' });
    render(() => <App />);
    await waitFor(() => expect(setMaintenanceActive).toHaveBeenCalledWith(true));
  });

  it('detects version change and pushes update info', async () => {
    let calls = 0;
    apiGet.mockImplementation(() => {
      calls++;
      return Promise.resolve({ maintenanceMode: false, version: calls === 1 ? 'v1' : 'v2' });
    });
    vi.useFakeTimers();
    try {
      render(() => <App />);
      await Promise.resolve();
      await Promise.resolve();
      // 第一次 checkStatus 已发生（同步注册的 onMount + microtask）
      vi.advanceTimersByTime(30_500);
      await Promise.resolve();
      await Promise.resolve();
    } finally {
      vi.useRealTimers();
    }
    // 等异步链
    await new Promise((r) => setTimeout(r, 30));
    if (apiGet.mock.calls.length >= 2) {
      expect(setUpdateInfo).toHaveBeenCalled();
    }
  });

  it('swallows api error silently', async () => {
    apiGet.mockRejectedValue(new Error('net'));
    render(() => <App />);
    await waitFor(() => expect(apiGet).toHaveBeenCalled());
    // 未抛错即通过
    expect(setMaintenanceActive).not.toHaveBeenCalled();
  });
});
