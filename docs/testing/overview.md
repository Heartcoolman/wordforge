# 测试概览

WordForge 包含完整的测试套件，覆盖后端 Rust 代码和前端 SolidJS/TypeScript 代码。

## 快速运行

```bash
# 运行所有测试（推荐）
./run-all-tests.sh

# 仅前端测试
cd frontend && npm test

# 仅后端测试
JWT_SECRET="test_secret" ADMIN_JWT_SECRET="test_admin_secret" cargo test
```

## 测试类型

| 类型 | 框架 | 说明 |
|------|------|------|
| [后端单元测试](/testing/unit-tests) | Rust cargo test | Rust 模块和 API 路由测试 |
| [前端单元测试](/testing/unit-tests#前端测试-vitest) | Vitest | 组件、API 客户端、Store 测试 |
| [E2E 测试](/testing/e2e-tests) | Playwright | 端到端功能流程测试（71 个用例） |
| [覆盖率](/testing/coverage) | cargo-llvm-cov / v8 | 代码覆盖率统计 |

## CI/CD 集成

项目包含 GitHub Actions 工作流，在 push 到 `main`/`develop` 分支或 PR 时自动运行测试并生成覆盖率报告。
