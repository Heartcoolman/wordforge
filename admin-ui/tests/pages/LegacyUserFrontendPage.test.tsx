import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { screen } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';
import LegacyUserFrontendPage from '@/pages/LegacyUserFrontendPage';

// USER_APP_URL 在模块顶层读取一次，无法在测试间切换；
// 这里把视野放在 LegacyUserFrontendPage 自身渲染分支：
//   - isHome（pathname === '/'）vs 非 home 分支
//   - userAppHref 配置（来自 import.meta.env）有/无 时的 fallback / link
describe('LegacyUserFrontendPage', () => {
  const originalPath = window.location.pathname;
  const originalSearch = window.location.search;
  const originalHash = window.location.hash;

  beforeEach(() => {
    // happy-dom 允许直接覆盖 location.pathname
    (window as unknown as { location: Location }).location = {
      ...window.location,
      pathname: '/',
      search: '',
      hash: '',
      assign: () => {},
      replace: () => {},
      reload: () => {},
    } as unknown as Location;
  });

  afterEach(() => {
    (window as unknown as { location: Location }).location = {
      ...window.location,
      pathname: originalPath,
      search: originalSearch,
      hash: originalHash,
      assign: () => {},
      replace: () => {},
      reload: () => {},
    } as unknown as Location;
  });

  it('renders heading and 打开管理后台 link on home path', () => {
    renderWithProviders(() => <LegacyUserFrontendPage />);
    expect(screen.getByText('用户前端已迁移到独立仓库')).toBeInTheDocument();
    expect(screen.getByText('打开管理后台')).toBeInTheDocument();
    expect(screen.getByText('如果你是管理员，可以直接进入后台。')).toBeInTheDocument();
  });

  it('shows old path notice when not on home', () => {
    (window as unknown as { location: Location }).location = {
      ...window.location,
      pathname: '/learning',
      search: '?id=42',
      hash: '#top',
    } as unknown as Location;
    renderWithProviders(() => <LegacyUserFrontendPage />);
    expect(screen.getByText(/旧路径是.*\/learning/)).toBeInTheDocument();
  });

  it('shows 尚未上线 hint when VITE_USER_APP_URL is empty', () => {
    // 默认 import.meta.env 未设置 VITE_USER_APP_URL（fallback 分支）
    if (!(import.meta.env as Record<string, unknown>).VITE_USER_APP_URL) {
      renderWithProviders(() => <LegacyUserFrontendPage />);
      expect(screen.getByText(/尚未上线/)).toBeInTheDocument();
    } else {
      // CI 注入了该 env 时跳过严格断言，仍要保证渲染不抛错
      renderWithProviders(() => <LegacyUserFrontendPage />);
      expect(screen.getByText('用户前端已迁移到独立仓库')).toBeInTheDocument();
    }
  });
});
