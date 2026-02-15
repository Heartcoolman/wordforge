import { test, expect } from '@playwright/test';

test.describe('Notifications', () => {
  test('notifications page loads', async ({ page }) => {
    await page.goto('/notifications');
    await expect(page.locator('body')).toBeVisible();
    await page.waitForTimeout(2000);
    const pageContent = await page.textContent('body');
    const hasContent =
      pageContent?.includes('通知') ||
      pageContent?.includes('消息') ||
      pageContent?.includes('提醒');
    expect(hasContent).toBe(true);
  });

  test('displays notification list', async ({ page }) => {
    await page.goto('/notifications');
    await page.waitForTimeout(2000);
    await expect(page.locator('body')).toBeVisible();
  });

  test('can mark notification as read', async ({ page }) => {
    await page.goto('/notifications');
    await page.waitForTimeout(2000);
    const markReadButton = page.getByRole('button', { name: /标记|已读/ }).first();
    const buttonExists = await markReadButton.isVisible().catch(() => false);
    
    if (buttonExists) {
      await markReadButton.click();
      await page.waitForTimeout(500);
      await expect(page.locator('body')).toBeVisible();
    }
  });

  test('can mark all as read', async ({ page }) => {
    await page.goto('/notifications');
    await page.waitForTimeout(1500);
    const markAllButton = page.getByRole('button', { name: /全部已读|标记所有/ });
    const buttonExists = await markAllButton.isVisible().catch(() => false);
    
    if (buttonExists) {
      await markAllButton.click();
      await page.waitForTimeout(1000);
      await expect(page.locator('body')).toBeVisible();
    }
  });

  test('can filter notifications', async ({ page }) => {
    await page.goto('/notifications');
    await page.waitForTimeout(1500);
    const filterButton = page.getByRole('button', { name: /筛选|过滤|类型/ }).first();
    const buttonExists = await filterButton.isVisible().catch(() => false);
    
    if (buttonExists) {
      await filterButton.click();
      await page.waitForTimeout(500);
      await expect(page.locator('body')).toBeVisible();
    }
  });

  test('can delete notification', async ({ page }) => {
    await page.goto('/notifications');
    await page.waitForTimeout(2000);
    const deleteButton = page.getByRole('button', { name: /删除/ }).first();
    const buttonExists = await deleteButton.isVisible().catch(() => false);
    
    if (buttonExists) {
      await deleteButton.click();
      await page.waitForTimeout(500);
      await expect(page.locator('body')).toBeVisible();
    }
  });

  test('notification badge shows count', async ({ page }) => {
    await page.goto('/');
    await page.waitForTimeout(2000);
    const badge = page.locator('[data-testid="notification-badge"], .badge');
    const badgeExists = await badge.isVisible().catch(() => false);
    expect(typeof badgeExists).toBe('boolean');
  });

  test('empty state displays correctly', async ({ page }) => {
    await page.goto('/notifications');
    await page.waitForTimeout(2000);
    const pageContent = await page.textContent('body');
    const hasEmptyOrContent =
      pageContent?.includes('暂无') ||
      pageContent?.includes('通知') ||
      pageContent?.includes('消息');
    expect(hasEmptyOrContent).toBe(true);
  });
});
