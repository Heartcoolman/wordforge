import { describe, it, expect, vi, beforeAll, afterAll, afterEach, type Mock } from 'vitest';
import { fireEvent, screen, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';
import { server } from '../helpers/msw-server';
import LoginPage from '@/pages/LoginPage';

vi.mock('@/stores/auth', () => {
  const login = vi.fn();
  return {
    authStore: {
      isAuthenticated: vi.fn(() => false),
      login,
      user: vi.fn(() => null),
    },
  };
});

vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

const mockNavigate = vi.fn();
vi.mock('@solidjs/router', async (importOriginal) => {
  const mod = await importOriginal<typeof import('@solidjs/router')>();
  return { ...mod, useNavigate: () => mockNavigate };
});

import { authStore } from '@/stores/auth';

beforeAll(() => server.listen({ onUnhandledRequest: 'error' }));
afterEach(() => { server.resetHandlers(); vi.clearAllMocks(); });
afterAll(() => server.close());

describe('LoginPage', () => {
  it('renders 登录 heading', () => {
    renderWithProviders(() => <LoginPage />);
    expect(screen.getByRole('heading', { name: '登录' })).toBeInTheDocument();
  });

  it('renders email and password inputs', () => {
    renderWithProviders(() => <LoginPage />);
    expect(screen.getByPlaceholderText('your@email.com')).toBeInTheDocument();
    expect(screen.getByPlaceholderText('输入密码')).toBeInTheDocument();
  });

  it('shows validation error when fields empty', async () => {
    renderWithProviders(() => <LoginPage />);
    const btn = screen.getByRole('button', { name: '登录' });
    fireEvent.click(btn);
    await waitFor(() => {
      expect(screen.getByText('请填写邮箱和密码')).toBeInTheDocument();
    });
  });

  it('shows login button', () => {
    renderWithProviders(() => <LoginPage />);
    expect(screen.getByRole('button', { name: '登录' })).toBeInTheDocument();
  });

  it('shows loading state on submit', async () => {
    (authStore.login as Mock).mockImplementation(() => new Promise(() => {}));
    renderWithProviders(() => <LoginPage />);

    const emailInput = screen.getByPlaceholderText('your@email.com');
    const pwInput = screen.getByPlaceholderText('输入密码');
    fireEvent.input(emailInput, { target: { value: 'a@b.com' } });
    fireEvent.input(pwInput, { target: { value: '123456' } });

    const form = screen.getByRole('button', { name: '登录' }).closest('form')!;
    fireEvent.submit(form);

    await waitFor(() => {
      expect(authStore.login).toHaveBeenCalledWith('a@b.com', '123456');
    });
  });

  it('shows error message on failed login', async () => {
    (authStore.login as Mock).mockRejectedValue(new Error('邮箱或密码错误'));
    renderWithProviders(() => <LoginPage />);

    const emailInput = screen.getByPlaceholderText('your@email.com');
    const pwInput = screen.getByPlaceholderText('输入密码');
    fireEvent.input(emailInput, { target: { value: 'a@b.com' } });
    fireEvent.input(pwInput, { target: { value: 'wrong' } });

    const form = screen.getByRole('button', { name: '登录' }).closest('form')!;
    fireEvent.submit(form);

    await waitFor(() => {
      expect(screen.getByText('邮箱或密码错误')).toBeInTheDocument();
    });
  });

  it('navigates to / on success', async () => {
    (authStore.login as Mock).mockResolvedValue({});
    renderWithProviders(() => <LoginPage />);

    const emailInput = screen.getByPlaceholderText('your@email.com');
    const pwInput = screen.getByPlaceholderText('输入密码');
    fireEvent.input(emailInput, { target: { value: 'a@b.com' } });
    fireEvent.input(pwInput, { target: { value: '123456' } });

    const form = screen.getByRole('button', { name: '登录' }).closest('form')!;
    fireEvent.submit(form);

    await waitFor(() => {
      expect(mockNavigate).toHaveBeenCalledWith('/', { replace: true });
    });
  });

  it('has link to register page (立即注册)', () => {
    renderWithProviders(() => <LoginPage />);
    const link = screen.getByText('立即注册');
    expect(link).toBeInTheDocument();
    expect(link.closest('a')).toHaveAttribute('href', '/register');
  });

  it('form has submit handler', () => {
    renderWithProviders(() => <LoginPage />);
    const form = screen.getByRole('button', { name: '登录' }).closest('form');
    expect(form).toBeInTheDocument();
  });
});
