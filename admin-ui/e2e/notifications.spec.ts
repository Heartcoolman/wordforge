import { test, expect } from '@playwright/test';

/**
 * 通知中心
 *
 * /notifications 已迁移至独立用户前端 wordforge-web，
 * 当前仓库该路由统一渲染 LegacyUserFrontendPage 提示页。
 */
test.describe('Notifications (legacy redirect)', () => {
  const legacyHeading = '用户前端已迁移到独立仓库';

  test('notifications route renders migration notice', async ({ page }) => {
    await page.goto('/notifications');
    await expect(page.getByRole('heading', { name: legacyHeading })).toBeVisible();
  });

  test('notifications path is echoed in notice', async ({ page }) => {
    await page.goto('/notifications');
    await expect(page.getByText('你访问的旧路径是 /notifications')).toBeVisible();
  });

  test('notifications notice exposes admin entry link', async ({ page }) => {
    await page.goto('/notifications');
    const adminLink = page.getByRole('link', { name: '打开管理后台' });
    await expect(adminLink).toBeVisible();
    await expect(adminLink).toHaveAttribute('href', '/admin');
  });

  test('notifications notice shows user-app config hint', async ({ page }) => {
    await page.goto('/notifications');
    await expect(
      page.getByText('用户前端尚未上线，请联系管理员')
    ).toBeVisible();
  });

  test('notifications route does not render old list UI', async ({ page }) => {
    await page.goto('/notifications');
    await expect(page.getByRole('heading', { name: legacyHeading })).toBeVisible();
    await expect(page.getByRole('button', { name: /标记|已读/ })).toHaveCount(0);
    await expect(page.getByRole('button', { name: /全部已读|标记所有/ })).toHaveCount(0);
  });

  test('legacy notice exposes WordForge brand', async ({ page }) => {
    await page.goto('/notifications');
    await expect(page.getByText('WordForge', { exact: true })).toBeVisible();
  });

  test('notifications notice references migrated repo name', async ({ page }) => {
    await page.goto('/notifications');
    await expect(page.getByText(/wordforge-web/)).toBeVisible();
  });

  test('notifications path mismatch does not crash routing', async ({ page }) => {
    await page.goto('/notifications');
    // 仍然在原路径，未被 404 兜底吞掉
    await expect(page).toHaveURL(/\/notifications$/);
  });
});
