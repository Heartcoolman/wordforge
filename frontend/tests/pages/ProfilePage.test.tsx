import { describe, it, expect, vi, beforeEach } from 'vitest';
import { waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';
import { createFakeUser } from '../helpers/factories';

// Mock dependencies
vi.mock('@/stores/auth', async () => {
  const { createSignal } = await import('solid-js');
  const [user, setUser] = createSignal(null);
  return {
    authStore: {
      user,
      setUser,
      logout: vi.fn().mockResolvedValue(undefined),
      updateUser: vi.fn(),
      isAuthenticated: () => user() !== null,
      loading: () => false,
      initialized: () => true,
    },
  };
});

vi.mock('@/stores/ui', () => ({
  uiStore: {
    toast: {
      success: vi.fn(),
      error: vi.fn(),
      warning: vi.fn(),
      info: vi.fn(),
    },
  },
}));

vi.mock('@/api/users', () => ({
  usersApi: {
    updateMe: vi.fn(),
    changePassword: vi.fn(),
  },
}));

import { authStore } from '@/stores/auth';

const mockAuthStore = authStore as unknown as {
  user: () => ReturnType<typeof createFakeUser> | null;
  setUser: (u: ReturnType<typeof createFakeUser> | null) => void;
  logout: ReturnType<typeof vi.fn>;
  updateUser: ReturnType<typeof vi.fn>;
};

describe('ProfilePage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('shows "个人中心" heading', async () => {
    const { default: ProfilePage } = await import('@/pages/ProfilePage');
    const { getByText } = renderWithProviders(() => <ProfilePage />);
    expect(getByText('个人中心')).toBeInTheDocument();
  });

  it('shows user email when authenticated', async () => {
    const fakeUser = createFakeUser({ email: 'alice@example.com', username: 'alice' });
    mockAuthStore.setUser(fakeUser);

    const { default: ProfilePage } = await import('@/pages/ProfilePage');
    const { findByText } = renderWithProviders(() => <ProfilePage />);

    expect(await findByText('alice@example.com')).toBeInTheDocument();

    // Cleanup
    mockAuthStore.setUser(null);
  });

  it('shows username input with current username', async () => {
    const fakeUser = createFakeUser({ username: 'bob', email: 'bob@test.com' });
    mockAuthStore.setUser(fakeUser);

    const { default: ProfilePage } = await import('@/pages/ProfilePage');
    const { findByText, container } = renderWithProviders(() => <ProfilePage />);

    // Wait for the user card to render
    await findByText('bob@test.com');

    // Find the username input by its label
    const label = container.querySelector('label');
    const inputs = container.querySelectorAll('input');
    // The username input should have the current username as value
    const usernameInput = Array.from(inputs).find(
      (input) => input.value === 'bob'
    );
    expect(usernameInput).toBeTruthy();

    mockAuthStore.setUser(null);
  });

  it('shows "修改密码" section heading', async () => {
    const { default: ProfilePage } = await import('@/pages/ProfilePage');
    const { getByText } = renderWithProviders(() => <ProfilePage />);
    expect(getByText('修改密码', { selector: 'h2' })).toBeInTheDocument();
  });

  it('shows password input fields', async () => {
    const { default: ProfilePage } = await import('@/pages/ProfilePage');
    const { getByText } = renderWithProviders(() => <ProfilePage />);

    expect(getByText('当前密码')).toBeInTheDocument();
    expect(getByText('新密码')).toBeInTheDocument();
    expect(getByText('确认新密码')).toBeInTheDocument();
  });

  it('shows "退出登录" button', async () => {
    const { default: ProfilePage } = await import('@/pages/ProfilePage');
    const { getByText } = renderWithProviders(() => <ProfilePage />);
    expect(getByText('退出登录')).toBeInTheDocument();
  });

  it('shows "保存" button when user is authenticated', async () => {
    const fakeUser = createFakeUser({ username: 'charlie' });
    mockAuthStore.setUser(fakeUser);

    const { default: ProfilePage } = await import('@/pages/ProfilePage');
    const { findByText } = renderWithProviders(() => <ProfilePage />);

    expect(await findByText('保存')).toBeInTheDocument();

    mockAuthStore.setUser(null);
  });
});
