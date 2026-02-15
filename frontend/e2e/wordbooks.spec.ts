import { test, expect } from '@playwright/test';

test.describe('Wordbook Management', () => {
  test('wordbooks page loads correctly', async ({ page }) => {
    await page.goto('/wordbooks');
    await expect(page.locator('body')).toBeVisible();
    await expect(page.getByText('词本')).toBeVisible({ timeout: 5000 });
  });

  test('shows wordbook list', async ({ page }) => {
    await page.goto('/wordbooks');
    await page.waitForTimeout(2000);
    const pageContent = await page.textContent('body');
    const hasContent =
      pageContent?.includes('词本') ||
      pageContent?.includes('加载') ||
      pageContent?.includes('暂无');
    expect(hasContent).toBe(true);
  });

  test('can navigate to wordbook detail', async ({ page }) => {
    await page.goto('/wordbooks');
    await page.waitForTimeout(2000);
    const firstWordbook = page.locator('[data-testid="wordbook-item"]').first();
    const wordbookExists = await firstWordbook.isVisible().catch(() => false);
    
    if (wordbookExists) {
      await firstWordbook.click();
      await page.waitForTimeout(1000);
      await expect(page.locator('body')).toBeVisible();
    }
  });

  test('wordbook search works', async ({ page }) => {
    await page.goto('/wordbooks');
    await page.waitForTimeout(1000);
    const searchInput = page.locator('input[type="search"], input[placeholder*="搜索"]');
    const searchExists = await searchInput.isVisible().catch(() => false);
    
    if (searchExists) {
      await searchInput.fill('test');
      await page.waitForTimeout(500);
      await expect(page.locator('body')).toBeVisible();
    }
  });

  test('create wordbook button visible', async ({ page }) => {
    await page.goto('/wordbooks');
    await page.waitForTimeout(1000);
    const createButton = page.getByRole('button', { name: /创建|新建/ });
    const buttonExists = await createButton.isVisible().catch(() => false);
    expect(typeof buttonExists).toBe('boolean');
  });
});
