# 单元测试

## 后端测试 (Rust)

```bash
# 运行所有测试
JWT_SECRET="test_secret_key_for_jwt_signing_minimum_64_characters_long_abcd" \
ADMIN_JWT_SECRET="test_admin_secret_key_for_jwt_signing_minimum_64_chars_long" \
cargo test

# 运行特定测试
cargo test --test auth_tests
```

需要设置 JWT 环境变量，否则测试会因缺少密钥而失败。

## 前端测试 (Vitest)

```bash
cd frontend

# 运行测试
npm test

# 监听模式
npm run test:watch

# 生成覆盖率报告
npm run test:coverage
```

前端使用 `happy-dom` 作为测试环境，与 MSW (Mock Service Worker) 兼容性更好。

### 测试覆盖范围

- API 客户端测试（120+ 测试通过）
- 组件渲染测试
- Store 逻辑测试
- 工具函数测试
