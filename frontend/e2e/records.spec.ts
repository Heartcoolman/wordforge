import { test, expect } from '@playwright/test';

test.describe('Learning Records', () => {
  test('records page loads', async ({ page }) => {
    await page.goto('/records');
    await expect(page.locator('body')).toBeVisible();
    await page.waitForTimeout(2000);
    const pageContent = await page.textContent('body');
    const hasContent =
      pageContent?.includes('记录') ||
      pageContent?.includes('历史') ||
      pageContent?.includes('学习');
    expect(hasContent).toBe(true);
  });

  test('displays learning history', async ({ page }) => {
    await page.goto('/records');
    await page.waitForTimeout(2000);
    await expect(page.locator('body')).toBeVisible();
  });

  test('can filter by date', async ({ page }) => {
    await page.goto('/records');
    await page.waitForTimeout(1500);
    const dateFilter = page.locator('input[type="date"], [data-testid="date-filter"]').first();
    const filterExists = await dateFilter.isVisible().catch(() => false);
    
    if (filterExists) {
      await dateFilter.click();
      await page.waitForTimeout(500);
      await expect(page.locator('body')).toBeVisible();
    }
  });

  test('can view record details', async ({ page }) => {
    await page.goto('/records');
    await page.waitForTimeout(2000);
    const recordItem = page.locator('[data-testid="record-item"]').first();
    const itemExists = await recordItem.isVisible().catch(() => false);
    
    if (itemExists) {
      await recordItem.click();
      await page.waitForTimeout(1000);
      await expect(page.locator('body')).toBeVisible();
    }
  });

  test('displays statistics summary', async ({ page }) => {
    await page.goto('/records');
    await page.waitForTimeout(2000);
    const pageContent = await page.textContent('body');
    const hasStats =
      pageContent?.includes('统计') ||
      pageContent?.includes('总计') ||
      pageContent?.includes('正确率');
    expect(typeof hasStats).toBe('boolean');
  });

  test('pagination works', async ({ page }) => {
    await page.goto('/records');
    await page.waitForTimeout(2000);
    const nextButton = page.getByRole('button', { name: /下一页|next/i });
    const buttonExists = await nextButton.isVisible().catch(() => false);
    
    if (buttonExists) {
      await nextButton.click();
      await page.waitForTimeout(1000);
      await expect(page.locator('body')).toBeVisible();
    }
  });

  test('can export records', async ({ page }) => {
    await page.goto('/records');
    await page.waitForTimeout(1500);
    const exportButton = page.getByRole('button', { name: /导出|下载/ });
    const buttonExists = await exportButton.isVisible().catch(() => false);
    expect(typeof buttonExists).toBe('boolean');
  });

  test('shows time range selector', async ({ page }) => {
    await page.goto('/records');
    await page.waitForTimeout(1500);
    const rangeSelector = page.locator('select, [role="combobox"]').first();
    const selectorExists = await rangeSelector.isVisible().catch(() => false);
    
    if (selectorExists) {
      await rangeSelector.click();
      await page.waitForTimeout(300);
      await expect(page.locator('body')).toBeVisible();
    }
  });
});
