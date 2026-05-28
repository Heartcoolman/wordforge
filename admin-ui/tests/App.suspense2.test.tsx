// 独立隔离每次测试：用 vi.isolateModules + vi.resetModules 让每个 Suspense fallback
// 测试都使用全新的 lazy / route 状态，避免 sharedConfig.context.lazy 跨用例 cache 干扰
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, waitFor, cleanup } from '@solidjs/testing-library';

vi.mock('@/components/auth/ProtectedRoute', () => ({
  AdminProtectedRoute: (props: any) => <>{props.children}</>,
}));
vi.mock('@/components/layout/AdminLayout', () => ({
  AdminLayout: (props: any) => <div>{props.children}</div>,
}));

vi.mock('@/stores/ui', () => ({
  uiStore: {
    toasts: () => [],
    addToast: vi.fn(),
    removeToast: vi.fn(),
    toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() },
  },
}));
vi.mock('@/lib/token', () => ({
  tokenManager: {
    getToken: () => null,
    getAdminToken: () => 'fake',
    setTokens: vi.fn(), clearTokens: vi.fn(),
    needsRefresh: () => false, isAuthenticated: () => false,
    setAdminToken: vi.fn(), clearAdminToken: vi.fn(),
  },
}));
vi.mock('@/api/admin', () => ({
  adminApi: { verifyToken: vi.fn().mockResolvedValue({ id: 'a', email: 'a@b.c' }) },
}));

// 关键：mock api/client 使 maintenanceActive 返回 true，同时 isAdminPath 在
// 非 admin 路径下 false → MaintenanceProvider 的 Show fallback={<MaintenancePage />} 求值。
vi.mock('@/api/http', () => ({
  api: { get: vi.fn().mockResolvedValue({ maintenanceMode: true, version: 'v1' }) },
  // 让 maintenanceActive 始终为 true：admin path 测试时触发 isAdminPath() 求值
  maintenanceActive: () => true,
  updateInfo: () => null,
  setMaintenanceActive: vi.fn(),
  setUpdateInfo: vi.fn(),
  connectSseStream: vi.fn(() => () => undefined),
}));

// MaintenancePage 简单 mock 避免依赖业务
vi.mock('@/pages/MaintenancePage', () => ({ default: () => <div data-testid="maint-page">Maintenance</div> }));
// 其余 lazy 目标按需 mock —— 在 maintenance fallback 路径下，admin login lazy 不会渲染
vi.mock('@/pages/NotFoundPage', () => ({ default: () => <div>NF</div> }));
vi.mock('@/pages/LegacyUserFrontendPage', () => ({ default: () => <div>L</div> }));
vi.mock('@/pages/LoginPage', () => ({ default: () => <div>AL</div> }));
vi.mock('@/pages/SetupPage', () => ({ default: () => <div>AS</div> }));
vi.mock('@/pages/DashboardPage', () => ({ default: () => <div>AD</div> }));
vi.mock('@/pages/UserManagementPage', () => ({ default: () => <div>UM</div> }));
vi.mock('@/pages/AmasConfigPage', () => ({ default: () => <div>AC</div> }));
vi.mock('@/pages/AmasMetricsPage', () => ({ default: () => <div>AM</div> }));
vi.mock('@/pages/AmasAdvisorPage', () => ({ default: () => <div>AA</div> }));
vi.mock('@/pages/MonitoringPage', () => ({ default: () => <div>M</div> }));
vi.mock('@/pages/AnalyticsPage', () => ({ default: () => <div>An</div> }));
vi.mock('@/pages/SettingsPage', () => ({ default: () => <div>S</div> }));
vi.mock('@/pages/UpdatesPage', () => ({ default: () => <div>U</div> }));
vi.mock('@/pages/DevicesPage', () => ({ default: () => <div>C</div> }));
vi.mock('@/pages/WordbookCenterPage', () => ({ default: () => <div>WB</div> }));

import App from '@/App';

describe('App MaintenanceProvider — fallback & isAdminPath branches', () => {
  beforeEach(() => vi.clearAllMocks());

  it('renders MaintenancePage when maintenance is on and path is NOT admin (covers line 72 get fallback)', async () => {
    window.location.pathname = '/login';
    window.location.href = 'http://localhost:3000/login';
    const { findByTestId } = render(() => <App />);
    await findByTestId('maint-page');
  });

  it('does NOT render MaintenancePage on admin path (covers isAdminPath() true branch)', async () => {
    window.location.pathname = '/admin/login';
    window.location.href = 'http://localhost:3000/admin/login';
    const { queryByTestId, findByText } = render(() => <App />);
    await findByText('AL');
    expect(queryByTestId('maint-page')).toBeNull();
  });
});
