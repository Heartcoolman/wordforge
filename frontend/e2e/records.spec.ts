import { test, expect } from '@playwright/test';

/**
 * 学习记录
 *
 * /records 路由已不存在（用户前端迁移后未保留），客户端路由会落到 404 兜底。
 * 这里的断言守住"被删除路由必须给出 404 页，不能白屏"。
 */
test.describe('Learning Records (route removed)', () => {
  test('records route falls back to 404 page', async ({ page }) => {
    await page.goto('/records');
    await expect(page.getByText('404')).toBeVisible({ timeout: 5000 });
    await expect(page.getByRole('heading', { name: '页面不存在' })).toBeVisible();
  });

  test('404 page exposes accent 404 hero', async ({ page }) => {
    await page.goto('/records');
    await expect(page.locator('p', { hasText: '404' })).toBeVisible();
  });

  test('404 page exposes return-home link', async ({ page }) => {
    await page.goto('/records');
    const homeLink = page.getByRole('link', { name: /返回首页/ });
    await expect(homeLink).toBeVisible();
    await expect(homeLink).toHaveAttribute('href', '/');
  });

  test('404 page contains friendly description', async ({ page }) => {
    await page.goto('/records');
    await expect(page.getByText(/你访问的页面可能已被移除或地址有误/)).toBeVisible();
  });

  test('records sub-paths also fall back to 404', async ({ page }) => {
    await page.goto('/records/123');
    await expect(page.getByRole('heading', { name: '页面不存在' })).toBeVisible();
  });

  test('404 fallback keeps the original URL', async ({ page }) => {
    await page.goto('/records');
    await expect(page).toHaveURL(/\/records$/);
  });

  test('404 page renders return-home button label', async ({ page }) => {
    await page.goto('/records');
    await expect(page.getByRole('button', { name: '返回首页' })).toBeVisible();
  });

  test('records statistics page also routes to 404', async ({ page }) => {
    await page.goto('/records/statistics');
    await expect(page.getByText('404')).toBeVisible({ timeout: 5000 });
  });
});
