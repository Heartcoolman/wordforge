# 贡献指南

感谢参与 WordForge 开发。本文件说明本地开发流程、代码规范与 PR 标准。

## 分支策略

- `main`：稳定发布分支，**不直接推送**
- 所有改动通过 PR 合并到 `main`
- 分支命名：`feat/<简述>`、`fix/<简述>`、`docs/<简述>`、`refactor/<简述>`

## 提交信息规范

遵循 **Conventional Commits**：

```
<type>(<scope>): <简短描述> (#PR)

[可选：详细说明]
```

常用 type：`feat` / `fix` / `docs` / `refactor` / `test` / `chore` / `perf` / `polish`

示例：
```
fix(updater): 镜像 prefix + 拆 download client read_timeout (#51)
feat(admin): AMAS 调参后台产品化 — 结构化编辑 + 可视化 (#31)
```

## 本地开发三件套

```bash
# 后端测试
cargo test

# 前端单测（Vitest）
cd admin-ui && npm test

# E2E 测试（Playwright）
cd admin-ui && npm run test:e2e
```

完整套件（含覆盖率报告）：

```bash
./run-all-tests.sh
make coverage   # cargo-llvm-cov 生成 HTML 报告
```

## 代码风格

- **Rust**：`cargo fmt` + `cargo clippy --all-targets -- -D warnings`，不允许 warning 进 PR
- **TypeScript / TSX**：`eslint`（项目已配）+ Prettier；禁用 `@ts-ignore` 除非有注释说明原因
- 代码注释**中文为主**，专有名词保留英文原文

## PR 提交前 Checklist

以下为发版前曾发现的典型遗漏，请逐项核查：

- [ ] **lockfile 同步**：`admin-ui/package-lock.json` 与 `package.json` 一致，未出现 "lockfile out of sync" 警告
- [ ] **a11y 与 role**：新增或改动 UI 组件后，若有 `role` / `aria-*` 属性变化，必须跑一次 E2E
- [ ] **二次确认交互**：涉及破坏性操作的按钮（删除 / 升级 / 重置）必须有二次确认弹窗或提示，并在 E2E 中覆盖
- [ ] **表单按钮 disabled 表达式**：`disabled` 逻辑逐个检查——特别注意 `&&` 与 `||` 优先级，以及 signal 为 undefined 时的行为
- [ ] **`paginated()` 字段名**：后端 `paginated()` 返回 `data.data`，前端 API 类型签名一律用 `data: T[]`，不要写 `items`
- [ ] **安全**：不引入 `innerHTML`；外部链接只允许 http/https；无新增 `eval`

## 安全漏洞

安全漏洞**不要**通过 Issue 公开报告，见 [SECURITY.md](SECURITY.md)。
