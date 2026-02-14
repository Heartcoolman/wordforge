# 测试覆盖率指南

本文档介绍如何生成和查看 WordForge 项目的测试覆盖率报告。

## 快速开始

### 使用 Makefile（推荐）

```bash
# 生成所有覆盖率报告
make coverage

# 仅生成后端覆盖率
make coverage-backend

# 仅生成前端覆盖率
make coverage-frontend

# 在浏览器中打开报告
make coverage-open

# 清理覆盖率报告
make clean
```

## 后端覆盖率（Rust）

### 安装工具

```bash
cargo install cargo-llvm-cov
```

### 生成覆盖率报告

```bash
# 生成 HTML 报告
JWT_SECRET="test_secret_key_for_jwt_signing_minimum_64_characters_long_abcd" \
ADMIN_JWT_SECRET="test_admin_secret_key_for_jwt_signing_minimum_64_chars_long" \
cargo llvm-cov --html --ignore-filename-regex="tests/" --ignore-run-fail

# 生成 JSON 报告
JWT_SECRET="test_secret_key_for_jwt_signing_minimum_64_characters_long_abcd" \
ADMIN_JWT_SECRET="test_admin_secret_key_for_jwt_signing_minimum_64_chars_long" \
cargo llvm-cov --json --ignore-filename-regex="tests/" --ignore-run-fail \
  --output-path target/llvm-cov/coverage.json

# 仅查看摘要
JWT_SECRET="test_secret_key_for_jwt_signing_minimum_64_characters_long_abcd" \
ADMIN_JWT_SECRET="test_admin_secret_key_for_jwt_signing_minimum_64_chars_long" \
cargo llvm-cov --summary-only --ignore-filename-regex="tests/" --ignore-run-fail
```

### 查看报告

HTML 报告位于：`target/llvm-cov/html/index.html`

## 前端覆盖率（TypeScript/SolidJS）

### 安装依赖

```bash
cd frontend
npm install
```

### 生成覆盖率报告

```bash
cd frontend
npm run test:coverage
```

### 配置

前端测试覆盖率配置位于 `frontend/vitest.config.ts`：

- **覆盖率提供者**: v8
- **包含文件**: `src/**/*.{ts,tsx}`
- **排除文件**: 
  - `src/main.tsx`
  - `src/admin-main.tsx`
  - `src/types/**`
  - `src/index.css`

### 覆盖率阈值

- **行覆盖率**: 80%
- **函数覆盖率**: 80%
- **分支覆盖率**: 75%

### 查看报告

HTML 报告位于：`frontend/coverage/index.html`

## 当前覆盖率状态

### 后端（Rust）
- **行覆盖率**: ~58.79%
- **函数覆盖率**: ~56.84%
- **区域覆盖率**: ~56.03%

### 前端（TypeScript）
- 配置覆盖率阈值已设置
- 测试基础设施已完善
- API 测试已修复（120+ 测试通过）

## CI/CD 集成

项目包含 GitHub Actions 工作流（`.github/workflows/coverage.yml`），会在以下情况自动运行：

- Push 到 `main` 或 `develop` 分支
- Pull Request 到 `main` 或 `develop` 分支

工作流会：
1. 运行后端测试并生成覆盖率
2. 运行前端测试并生成覆盖率  
3. 上传覆盖率报告到 Codecov（如已配置）

## 故障排除

### 后端测试失败

如果遇到 JWT 相关错误，确保设置了环境变量：

```bash
export JWT_SECRET="test_secret_key_for_jwt_signing_minimum_64_characters_long_abcd"
export ADMIN_JWT_SECRET="test_admin_secret_key_for_jwt_signing_minimum_64_chars_long"
```

### 前端测试环境

项目使用 `happy-dom` 作为测试环境（而非 `jsdom`），这提供了更好的与 MSW (Mock Service Worker) 的兼容性。

## 提高覆盖率的建议

1. **识别未覆盖代码**: 查看 HTML 报告，找出未被测试覆盖的代码行
2. **添加单元测试**: 为核心业务逻辑添加单元测试
3. **添加集成测试**: 测试多个模块之间的交互
4. **边界条件测试**: 确保测试了异常情况和边界条件

## 相关资源

- [cargo-llvm-cov 文档](https://github.com/taiki-e/cargo-llvm-cov)
- [Vitest 覆盖率文档](https://vitest.dev/guide/coverage.html)
- [MSW 文档](https://mswjs.io/)
