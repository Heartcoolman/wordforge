# E2E 测试

项目使用 Playwright 进行端到端测试，覆盖 71 个测试用例。

## 运行 E2E 测试

```bash
cd frontend

# 首次运行需要安装 Playwright 浏览器
npx playwright install chromium

# 运行 E2E 测试
npm run test:e2e

# 使用脚本运行（自动安装依赖）
./run-e2e-tests.sh

# 查看测试报告
npx playwright show-report

# 调试模式
npx playwright test --debug
```

## 测试结构

```
frontend/e2e/
├── admin.spec.ts           # 管理后台测试
├── auth.spec.ts            # 认证流程测试
├── home-navigation.spec.ts # 主页和导航测试
├── learning-flow.spec.ts   # 学习流程测试
├── notifications.spec.ts   # 通知系统测试
├── profile.spec.ts         # 用户资料测试
├── records.spec.ts         # 学习记录测试
├── study-config.spec.ts    # 学习配置测试
├── wordbook-center.spec.ts # 词书中心测试
├── wordbooks.spec.ts       # 词本管理测试
├── helpers/
│   └── test-helpers.ts     # 测试辅助函数
└── fixtures.ts             # 测试 Fixtures
```

## 覆盖模块

| 模块 | 测试内容 |
|------|---------|
| 认证流程 | 登录、注册、密码重置、表单验证 |
| 管理后台 | 管理员登录、权限验证、侧边栏导航 |
| 学习流程 | 学习页面访问、加载状态、模式切换、测验显示 |
| 词本管理 | 词本列表、搜索、详情导航、创建 |
| 词书中心 | 词书列表、预览、导入流程、分类筛选 |
| 学习配置 | 每日目标调整、学习模式切换、词本选择 |
| 用户资料 | 用户名编辑、密码修改、头像上传、退出登录 |
| 通知系统 | 通知列表、标记已读、筛选、通知徽章 |
| 学习记录 | 历史记录、日期筛选、分页、导出 |
| 主页和导航 | 页面加载、导航栏、主题切换、移动端菜单、404 |

## 测试配置

配置位于 `frontend/playwright.config.ts`，关键设置：

- 测试目录：`./e2e`
- 基础 URL：`http://localhost:5173`
- CI 环境重试 2 次，串行执行
- 自动启动前端开发服务器
