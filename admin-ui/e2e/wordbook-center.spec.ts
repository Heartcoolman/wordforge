import { test, expect } from '@playwright/test';

/**
 * 词书中心
 *
 * /wordbook-center 路由仍存在，但已迁移至独立用户前端 wordforge-web。
 * 当前仓库该路由统一渲染 LegacyUserFrontendPage 提示页；管理员词书中心位于 /admin/wordbook-center。
 */
test.describe('Wordbook Center (legacy redirect)', () => {
  const legacyHeading = '用户前端已迁移到独立仓库';

  test('wordbook-center route renders migration notice', async ({ page }) => {
    await page.goto('/wordbook-center');
    await expect(page.getByRole('heading', { name: legacyHeading })).toBeVisible();
  });

  test('wordbook-center path is echoed in notice', async ({ page }) => {
    await page.goto('/wordbook-center');
    await expect(page.getByText('你访问的旧路径是 /wordbook-center')).toBeVisible();
  });

  test('legacy notice exposes admin entry link', async ({ page }) => {
    await page.goto('/wordbook-center');
    const adminLink = page.getByRole('link', { name: '打开管理后台' });
    await expect(adminLink).toBeVisible();
    await expect(adminLink).toHaveAttribute('href', '/admin');
  });

  test('admin wordbook center is the new entrypoint', async ({ page }) => {
    await page.goto('/admin/wordbook-center');
    // 未登录会被 ProtectedRoute 跳到管理员登录
    await page.waitForURL('**/admin/login', { timeout: 10_000 });
    await expect(page.getByText('管理后台登录')).toBeVisible();
  });

  test('legacy notice references migrated repo name', async ({ page }) => {
    await page.goto('/wordbook-center');
    await expect(page.getByText(/wordforge-web/)).toBeVisible();
  });

  test('legacy notice shows config hint', async ({ page }) => {
    await page.goto('/wordbook-center');
    await expect(
      page.getByText('用户前端尚未上线，请联系管理员')
    ).toBeVisible();
  });
});
