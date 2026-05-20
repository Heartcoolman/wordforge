import { test, expect } from '@playwright/test';

/**
 * 用户登录/注册流程
 *
 * 用户前端已迁移至独立仓库 wordforge-web。
 * 当前仓库对 /login、/register 等旧路由统一渲染 LegacyUserFrontendPage 提示页。
 * 这里的断言用于守住"旧路径必须给出明确迁移提示，不能 404 / 白屏"。
 */
test.describe('Legacy user routes (auth surfaces)', () => {
  const legacyHeading = '用户前端已迁移到独立仓库';

  test('login route shows legacy migration notice', async ({ page }) => {
    await page.goto('/login');
    await expect(page.getByRole('heading', { name: legacyHeading })).toBeVisible();
    await expect(page.getByText('你访问的旧路径是 /login')).toBeVisible();
  });

  test('register route shows legacy migration notice', async ({ page }) => {
    await page.goto('/register');
    await expect(page.getByRole('heading', { name: legacyHeading })).toBeVisible();
    await expect(page.getByText('你访问的旧路径是 /register')).toBeVisible();
  });

  test('legacy notice exposes link to admin console', async ({ page }) => {
    await page.goto('/login');
    const adminLink = page.getByRole('link', { name: '打开管理后台' });
    await expect(adminLink).toBeVisible();
    await expect(adminLink).toHaveAttribute('href', '/admin');
  });

  test('legacy notice surfaces missing VITE_USER_APP_URL hint', async ({ page }) => {
    await page.goto('/login');
    await expect(
      page.getByText('用户前端尚未上线，请联系管理员')
    ).toBeVisible();
  });

  test('legacy notice never renders the old login form', async ({ page }) => {
    await page.goto('/login');
    await expect(page.getByRole('heading', { name: legacyHeading })).toBeVisible();
    await expect(page.locator('input[type="email"]')).toHaveCount(0);
    await expect(page.locator('input[type="password"]')).toHaveCount(0);
  });

  test('root redirects to admin login', async ({ page }) => {
    await page.goto('/');
    await page.waitForURL('**/admin/login', { timeout: 5000 });
    await expect(page).toHaveURL(/\/admin\/login$/);
  });

  test('vocabulary legacy alias also renders notice', async ({ page }) => {
    await page.goto('/vocabulary');
    await expect(page.getByRole('heading', { name: legacyHeading })).toBeVisible();
  });

  test('flashcard legacy alias also renders notice', async ({ page }) => {
    await page.goto('/flashcard');
    await expect(page.getByRole('heading', { name: legacyHeading })).toBeVisible();
  });

  test('history legacy alias also renders notice', async ({ page }) => {
    await page.goto('/history');
    await expect(page.getByRole('heading', { name: legacyHeading })).toBeVisible();
  });
});
