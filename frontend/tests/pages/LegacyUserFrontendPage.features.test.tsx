import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';

// USER_APP_URL 在模块顶层 `import.meta.env` 读取一次。
// 为了覆盖 try 分支（lines 8-12）和 <a href={userAppHref!}> 渲染（line 47），
// 需要在 import 之前 stub env 并 reset modules 强制重新执行模块顶层代码。
// 注意：resetModules 后 @solidjs/router / @solidjs/testing-library 也都要重新 import，
// 否则会出现 "<A> can be only used inside a Route" 错误。
describe('LegacyUserFrontendPage extra (USER_APP_URL configured)', () => {
  const originalPath = window.location.pathname;
  const originalSearch = window.location.search;
  const originalHash = window.location.hash;

  beforeEach(() => {
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
    vi.unstubAllEnvs();
    vi.resetModules();
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

  async function loadFresh() {
    vi.resetModules();
    // 必须在 resetModules 之后整组 dynamic import：渲染库 + 路由 + 业务组件
    const [{ render }, { Router, Route }, { QueryClient, QueryClientProvider }, pageMod] =
      await Promise.all([
        import('@solidjs/testing-library'),
        import('@solidjs/router'),
        import('@tanstack/solid-query'),
        import('@/pages/LegacyUserFrontendPage'),
      ]);
    const Page = pageMod.default;
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false, gcTime: 0 }, mutations: { retry: false } },
    });
    return render(() => (
      <QueryClientProvider client={queryClient}>
        <Router>
          <Route path="*" component={() => <Page />} />
        </Router>
      </QueryClientProvider>
    ));
  }

  it('renders 前往用户前端 link with absolute URL when VITE_USER_APP_URL set', async () => {
    vi.stubEnv('VITE_USER_APP_URL', 'https://web.wordforge.example/');
    const view = await loadFresh();
    const link = view.getByText('前往用户前端').closest('a') as HTMLAnchorElement;
    expect(link).not.toBeNull();
    // currentPath = "/" → new URL("/", "https://web.wordforge.example/") = "https://web.wordforge.example/"
    expect(link.getAttribute('href')).toBe('https://web.wordforge.example/');
  });

  it('falls back to raw USER_APP_URL when new URL() throws (invalid base)', async () => {
    // 触发 try/catch 中的 catch 分支：传入非法的 URL base
    vi.stubEnv('VITE_USER_APP_URL', 'not-a-valid-url');
    const view = await loadFresh();
    const link = view.getByText('前往用户前端').closest('a') as HTMLAnchorElement;
    expect(link).not.toBeNull();
    expect(link.getAttribute('href')).toBe('not-a-valid-url');
  });

  it('builds href against current non-root path when VITE_USER_APP_URL set', async () => {
    (window as unknown as { location: Location }).location = {
      ...window.location,
      pathname: '/learning',
      search: '?id=42',
      hash: '#top',
    } as unknown as Location;
    vi.stubEnv('VITE_USER_APP_URL', 'https://web.wordforge.example/');
    const view = await loadFresh();
    const link = view.getByText('前往用户前端').closest('a') as HTMLAnchorElement;
    expect(link).not.toBeNull();
    expect(link.getAttribute('href')).toContain('/learning');
  });
});
