# AMAS 配置类型 codegen 工作流

## 目标

后端 Rust struct → JSON Schema → 前端 TypeScript 类型 的**单一事实源链路**，
解决前端手写 `AmasConfig` 字段缺失、与后端 struct 漂移的问题。

## 链路

```
src/amas/config.rs (AMASConfig + 24 个子结构体)
  ↓  #[derive(JsonSchema)]
schemars::schema_for!(AMASConfig)
  ↓  cargo test --test amas_schema_export
admin-ui/src/types/amas.schema.json   ← checked-in，CI 单一事实源
  ↓  npm run gen:amas-types
admin-ui/src/types/amas.generated.ts  ← checked-in，TS 类型
  ↓  re-export 自 admin-ui/src/types/amas.ts
应用代码
```

## 触发条件

后端 `AMASConfig` 或任何子结构体（如 `MemoryModelConfig`, `EloConfig`）
增删字段、改字段类型时，需要刷新前端类型：

```bash
# 1. 刷新 schema（执行集成测试会写入 amas.schema.json）
cargo test --test amas_schema_export

# 2. 由 schema 生成 TS
cd admin-ui
npm run gen:amas-types

# 3. 提交两个文件
git add admin-ui/src/types/amas.schema.json admin-ui/src/types/amas.generated.ts
```

## CI 漂移防护

CI 应在构建前后跑下列序列，确保 PR 没遗漏 regenerate：

```yaml
- name: 重新生成 AMAS schema 与类型
  run: |
    cargo test --test amas_schema_export
    cd admin-ui && npm run gen:amas-types

- name: 校验 schema/类型未漂移
  run: |
    git diff --exit-code \
      admin-ui/src/types/amas.schema.json \
      admin-ui/src/types/amas.generated.ts
```

如果该 step 失败，说明开发者修改了后端 struct 但忘了 regenerate。

## 后端 endpoint（在线）

```
GET /api/admin/amas/config/schema
Authorization: Bearer <admin_token>
```

返回 `AMASConfig` 的 JSON Schema（draft-07）。该 endpoint 主要供：
- 调试 / 后台工具实时查 schema
- 未来若要做"运行时校验"或"动态表单生成"

**生产 codegen 不依赖在线后端**——离线读 `amas.schema.json`，由 cargo test 写入。

## 文件说明

| 文件 | 来源 | 是否手写 |
|------|------|---------|
| `src/amas/config.rs` 等 | 手写 + `#[derive(JsonSchema)]` | 手写 |
| `src/amas/types.rs` | 手写 + `#[derive(JsonSchema)]` | 手写 |
| `admin-ui/src/types/amas.schema.json` | `cargo test --test amas_schema_export` | 自动生成（checked-in） |
| `admin-ui/src/types/amas.generated.ts` | `npm run gen:amas-types` | 自动生成（checked-in） |
| `admin-ui/src/types/amas.ts` | 手写 re-export + 兼容 alias + 运行时类型 | 手写 |

## 兼容性

`amas.generated.ts` 用与后端一致的 PascalCase 命名（`AMASConfig`, `FeatureFlags`,
`MemoryModelConfig` 等）。原有代码使用 `AmasConfig` / `AmasFeatureFlags` 等
带 `Amas` 前缀的命名，在 `amas.ts` 内通过 `@deprecated` alias 平滑兼容，
未来逐步迁移。

## 已知约束

- schemars 0.8 的 `chrono::DateTime<Utc>` 渲染为 `"format": "date-time"` 的 string，
  与现有 TS `string` 类型一致，无需额外处理。
- `[f64; 19]`（FSRS-5 w 权重）渲染为 19 元组类型，前端取下标安全。
- 运行时 endpoint 返回类型（`ProcessResult`, `AmasStrategy` 等）目前**未**走 codegen，
  仍在 `amas.ts` 手写。若需扩展可同样给后端 struct 加 `JsonSchema` 并复用本流程。
