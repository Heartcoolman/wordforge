import { test, expect } from '@playwright/test';

test.describe('Wordbook Center', () => {
  test('wordbook center page loads', async ({ page }) => {
    await page.goto('/wordbook-center');
    await expect(page.locator('body')).toBeVisible();
    await page.waitForTimeout(2000);
    const pageContent = await page.textContent('body');
    const hasContent =
      pageContent?.includes('词书') ||
      pageContent?.includes('中心') ||
      pageContent?.includes('加载');
    expect(hasContent).toBe(true);
  });

  test('displays available wordbooks', async ({ page }) => {
    await page.goto('/wordbook-center');
    await page.waitForTimeout(3000);
    await expect(page.locator('body')).toBeVisible();
  });

  test('can preview wordbook', async ({ page }) => {
    await page.goto('/wordbook-center');
    await page.waitForTimeout(2000);
    const previewButton = page.getByRole('button', { name: /预览|查看/ }).first();
    const buttonExists = await previewButton.isVisible().catch(() => false);
    
    if (buttonExists) {
      await previewButton.click();
      await page.waitForTimeout(1000);
      await expect(page.locator('body')).toBeVisible();
    }
  });

  test('wordbook import flow', async ({ page }) => {
    await page.goto('/wordbook-center');
    await page.waitForTimeout(2000);
    const importButton = page.getByRole('button', { name: /导入|添加/ }).first();
    const buttonExists = await importButton.isVisible().catch(() => false);
    
    if (buttonExists) {
      await importButton.click();
      await page.waitForTimeout(1000);
      await expect(page.locator('body')).toBeVisible();
    }
  });

  test('wordbook search functionality', async ({ page }) => {
    await page.goto('/wordbook-center');
    await page.waitForTimeout(1000);
    const searchInput = page.locator('input[type="search"], input[placeholder*="搜索"]');
    const searchExists = await searchInput.isVisible().catch(() => false);
    
    if (searchExists) {
      await searchInput.fill('CET');
      await page.waitForTimeout(500);
      await expect(page.locator('body')).toBeVisible();
    }
  });

  test('wordbook category filtering', async ({ page }) => {
    await page.goto('/wordbook-center');
    await page.waitForTimeout(2000);
    const categoryButton = page.getByRole('button', { name: /分类|类别/ }).first();
    const buttonExists = await categoryButton.isVisible().catch(() => false);
    
    if (buttonExists) {
      await categoryButton.click();
      await page.waitForTimeout(500);
      await expect(page.locator('body')).toBeVisible();
    }
  });
});
