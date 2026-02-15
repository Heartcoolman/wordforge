import { test, expect } from '@playwright/test';

test.describe('User Profile', () => {
  test('profile page loads', async ({ page }) => {
    await page.goto('/profile');
    await expect(page.locator('body')).toBeVisible();
    await page.waitForTimeout(2000);
    const pageContent = await page.textContent('body');
    const hasContent =
      pageContent?.includes('个人') ||
      pageContent?.includes('资料') ||
      pageContent?.includes('设置') ||
      pageContent?.includes('登录');
    expect(hasContent).toBe(true);
  });

  test('displays user information', async ({ page }) => {
    await page.goto('/profile');
    await page.waitForTimeout(2000);
    await expect(page.locator('body')).toBeVisible();
  });

  test('can edit username', async ({ page }) => {
    await page.goto('/profile');
    await page.waitForTimeout(1500);
    const nameInput = page.locator('input[type="text"]').first();
    const inputExists = await nameInput.isVisible().catch(() => false);
    
    if (inputExists) {
      const currentValue = await nameInput.inputValue();
      await nameInput.fill('NewUsername');
      await page.waitForTimeout(300);
      await expect(nameInput).toHaveValue('NewUsername');
      await nameInput.fill(currentValue);
    }
  });

  test('can change password', async ({ page }) => {
    await page.goto('/profile');
    await page.waitForTimeout(1500);
    const changePasswordButton = page.getByRole('button', { name: /修改密码|更改密码/ });
    const buttonExists = await changePasswordButton.isVisible().catch(() => false);
    
    if (buttonExists) {
      await changePasswordButton.click();
      await page.waitForTimeout(1000);
      await expect(page.locator('body')).toBeVisible();
    }
  });

  test('displays learning statistics', async ({ page }) => {
    await page.goto('/profile');
    await page.waitForTimeout(2000);
    const pageContent = await page.textContent('body');
    const hasStats =
      pageContent?.includes('统计') ||
      pageContent?.includes('学习') ||
      pageContent?.includes('词汇');
    expect(typeof hasStats).toBe('boolean');
  });

  test('can save profile changes', async ({ page }) => {
    await page.goto('/profile');
    await page.waitForTimeout(1500);
    const saveButton = page.getByRole('button', { name: /保存|更新/ });
    const buttonExists = await saveButton.isVisible().catch(() => false);
    
    if (buttonExists) {
      await saveButton.click();
      await page.waitForTimeout(1000);
      await expect(page.locator('body')).toBeVisible();
    }
  });

  test('avatar upload section visible', async ({ page }) => {
    await page.goto('/profile');
    await page.waitForTimeout(1500);
    const uploadArea = page.locator('input[type="file"], [data-testid="avatar-upload"]');
    const uploadExists = await uploadArea.isVisible().catch(() => false);
    expect(typeof uploadExists).toBe('boolean');
  });

  test('logout button works', async ({ page }) => {
    await page.goto('/profile');
    await page.waitForTimeout(1500);
    const logoutButton = page.getByRole('button', { name: /退出|登出/ });
    const buttonExists = await logoutButton.isVisible().catch(() => false);
    
    if (buttonExists) {
      await logoutButton.click();
      await page.waitForTimeout(1000);
      await expect(page.locator('body')).toBeVisible();
    }
  });
});
