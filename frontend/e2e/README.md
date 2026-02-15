# E2E测试文档

## 概述

本项目使用 Playwright 进行端到端测试，涵盖了前端应用的主要功能流程。

## 测试结构

```
frontend/e2e/
├── admin.spec.ts           # 管理后台测试
├── auth.spec.ts             # 认证流程测试  
├── home-navigation.spec.ts  # 主页和导航测试
├── learning-flow.spec.ts    # 学习流程测试
├── notifications.spec.ts    # 通知系统测试
├── profile.spec.ts          # 用户资料测试
├── records.spec.ts          # 学习记录测试
├── study-config.spec.ts     # 学习配置测试
├── wordbook-center.spec.ts  # 词书中心测试
├── wordbooks.spec.ts        # 词本管理测试
├── helpers/
│   └── test-helpers.ts      # 测试辅助函数
└── fixtures.ts              # 测试Fixtures

```

## 测试覆盖范围

### 认证模块 (`auth.spec.ts`)
- 登录页面加载和表单验证
- 错误凭据处理
- 注册页面功能
- 密码可见性切换
- 忘记密码链接

### 管理后台 (`admin.spec.ts`)
- 管理员登录
- 权限验证
- 侧边栏导航

### 主页和导航 (`home-navigation.spec.ts`)
- 页面加载
- 导航栏功能
- 主题切换
- 移动端菜单
- 404页面

### 学习流程 (`learning-flow.spec.ts`)
- 学习页面访问
- 加载状态
- 模式切换
- 测验显示

### 词本管理 (`wordbooks.spec.ts`)
- 词本列表
- 搜索功能
- 详情页导航
- 创建按钮

### 词书中心 (`wordbook-center.spec.ts`)
- 词书列表
- 预览功能
- 导入流程
- 分类筛选

### 学习配置 (`study-config.spec.ts`)
- 配置页面加载
- 每日目标调整
- 学习模式切换
- 词本选择
- AMAS设置

### 用户资料 (`profile.spec.ts`)
- 用户信息显示
- 用户名编辑
- 密码修改
- 学习统计
- 头像上传
- 退出登录

### 通知系统 (`notifications.spec.ts`)
- 通知列表
- 标记已读
- 筛选功能
- 删除通知
- 通知徽章

### 学习记录 (`records.spec.ts`)
- 历史记录
- 日期筛选
- 记录详情
- 统计摘要
- 分页功能
- 导出记录

## 运行测试

### 前置要求

- Node.js 20+
- npm 或 pnpm
- Playwright 浏览器（自动安装）

### 安装依赖

```bash
cd frontend
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

配置文件位于 `frontend/playwright.config.ts`：

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

`helpers/test-helpers.ts` 提供了常用的辅助函数：

```typescript
// 用户登录
await loginAsUser(page, 'user@example.com', 'password');

// 管理员登录
await loginAsAdmin(page, 'admin@example.com', 'admin123');

// 等待导航完成
await waitForNavigation(page);

// 检查错误消息
await expectErrorVisible(page);

// 清除存储
await clearStorage(page);
```

## CI/CD 集成

E2E测试可以集成到GitHub Actions工作流中：

```yaml
- name: Install dependencies
  run: |
    cd frontend
    npm ci

- name: Install Playwright browsers
  run: npx playwright install --with-deps chromium

- name: Run E2E tests
  run: npm run test:e2e

- name: Upload test results
  if: always()
  uses: actions/upload-artifact@v3
  with:
    name: playwright-report
    path: frontend/playwright-report/
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
