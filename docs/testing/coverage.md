# 测试覆盖率

## 快速开始

```bash
# 生成所有覆盖率报告
make coverage

# 仅后端
make coverage-backend

# 仅前端
make coverage-frontend

# 在浏览器中查看
make coverage-open

# 清理
make clean
```

## 后端覆盖率（Rust）

### 安装工具

```bash
cargo install cargo-llvm-cov
```

### 生成报告

```bash
# HTML 报告
JWT_SECRET="test_secret_key_for_jwt_signing_minimum_64_characters_long_abcd" \
ADMIN_JWT_SECRET="test_admin_secret_key_for_jwt_signing_minimum_64_chars_long" \
cargo llvm-cov --html --ignore-filename-regex="tests/" --ignore-run-fail

# JSON 报告
JWT_SECRET="test_secret_key_for_jwt_signing_minimum_64_characters_long_abcd" \
ADMIN_JWT_SECRET="test_admin_secret_key_for_jwt_signing_minimum_64_chars_long" \
cargo llvm-cov --json --ignore-filename-regex="tests/" --ignore-run-fail \
  --output-path target/llvm-cov/coverage.json
```

HTML 报告位于：`target/llvm-cov/html/index.html`

### 当前状态

- 行覆盖率：~58.79%
- 函数覆盖率：~56.84%
- 区域覆盖率：~56.03%

## 前端覆盖率（TypeScript）

```bash
cd frontend
npm run test:coverage
```

HTML 报告位于：`frontend/coverage/index.html`

### 覆盖率阈值

| 指标 | 阈值 |
|------|------|
| 行覆盖率 | 80% |
| 函数覆盖率 | 80% |
| 分支覆盖率 | 75% |

### 配置

覆盖率配置位于 `frontend/vitest.config.ts`：

- 覆盖率提供者：v8
- 包含文件：`src/**/*.{ts,tsx}`
- 排除文件：`src/main.tsx`、`src/admin-main.tsx`、`src/types/**`、`src/index.css`

## CI/CD 集成

GitHub Actions 工作流（`.github/workflows/coverage.yml`）在 push 到 `main`/`develop` 分支或 PR 时自动运行，生成覆盖率报告并上传至 Codecov。
