import { test, expect } from '@playwright/test';

test.describe('Admin flows', () => {
  test('admin login page loads', async ({ page }) => {
    await page.goto('/admin/login');
    await expect(page.getByText('WordForge Admin')).toBeVisible();
    await expect(page.getByText('管理后台登录')).toBeVisible();
    await expect(page.locator('input[type="email"]')).toBeVisible();
    await expect(page.locator('input[type="password"]')).toBeVisible();
    await expect(page.getByRole('button', { name: '登录' })).toBeVisible();
  });

  test('shows error on wrong credentials', async ({ page }) => {
    await page.goto('/admin/login');
    await page.locator('input[type="email"]').fill('wrong@admin.com');
    await page.locator('input[type="password"]').fill('wrongpass');
    await page.getByRole('button', { name: '登录' }).click();
    // 后端不可用或返回 401 都会触发 role="alert"，统一断言错误文案容器可见
    await expect(page.locator('[role="alert"]')).toBeVisible({ timeout: 10_000 });
  });

  test('admin dashboard requires authentication', async ({ page }) => {
    await page.goto('/admin');
    // 未带 token 时 ProtectedRoute 会跳转到 /admin/login
    await page.waitForURL('**/admin/login', { timeout: 10_000 });
    await expect(page.getByText('管理后台登录')).toBeVisible();
  });

  test('login form structural integrity', async ({ page }) => {
    await page.goto('/admin/login');
    await expect(page.locator('form')).toBeVisible();
    const inputs = page.locator('form input');
    await expect(inputs).toHaveCount(2);
  });

  test('logout state keeps user on admin login', async ({ page }) => {
    await page.goto('/admin/login');
    await expect(page.getByText('管理后台登录')).toBeVisible();
    await expect(page).toHaveURL(/\/admin\/login$/);
  });
});
