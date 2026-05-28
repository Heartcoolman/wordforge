import { Page, expect } from '@playwright/test';

/**
 * E2E 测试辅助函数
 *
 * 说明：用户前端已迁移至独立仓库 `wordforge-web`，
 * 本仓库的所有用户路由（/login、/profile 等）均渲染 LegacyUserFrontendPage 提示页。
 * 因此原 `loginAsUser` 不再可用，仅保留 admin 登录辅助。
 */

/**
 * 等待 Legacy 提示页可见（用户前端旧路由统一渲染）
 */
export async function expectLegacyPage(page: Page, timeout = 5000) {
  await expect(page.getByRole('heading', { name: '用户前端已迁移到独立仓库' })).toBeVisible({ timeout });
}

/**
 * 管理员登录辅助函数
 */
export async function loginAsAdmin(page: Page, email = 'admin@example.com', password = 'admin123') {
  await page.goto('/admin/login');
  await page.locator('input[type="email"]').fill(email);
  await page.locator('input[type="password"]').fill(password);
  await page.getByRole('button', { name: '登录' }).click();
}

/**
 * 清除本地存储与 Cookie
 */
export async function clearStorage(page: Page) {
  await page.evaluate(() => {
    localStorage.clear();
    sessionStorage.clear();
  });
  await page.context().clearCookies();
}
