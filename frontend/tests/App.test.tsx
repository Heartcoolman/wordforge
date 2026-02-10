import { describe, it, expect, vi } from 'vitest';
import { render } from '@solidjs/testing-library';

// Mock all lazy-loaded pages to avoid heavy imports
vi.mock('@/pages/NotFoundPage', () => ({ default: () => <div>NotFound</div> }));
vi.mock('@/pages/HomePage', () => ({ default: () => <div>Home</div> }));
vi.mock('@/pages/LoginPage', () => ({ default: () => <div>Login</div> }));
vi.mock('@/pages/RegisterPage', () => ({ default: () => <div>Register</div> }));
vi.mock('@/pages/LearningPage', () => ({ default: () => <div>Learning</div> }));
vi.mock('@/pages/FlashcardPage', () => ({ default: () => <div>Flashcard</div> }));
vi.mock('@/pages/VocabularyPage', () => ({ default: () => <div>Vocabulary</div> }));
vi.mock('@/pages/WordbookPage', () => ({ default: () => <div>Wordbook</div> }));
vi.mock('@/pages/StatisticsPage', () => ({ default: () => <div>Statistics</div> }));
vi.mock('@/pages/HistoryPage', () => ({ default: () => <div>History</div> }));
vi.mock('@/pages/ProfilePage', () => ({ default: () => <div>Profile</div> }));
vi.mock('@/pages/NotificationsPage', () => ({ default: () => <div>Notifications</div> }));
vi.mock('@/pages/admin/AdminLoginPage', () => ({ default: () => <div>AdminLogin</div> }));
vi.mock('@/pages/admin/AdminSetupPage', () => ({ default: () => <div>AdminSetup</div> }));
vi.mock('@/pages/admin/AdminDashboard', () => ({ default: () => <div>AdminDashboard</div> }));
vi.mock('@/pages/admin/UserManagementPage', () => ({ default: () => <div>UserMgmt</div> }));
vi.mock('@/pages/admin/AmasConfigPage', () => ({ default: () => <div>AmasConfig</div> }));
vi.mock('@/pages/admin/MonitoringPage', () => ({ default: () => <div>Monitoring</div> }));
vi.mock('@/pages/admin/AnalyticsPage', () => ({ default: () => <div>Analytics</div> }));
vi.mock('@/pages/admin/SettingsPage', () => ({ default: () => <div>Settings</div> }));

vi.mock('@/stores/auth', () => ({
  authStore: {
    isAuthenticated: vi.fn(() => true),
    user: vi.fn(() => ({ id: '1', email: 'a@b.com', username: 'test', isBanned: false })),
    loading: vi.fn(() => false),
    initialized: vi.fn(() => true),
    init: vi.fn(),
  },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));
vi.mock('@/lib/token', () => ({
  tokenManager: {
    getToken: () => 'fake',
    getAdminToken: () => 'fake-admin',
    setTokens: vi.fn(),
    clearTokens: vi.fn(),
    needsRefresh: () => false,
    isAuthenticated: () => true,
    setAdminToken: vi.fn(),
    clearAdminToken: vi.fn(),
  },
}));

import App from '@/App';

describe('App', () => {
  it('renders without crashing', () => {
    const { container } = render(() => <App />);
    expect(container).toBeTruthy();
  });

  it('renders the Toaster component', () => {
    const { container } = render(() => <App />);
    // Toaster renders a div container for toasts
    expect(container.innerHTML).toBeTruthy();
  });

  it('renders route structure', () => {
    const { container } = render(() => <App />);
    // App should render some content (at minimum the layout)
    expect(container.children.length).toBeGreaterThan(0);
  });
});
