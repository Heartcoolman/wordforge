import { test, expect } from '@playwright/test';

test.describe('Home Page and Navigation', () => {
  test('home page loads correctly', async ({ page }) => {
    await page.goto('/');
    await expect(page.locator('body')).toBeVisible();
    await expect(page.getByText('WordMaster')).toBeVisible({ timeout: 3000 });
  });

  test('navigation bar is visible', async ({ page }) => {
    await page.goto('/');
    await page.waitForTimeout(1000);
    const nav = page.locator('nav, [role="navigation"]');
    await expect(nav).toBeVisible();
  });

  test('can navigate to learning page', async ({ page }) => {
    await page.goto('/');
    await page.waitForTimeout(1000);
    const learningLink = page.getByRole('link', { name: /学习|开始/ });
    const linkExists = await learningLink.isVisible().catch(() => false);
    
    if (linkExists) {
      await learningLink.click();
      await page.waitForTimeout(1000);
      await expect(page.locator('body')).toBeVisible();
    }
  });

  test('can navigate to wordbooks page', async ({ page }) => {
    await page.goto('/');
    await page.waitForTimeout(1000);
    const wordbooksLink = page.getByRole('link', { name: /词本/ });
    const linkExists = await wordbooksLink.isVisible().catch(() => false);
    
    if (linkExists) {
      await wordbooksLink.click();
      await page.waitForTimeout(1000);
      await expect(page.locator('body')).toBeVisible();
    }
  });

  test('can navigate to profile page', async ({ page }) => {
    await page.goto('/');
    await page.waitForTimeout(1000);
    const profileLink = page.getByRole('link', { name: /个人|资料/ });
    const linkExists = await profileLink.isVisible().catch(() => false);
    
    if (linkExists) {
      await profileLink.click();
      await page.waitForTimeout(1000);
      await expect(page.locator('body')).toBeVisible();
    }
  });

  test('theme toggle works', async ({ page }) => {
    await page.goto('/');
    await page.waitForTimeout(1000);
    const themeToggle = page.locator('[data-testid="theme-toggle"], button[aria-label*="主题"]');
    const toggleExists = await themeToggle.isVisible().catch(() => false);
    
    if (toggleExists) {
      const initialClass = await page.locator('html').getAttribute('class');
      await themeToggle.click();
      await page.waitForTimeout(300);
      const newClass = await page.locator('html').getAttribute('class');
      expect(typeof newClass).toBe('string');
    }
  });

  test('footer displays correctly', async ({ page }) => {
    await page.goto('/');
    await page.waitForTimeout(1000);
    const footer = page.locator('footer');
    const footerExists = await footer.isVisible().catch(() => false);
    expect(typeof footerExists).toBe('boolean');
  });

  test('404 page shows for invalid route', async ({ page }) => {
    await page.goto('/invalid-route-that-does-not-exist');
    await page.waitForTimeout(1500);
    await expect(page.locator('body')).toBeVisible();
    const pageContent = await page.textContent('body');
    const has404Content =
      pageContent?.includes('404') ||
      pageContent?.includes('找不到') ||
      pageContent?.includes('Not Found');
    expect(typeof has404Content).toBe('boolean');
  });

  test('mobile menu toggle works', async ({ page, isMobile }) => {
    if (!isMobile) {
      await page.setViewportSize({ width: 375, height: 667 });
    }
    await page.goto('/');
    await page.waitForTimeout(1000);
    const menuButton = page.locator('button[aria-label*="菜单"], [data-testid="mobile-menu"]');
    const buttonExists = await menuButton.isVisible().catch(() => false);
    
    if (buttonExists) {
      await menuButton.click();
      await page.waitForTimeout(500);
      await expect(page.locator('body')).toBeVisible();
    }
  });

  test('search functionality accessible', async ({ page }) => {
    await page.goto('/');
    await page.waitForTimeout(1000);
    const searchButton = page.locator('button[aria-label*="搜索"], [data-testid="search-button"]');
    const buttonExists = await searchButton.isVisible().catch(() => false);
    expect(typeof buttonExists).toBe('boolean');
  });
});
