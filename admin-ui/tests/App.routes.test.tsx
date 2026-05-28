import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, waitFor, cleanup } from '@solidjs/testing-library';

// 与 App.extra/App.test 保持一致的 mock 集合：
// 关键是覆盖 App.tsx 内所有 lazy(() => import('@/pages/...')) 工厂
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

// AdminProtectedRoute 放行：直接渲染 children，避免 token / 权限校验阻断 lazy 工厂
vi.mock('@/components/auth/ProtectedRoute', () => ({
  AdminProtectedRoute: (props: any) => <>{props.children}</>,
}));
// AdminLayout 透传：保持路由树可达
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
    getAdminToken: () => 'fake-admin',
    setTokens: vi.fn(),
    clearTokens: vi.fn(),
    needsRefresh: () => false,
    isAuthenticated: () => false,
    setAdminToken: vi.fn(),
    clearAdminToken: vi.fn(),
  },
}));
vi.mock('@/api/admin', () => ({
  adminApi: { verifyToken: vi.fn().mockResolvedValue({ id: 'a', email: 'a@b.c' }) },
}));
vi.mock('@/api/http', () => ({
  api: { get: vi.fn().mockResolvedValue({ maintenanceMode: false, version: 'v1' }) },
  maintenanceActive: () => false,
  updateInfo: () => null,
  setMaintenanceActive: vi.fn(),
  setUpdateInfo: vi.fn(),
  connectSseStream: vi.fn(() => () => undefined),
}));

import App from '@/App';

function setPath(p: string) {
  window.location.pathname = p;
  window.location.href = `http://localhost:3000${p}`;
}

// 每条路径覆盖 App.tsx 中的一个 Route component factory（含 Suspense 包装的 lazy 加载）。
// AdminLayout/AdminProtectedRoute 透传 mock 让 admin 子路由能完成挂载。
const ROUTES: Array<[string, string]> = [
  ['/login', 'Legacy'],
  ['/register', 'Legacy'],
  ['/learning', 'Legacy'],
  ['/flashcard', 'Legacy'],
  ['/vocabulary', 'Legacy'],
  ['/wordbooks', 'Legacy'],
  ['/wordbook-center', 'Legacy'],
  ['/statistics', 'Legacy'],
  ['/history', 'Legacy'],
  ['/profile', 'Legacy'],
  ['/notifications', 'Legacy'],
  ['/admin/login', 'AdminLogin'],
  ['/admin/setup', 'AdminSetup'],
  ['/admin', 'DashboardPage'],
  ['/admin/users', 'UserMgmt'],
  ['/admin/clients', 'Clients'],
  ['/admin/amas-config', 'AmasConfig'],
  ['/admin/amas-metrics', 'AmasMetrics'],
  ['/admin/amas-advisor', 'AmasAdvisor'],
  ['/admin/monitoring', 'Monitoring'],
  ['/admin/analytics', 'Analytics'],
  ['/admin/settings', 'Settings'],
  ['/admin/updates', 'Updates'],
  ['/admin/wordbook-center', 'AdminWB'],
  ['/admin/__not__', 'NotFound'],
  ['/__totally_unknown__', 'NotFound'],
];

describe('App lazy route component factories', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  for (const [path, label] of ROUTES) {
    it(`renders ${label} for ${path}`, async () => {
      setPath(path);
      const { findByText, unmount } = render(() => <App />);
      await findByText(label);
      unmount();
      cleanup();
    });
  }

  it('"/" route triggers window.location.replace to admin login', async () => {
    setPath('/');
    const replaceMock = window.location.replace as ReturnType<typeof vi.fn>;
    replaceMock.mockClear();
    render(() => <App />);
    await waitFor(() => expect(replaceMock).toHaveBeenCalledWith('/admin/login'));
  });
});
