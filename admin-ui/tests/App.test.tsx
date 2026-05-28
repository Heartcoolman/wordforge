import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render } from '@solidjs/testing-library';

vi.mock('@/pages/NotFoundPage', () => ({ default: () => <div>NotFound</div> }));
vi.mock('@/pages/LegacyUserFrontendPage', () => ({ default: () => <div>LegacyUserFrontend</div> }));
vi.mock('@/pages/admin/AdminLoginPage', () => ({ default: () => <div>AdminLogin</div> }));
vi.mock('@/pages/admin/AdminSetupPage', () => ({ default: () => <div>AdminSetup</div> }));
vi.mock('@/pages/admin/AdminDashboard', () => ({ default: () => <div>AdminDashboard</div> }));
vi.mock('@/pages/admin/UserManagementPage', () => ({ default: () => <div>UserMgmt</div> }));
vi.mock('@/pages/admin/AmasConfigPage', () => ({ default: () => <div>AmasConfig</div> }));
vi.mock('@/pages/admin/MonitoringPage', () => ({ default: () => <div>Monitoring</div> }));
vi.mock('@/pages/admin/AnalyticsPage', () => ({ default: () => <div>Analytics</div> }));
vi.mock('@/pages/admin/SettingsPage', () => ({ default: () => <div>Settings</div> }));
vi.mock('@/pages/admin/ClientsPage', () => ({ default: () => <div>Clients</div> }));
vi.mock('@/pages/admin/AdminWordbookCenterPage', () => ({ default: () => <div>AdminWordbookCenter</div> }));

vi.mock('@/stores/ui', () => ({
  uiStore: {
    toasts: vi.fn(() => []),
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
  adminApi: {
    verifyToken: vi.fn().mockResolvedValue({ id: 'admin-1', email: 'admin@test.com' }),
  },
}));
vi.mock('@/api/client', () => ({
  api: { get: vi.fn().mockResolvedValue({ maintenanceMode: false, version: 'test' }) },
  maintenanceActive: () => false,
  updateInfo: () => null,
  setMaintenanceActive: vi.fn(),
  setUpdateInfo: vi.fn(),
  connectSseStream: vi.fn(() => () => undefined),
}));
vi.mock('@/workers/telemetry', () => ({
  startTelemetryWorker: vi.fn(),
  stopTelemetryWorker: vi.fn(),
  handleTelemetryRequest: vi.fn(),
}));

import App from '@/App';

describe('App', () => {
  beforeEach(() => {
    // 默认 pathname 指向 admin login，避免 "/" 路由触发 window.location.replace 后 container 为空
    window.location.pathname = '/admin/login';
    window.location.href = 'http://localhost:3000/admin/login';
  });

  it('renders without crashing', () => {
    const { container } = render(() => <App />);
    expect(container).toBeInstanceOf(HTMLElement);
  });

  it('renders content for the admin login route', () => {
    const { getByText } = render(() => <App />);
    expect(getByText('AdminLogin')).toBeInTheDocument();
  });

  it('renders route structure', () => {
    const { container } = render(() => <App />);
    expect(container.firstElementChild).toHaveTextContent('AdminLogin');
  });
});
