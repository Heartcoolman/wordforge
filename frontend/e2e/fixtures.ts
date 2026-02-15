import { test as base } from '@playwright/test';

/**
 * E2E测试全局设置
 * 这个文件用于配置测试钩子和共享的测试状态
 */

export const test = base.extend({
  // 可以在这里添加自定义fixtures
});

export { expect } from '@playwright/test';
