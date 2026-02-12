import { describe, it, expect, vi, beforeAll, afterAll, afterEach } from 'vitest';
import { fireEvent, screen, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';
import { server } from '../helpers/msw-server';
import RegisterPage from '@/pages/RegisterPage';

vi.mock('@/stores/auth', () => ({
  authStore: {
    isAuthenticated: vi.fn(() => false),
    loading: vi.fn(() => false),
    register: vi.fn(),
    user: vi.fn(() => null),
  },
}));

vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

vi.mock('@solidjs/router', async (importOriginal) => {
  const mod = await importOriginal<typeof import('@solidjs/router')>();
  return { ...mod, useNavigate: () => vi.fn() };
});

beforeAll(() => server.listen({ onUnhandledRequest: 'error' }));
afterEach(() => { server.resetHandlers(); vi.clearAllMocks(); });
afterAll(() => server.close());

describe('RegisterPage', () => {
  it('renders 注册 heading', () => {
    renderWithProviders(() => <RegisterPage />);
    expect(screen.getByRole('heading', { name: '注册' })).toBeInTheDocument();
  });

  it('renders all 4 inputs', () => {
    renderWithProviders(() => <RegisterPage />);
    expect(screen.getByPlaceholderText('your@email.com')).toBeInTheDocument();
    expect(screen.getByPlaceholderText('昵称')).toBeInTheDocument();
    expect(screen.getByPlaceholderText('至少 8 位')).toBeInTheDocument();
    expect(screen.getByPlaceholderText('再次输入密码')).toBeInTheDocument();
  });

  it('shows error when fields empty', async () => {
    renderWithProviders(() => <RegisterPage />);
    const btn = screen.getByRole('button', { name: '注册' });
    fireEvent.click(btn);
    await waitFor(() => {
      expect(screen.getByText('请填写所有字段')).toBeInTheDocument();
    });
  });

  it('shows error when password < 8 chars', async () => {
    renderWithProviders(() => <RegisterPage />);
    fireEvent.input(screen.getByPlaceholderText('your@email.com'), { target: { value: 'a@b.com' } });
    fireEvent.input(screen.getByPlaceholderText('昵称'), { target: { value: 'user' } });
    fireEvent.input(screen.getByPlaceholderText('至少 8 位'), { target: { value: '123' } });
    fireEvent.input(screen.getByPlaceholderText('再次输入密码'), { target: { value: '123' } });

    const form = screen.getByRole('button', { name: '注册' }).closest('form')!;
    fireEvent.submit(form);

    await waitFor(() => {
      expect(screen.getByText('密码至少 8 位')).toBeInTheDocument();
    });
  });

  it('shows error when passwords don\'t match', async () => {
    renderWithProviders(() => <RegisterPage />);
    fireEvent.input(screen.getByPlaceholderText('your@email.com'), { target: { value: 'a@b.com' } });
    fireEvent.input(screen.getByPlaceholderText('昵称'), { target: { value: 'user' } });
    fireEvent.input(screen.getByPlaceholderText('至少 8 位'), { target: { value: '12345678' } });
    fireEvent.input(screen.getByPlaceholderText('再次输入密码'), { target: { value: '87654321' } });

    const form = screen.getByRole('button', { name: '注册' }).closest('form')!;
    fireEvent.submit(form);

    await waitFor(() => {
      expect(screen.getByText('两次密码不一致')).toBeInTheDocument();
    });
  });

  it('has link to login page (去登录)', () => {
    renderWithProviders(() => <RegisterPage />);
    const link = screen.getByText('去登录');
    expect(link).toBeInTheDocument();
    expect(link.closest('a')).toHaveAttribute('href', '/login');
  });
});
