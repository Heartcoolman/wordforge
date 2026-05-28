import { test, expect } from '@playwright/test';

/**
 * 个人资料
 *
 * /profile 已迁移至独立用户前端 wordforge-web。
 */
test.describe('User Profile (legacy redirect)', () => {
  const legacyHeading = '用户前端已迁移到独立仓库';

  test('profile route renders migration notice', async ({ page }) => {
    await page.goto('/profile');
    await expect(page.getByRole('heading', { name: legacyHeading })).toBeVisible();
  });

  test('profile path is echoed in notice', async ({ page }) => {
    await page.goto('/profile');
    await expect(page.getByText('你访问的旧路径是 /profile')).toBeVisible();
  });

  test('profile notice exposes admin entry link', async ({ page }) => {
    await page.goto('/profile');
    const adminLink = page.getByRole('link', { name: '打开管理后台' });
    await expect(adminLink).toBeVisible();
    await expect(adminLink).toHaveAttribute('href', '/admin');
  });

  test('profile route does not render old form fields', async ({ page }) => {
    await page.goto('/profile');
    await expect(page.getByRole('heading', { name: legacyHeading })).toBeVisible();
    // 旧编辑用户名输入框不应存在
    await expect(page.locator('input[type="text"]')).toHaveCount(0);
  });

  test('profile notice contains repo migration description', async ({ page }) => {
    await page.goto('/profile');
    await expect(page.getByText(/wordforge-web/)).toBeVisible();
  });

  test('profile route does not crash routing', async ({ page }) => {
    await page.goto('/profile');
    await expect(page).toHaveURL(/\/profile$/);
  });

  test('profile notice shows config hint', async ({ page }) => {
    await page.goto('/profile');
    await expect(
      page.getByText('用户前端尚未上线，请联系管理员')
    ).toBeVisible();
  });

  test('profile route has no logout action exposed', async ({ page }) => {
    await page.goto('/profile');
    await expect(page.getByRole('button', { name: /退出|登出/ })).toHaveCount(0);
  });
});
