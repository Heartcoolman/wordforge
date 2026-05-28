## 变更类型

- [ ] feat（新功能）
- [ ] fix（缺陷修复）
- [ ] refactor（重构，不改变行为）
- [ ] docs（文档）
- [ ] test（测试）
- [ ] perf（性能）
- [ ] chore（构建/依赖/配置）

## 描述

<!-- 用一段话说明做了什么、为什么这么做 -->

## 关联 Issue / Task

<!-- Closes #xxx 或 Task #xxx -->

## 测试

- [ ] `cargo test` 全量通过
- [ ] `cd admin-ui && npm test` 通过
- [ ] E2E（`npm run test:e2e`）：受影响路径已覆盖 / 无需 E2E（请说明原因）

## 发版前 Checklist

- [ ] `admin-ui/package-lock.json` 与 `package.json` 已同步
- [ ] 新增 UI 改动：a11y / role 无回归，E2E 已跑
- [ ] 破坏性操作（删除/升级/重置）有二次确认弹窗并有 E2E 覆盖
- [ ] 表单按钮 `disabled` 表达式已逐个核查
- [ ] 后端 `paginated()` 对应前端字段名为 `data` 而非 `items`
- [ ] 无 `innerHTML` / 非 http(s) 外链 / 无 `eval`
