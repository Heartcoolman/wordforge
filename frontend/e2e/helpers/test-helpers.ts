import { Page, expect } from '@playwright/test';

/**
 * E2E测试辅助函数
 */

/**
 * 用户登录辅助函数
 */
export async function loginAsUser(page: Page, email = 'test@example.com', password = 'password123') {
  await page.goto('/login');
  await page.locator('input[type="email"]').fill(email);
  await page.locator('input[type="password"]').fill(password);
  await page.getByRole('button', { name: '登录' }).click();
  await page.waitForTimeout(2000);
}

/**
 * 管理员登录辅助函数
 */
export async function loginAsAdmin(page: Page, email = 'admin@example.com', password = 'admin123') {
  await page.goto('/admin/login');
  await page.locator('input[type="email"]').fill(email);
  await page.locator('input[type="password"]').fill(password);
  await page.getByRole('button', { name: '登录' }).click();
  await page.waitForTimeout(2000);
}

/**
 * 等待导航完成
 */
export async function waitForNavigation(page: Page, timeout = 3000) {
  await page.waitForTimeout(timeout);
  await expect(page.locator('body')).toBeVisible();
}

/**
 * 检查是否显示错误信息
 */
export async function expectErrorVisible(page: Page, timeout = 5000) {
  await expect(page.locator('.text-error, .error, [role="alert"]')).toBeVisible({ timeout });
}

/**
 * 检查是否显示成功信息
 */
export async function expectSuccessVisible(page: Page, timeout = 5000) {
  await expect(page.locator('.text-success, .success, [role="status"]')).toBeVisible({ timeout });
}

/**
 * 清除本地存储和Cookie
 */
export async function clearStorage(page: Page) {
  await page.evaluate(() => {
    localStorage.clear();
    sessionStorage.clear();
  });
  await page.context().clearCookies();
}
