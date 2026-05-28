import { test, expect } from '@playwright/test';

/**
 * 学习流程
 *
 * 学习页 /learning 已迁移至独立用户前端仓库 wordforge-web。
 * 当前仓库 /learning 路由统一渲染 LegacyUserFrontendPage 提示页。
 */
test.describe('Learning flow (legacy redirect)', () => {
  const legacyHeading = '用户前端已迁移到独立仓库';

  test('learning route renders migration notice', async ({ page }) => {
    await page.goto('/learning');
    await expect(page.getByRole('heading', { name: legacyHeading })).toBeVisible();
  });

  test('learning route exposes accurate path hint', async ({ page }) => {
    await page.goto('/learning');
    await expect(page.getByText('你访问的旧路径是 /learning')).toBeVisible();
  });

  test('learning route never bootstraps quiz UI', async ({ page }) => {
    await page.goto('/learning');
    await expect(page.getByRole('heading', { name: legacyHeading })).toBeVisible();
    // 不应再出现旧的学习 UI 文案
    await expect(page.getByText('单词学习', { exact: true })).toHaveCount(0);
    await expect(page.getByText('正在准备学习内容...')).toHaveCount(0);
  });

  test('flashcard alias renders migration notice', async ({ page }) => {
    await page.goto('/flashcard');
    await expect(page.getByRole('heading', { name: legacyHeading })).toBeVisible();
    await expect(page.getByText('你访问的旧路径是 /flashcard')).toBeVisible();
  });

  test('legacy notice shows brand prefix', async ({ page }) => {
    await page.goto('/learning');
    await expect(page.getByText('WordForge', { exact: true })).toBeVisible();
  });
});
