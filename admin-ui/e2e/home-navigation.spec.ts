import { test, expect } from '@playwright/test';

/**
 * 首页与导航
 *
 * 当前仓库根路径 `/` 会重定向到 `/admin/login`（用户前端已迁移）。
 * 这里的断言守住"未配置/未登录访客落地行为正确、404 兜底有效"。
 */
test.describe('Home Page and Navigation', () => {
  test('root redirects to admin login', async ({ page }) => {
    await page.goto('/');
    await page.waitForURL('**/admin/login', { timeout: 5000 });
    await expect(page).toHaveURL(/\/admin\/login$/);
    await expect(page.getByText('WordForge Admin')).toBeVisible();
  });

  test('admin login renders header brand', async ({ page }) => {
    await page.goto('/admin/login');
    await expect(page.getByText('WordForge Admin')).toBeVisible();
  });

  test('legacy learning route shows migration notice', async ({ page }) => {
    await page.goto('/learning');
    await expect(page.getByRole('heading', { name: '用户前端已迁移到独立仓库' })).toBeVisible();
  });

  test('legacy wordbooks route shows migration notice', async ({ page }) => {
    await page.goto('/wordbooks');
    await expect(page.getByRole('heading', { name: '用户前端已迁移到独立仓库' })).toBeVisible();
  });

  test('legacy profile route shows migration notice', async ({ page }) => {
    await page.goto('/profile');
    await expect(page.getByRole('heading', { name: '用户前端已迁移到独立仓库' })).toBeVisible();
  });

  test('legacy notice link points to admin console', async ({ page }) => {
    await page.goto('/profile');
    const adminLink = page.getByRole('link', { name: '打开管理后台' });
    await expect(adminLink).toBeVisible();
    await expect(adminLink).toHaveAttribute('href', '/admin');
  });

  test('admin login has a single form element', async ({ page }) => {
    await page.goto('/admin/login');
    await expect(page.locator('form')).toHaveCount(1);
  });

  test('404 page shows for invalid route', async ({ page }) => {
    await page.goto('/invalid-route-that-does-not-exist');
    await expect(page.getByText('404')).toBeVisible({ timeout: 5000 });
    await expect(page.getByRole('heading', { name: '页面不存在' })).toBeVisible();
  });

  test('admin login is responsive at mobile width', async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 667 });
    await page.goto('/admin/login');
    await expect(page.getByText('登录管理后台')).toBeVisible();
    await expect(page.locator('input[type="email"]')).toBeVisible();
  });

  test('404 page provides return-home button', async ({ page }) => {
    await page.goto('/some/non/existent/page');
    await expect(page.getByRole('link', { name: /返回首页/ })).toBeVisible();
  });
});
