import { test, expect } from '@playwright/test';

/**
 * 学习配置
 *
 * /study-config 路由已不存在（用户前端迁移后未保留），客户端路由会落到 404 兜底。
 * AMAS 配置已迁移至 /admin/amas-config（管理后台）。
 */
test.describe('Study Configuration (route removed)', () => {
  test('study-config route falls back to 404 page', async ({ page }) => {
    await page.goto('/study-config');
    await expect(page.getByText('404')).toBeVisible({ timeout: 5000 });
    await expect(page.getByRole('heading', { name: '页面不存在' })).toBeVisible();
  });

  test('404 page exposes return-home link', async ({ page }) => {
    await page.goto('/study-config');
    const homeLink = page.getByRole('link', { name: /返回首页/ });
    await expect(homeLink).toBeVisible();
    await expect(homeLink).toHaveAttribute('href', '/');
  });

  test('404 fallback keeps the original URL', async ({ page }) => {
    await page.goto('/study-config');
    await expect(page).toHaveURL(/\/study-config$/);
  });

  test('admin AMAS config route exists (replaces legacy study-config)', async ({ page }) => {
    await page.goto('/admin/amas-config');
    // 未登录 ProtectedRoute 会跳到 /admin/login，断言落地页可识别
    await page.waitForURL('**/admin/login', { timeout: 10_000 });
    await expect(page.getByText('登录管理后台')).toBeVisible();
  });

  test('404 page hints at removed page', async ({ page }) => {
    await page.goto('/study-config');
    await expect(page.getByText(/你访问的页面可能已被移除或地址有误/)).toBeVisible();
  });

  test('404 page renders return-home button label', async ({ page }) => {
    await page.goto('/study-config');
    await expect(page.getByRole('button', { name: '返回首页' })).toBeVisible();
  });

  test('study-config sub-paths also fall back to 404', async ({ page }) => {
    await page.goto('/study-config/amas');
    await expect(page.getByRole('heading', { name: '页面不存在' })).toBeVisible();
  });
});
