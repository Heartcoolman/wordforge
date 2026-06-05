# E2E测试文档

## 概述

本项目使用 Playwright 进行端到端测试，涵盖了前端应用的主要功能流程。

## 测试结构

```
admin-ui/e2e/
├── admin-real-flows.spec.ts # admin GUI 真实功能流（本套件主战场）
├── admin.spec.ts            # 管理后台登录页测试
├── auth.spec.ts             # 用户旧路由迁移提示（legacy redirect）
├── home-navigation.spec.ts  # 根路径重定向与 404 兜底
├── learning-flow.spec.ts    # 学习页旧路由迁移提示（legacy redirect）
├── notifications.spec.ts    # 通知旧路由迁移提示（legacy redirect）
├── profile.spec.ts          # 用户资料旧路由迁移提示（legacy redirect）
├── records.spec.ts          # 已删除路由 404 兜底
├── study-config.spec.ts     # 已删除路由 404 兜底
├── wordbook-center.spec.ts  # 词书中心旧路由迁移提示（legacy redirect）
├── wordbooks.spec.ts        # 词本旧路由迁移提示（legacy redirect）
├── helpers/
│   └── test-helpers.ts      # 测试辅助函数
└── fixtures.ts              # 测试Fixtures

```

> 用户前端已迁移至独立仓库 `wordforge-web`，admin GUI 才是本套件的主战场，真实功能流集中在 `admin-real-flows.spec.ts`。

## 测试覆盖范围

> 用户前端已迁移至独立仓库 `wordforge-web`，本仓库的用户旧路由（`/login`、`/profile`、`/learning`、`/wordbooks`、`/notifications`、`/wordbook-center` 等）统一渲染 `LegacyUserFrontendPage` 迁移提示页。以下这些 spec 已是 **legacy redirect 冒烟测试**，只验证旧路径给出明确迁移提示（不 404、不白屏），真实功能测试在独立仓 `wordforge-web`。

### admin 真实功能流 (`admin-real-flows.spec.ts`)
- admin GUI 的真实功能流（本套件主战场，以 mock 后端驱动）

### 管理后台 (`admin.spec.ts`)
- admin 登录页加载与表单元素

### 认证模块 (`auth.spec.ts`) — legacy redirect
- `/login`、`/register` 等旧路由渲染迁移提示页
- 验证迁移提示文案、不 404 / 不白屏

### 主页和导航 (`home-navigation.spec.ts`)
- 根路径 `/` 重定向到 `/admin/login`
- 未知路由 404 兜底

### 学习流程 (`learning-flow.spec.ts`) — legacy redirect
- `/learning`、`/flashcard` 渲染迁移提示页
- 不再 bootstrap 旧学习 / 测验 UI

### 词本管理 (`wordbooks.spec.ts`) — legacy redirect
- `/wordbooks` 渲染迁移提示页，暴露 admin 入口链接
- 旧列表 / 创建按钮不再出现

### 词书中心 (`wordbook-center.spec.ts`) — legacy redirect
- `/wordbook-center` 渲染迁移提示页（管理员词书中心在 `/admin/wordbook-center`）

### 学习配置 (`study-config.spec.ts`) — route removed
- `/study-config` 路由已删除，落 404 兜底（AMAS 配置迁至 `/admin/amas-config`）

### 用户资料 (`profile.spec.ts`) — legacy redirect
- `/profile` 渲染迁移提示页，暴露 admin 入口链接
- 旧表单字段 / 退出登录操作不再出现

### 通知系统 (`notifications.spec.ts`) — legacy redirect
- `/notifications` 渲染迁移提示页

### 学习记录 (`records.spec.ts`) — route removed
- `/records` 路由已删除，落 404 兜底

## 运行测试

### 前置要求

- Node.js 20+
- npm 或 pnpm
- Playwright 浏览器（自动安装）

### 安装依赖

```bash
cd admin-ui
npm ci
```

### 安装 Playwright 浏览器

```bash
npx playwright install chromium
```

### 运行所有E2E测试

```bash
npm run test:e2e
```

### 运行特定测试文件

```bash
npx playwright test auth.spec.ts
```

### 调试模式运行

```bash
npx playwright test --debug
```

### 查看测试报告

```bash
npx playwright show-report
```

## 测试配置

配置文件位于 `admin-ui/playwright.config.ts`：

```typescript
{
  testDir: './e2e',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: 'html',
  use: {
    baseURL: 'http://localhost:5173',
    trace: 'on-first-retry',
  },
  webServer: {
    command: 'npm run dev',
    url: 'http://localhost:5173',
    reuseExistingServer: !process.env.CI,
  },
}
```

## 测试特点

### 健壮性

- 所有测试使用条件等待和容错机制
- 处理后端不可用的情况
- 适当的超时设置
- 重试机制（CI环境）

### 可维护性

- 测试辅助函数复用
- 清晰的测试结构
- 描述性的测试名称
- 统一的编码风格

### 隔离性

- 每个测试独立运行
- 不依赖特定的测试顺序
- 清理测试数据（如需要）

## 辅助函数

`helpers/test-helpers.ts` 提供了常用的辅助函数（用户前端迁移后，`loginAsUser` 已移除）：

```typescript
// 等待 Legacy 迁移提示页可见（用户旧路由统一渲染）
await expectLegacyPage(page);

// 管理员登录
await loginAsAdmin(page, 'admin@example.com', 'admin123');

// 清除本地存储与 Cookie
await clearStorage(page);
```

## CI/CD 集成

E2E测试可以集成到GitHub Actions工作流中：

```yaml
- name: Install dependencies
  run: |
    cd admin-ui
    npm ci

- name: Install Playwright browsers
  run: npx playwright install --with-deps chromium

- name: Run E2E tests
  run: npm run test:e2e

- name: Upload test results
  if: always()
  uses: actions/upload-artifact@v4
  with:
    name: playwright-report
    path: admin-ui/playwright-report/
```

## 注意事项

1. **后端依赖**：部分测试需要后端API运行才能完整验证功能。当前测试设计为在后端不可用时仍能验证前端页面加载。

2. **环境变量**：某些测试可能需要特定的环境变量配置。

3. **数据库状态**：完整的E2E测试可能需要特定的数据库状态。

4. **并行执行**：在本地开发时测试并行执行，在CI环境中串行执行以提高稳定性。

## 故障排查

### 测试超时

如果测试频繁超时，可以增加超时时间：

```typescript
test('my test', async ({ page }) => {
  test.setTimeout(60000); // 60秒
  // 测试代码...
});
```

### 元素未找到

确保使用适当的等待策略：

```typescript
// 使用 isVisible 和 catch 处理元素可能不存在的情况
const exists = await page.locator('.my-element').isVisible().catch(() => false);
```

### 页面未加载

检查 webServer 配置和端口是否正确。

## 未来改进

- [ ] 添加视觉回归测试
- [ ] 增加性能测试
- [ ] 添加更多边界条件测试
- [ ] 集成测试数据工厂
- [ ] 添加API mock支持
- [ ] 增加可访问性测试

## 贡献指南

添加新测试时请遵循：

1. 使用描述性的测试名称
2. 添加适当的等待和错误处理
3. 使用测试辅助函数避免重复
4. 保持测试独立性
5. 更新本文档

## 参考资源

- [Playwright 官方文档](https://playwright.dev/)
- [Testing Library 最佳实践](https://testing-library.com/docs/guiding-principles/)
- [项目 README](../README.md)
