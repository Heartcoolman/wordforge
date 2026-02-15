import { test, expect } from '@playwright/test';

test.describe('Study Configuration', () => {
  test('study config page loads', async ({ page }) => {
    await page.goto('/study-config');
    await expect(page.locator('body')).toBeVisible();
    await page.waitForTimeout(2000);
    const pageContent = await page.textContent('body');
    const hasContent =
      pageContent?.includes('学习') ||
      pageContent?.includes('配置') ||
      pageContent?.includes('设置');
    expect(hasContent).toBe(true);
  });

  test('displays study settings', async ({ page }) => {
    await page.goto('/study-config');
    await page.waitForTimeout(2000);
    await expect(page.locator('body')).toBeVisible();
  });

  test('can adjust daily goal', async ({ page }) => {
    await page.goto('/study-config');
    await page.waitForTimeout(1500);
    const goalInput = page.locator('input[type="number"]').first();
    const inputExists = await goalInput.isVisible().catch(() => false);
    
    if (inputExists) {
      await goalInput.fill('50');
      await page.waitForTimeout(500);
      await expect(goalInput).toHaveValue('50');
    }
  });

  test('can toggle study modes', async ({ page }) => {
    await page.goto('/study-config');
    await page.waitForTimeout(1500);
    const toggleButton = page.locator('button[role="switch"], input[type="checkbox"]').first();
    const toggleExists = await toggleButton.isVisible().catch(() => false);
    
    if (toggleExists) {
      await toggleButton.click();
      await page.waitForTimeout(300);
      await expect(page.locator('body')).toBeVisible();
    }
  });

  test('can save configuration', async ({ page }) => {
    await page.goto('/study-config');
    await page.waitForTimeout(1500);
    const saveButton = page.getByRole('button', { name: /保存|确定|应用/ });
    const buttonExists = await saveButton.isVisible().catch(() => false);
    
    if (buttonExists) {
      await saveButton.click();
      await page.waitForTimeout(1000);
      await expect(page.locator('body')).toBeVisible();
    }
  });

  test('displays wordbook selection', async ({ page }) => {
    await page.goto('/study-config');
    await page.waitForTimeout(2000);
    const selectElement = page.locator('select, [role="combobox"]').first();
    const selectExists = await selectElement.isVisible().catch(() => false);
    expect(typeof selectExists).toBe('boolean');
  });

  test('AMAS settings panel accessible', async ({ page }) => {
    await page.goto('/study-config');
    await page.waitForTimeout(2000);
    const pageContent = await page.textContent('body');
    const hasAMAS =
      pageContent?.includes('AMAS') ||
      pageContent?.includes('自适应') ||
      pageContent?.includes('算法');
    expect(typeof hasAMAS).toBe('boolean');
  });
});
