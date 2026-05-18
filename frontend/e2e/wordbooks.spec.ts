import { test, expect } from '@playwright/test';

/**
 * 用户端词本管理
 *
 * /wordbooks 已迁移至独立用户前端 wordforge-web。
 */
test.describe('Wordbook Management (legacy redirect)', () => {
  const legacyHeading = '用户前端已迁移到独立仓库';

  test('wordbooks route renders migration notice', async ({ page }) => {
    await page.goto('/wordbooks');
    await expect(page.getByRole('heading', { name: legacyHeading })).toBeVisible();
  });

  test('wordbooks path is echoed in notice', async ({ page }) => {
    await page.goto('/wordbooks');
    await expect(page.getByText('你访问的旧路径是 /wordbooks')).toBeVisible();
  });

  test('legacy notice exposes admin entry link', async ({ page }) => {
    await page.goto('/wordbooks');
    const adminLink = page.getByRole('link', { name: '打开管理后台' });
    await expect(adminLink).toBeVisible();
    await expect(adminLink).toHaveAttribute('href', '/admin');
  });

  test('wordbooks route does not render old list UI', async ({ page }) => {
    await page.goto('/wordbooks');
    await expect(page.getByRole('heading', { name: legacyHeading })).toBeVisible();
    // 旧的"创建"按钮不应再出现
    await expect(page.getByRole('button', { name: /创建|新建/ })).toHaveCount(0);
  });

  test('legacy notice references migrated repo name', async ({ page }) => {
    await page.goto('/wordbooks');
    await expect(page.getByText(/wordforge-web/)).toBeVisible();
  });
});
