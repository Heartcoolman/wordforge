# v1.1 资源包热更 — 后端实施记录

> **角色**：dev-arch-2
> **分支**：`feat/v1-1-resource-pack`
> **基准**：`main` @ `99a54cc`（v1.0.0 GA）
> **客户端对接文档**：[`docs/backend-handoff-resource-pack-v1.1.md`](../backend-handoff-resource-pack-v1.1.md)
> **客户端实现**：`/Users/liji/WordForge-App` `feature/v1.1-resource-pack` @ `b1e1a41`

---

## 一、本 RC 范围

11 个 P0 子任务，10 个原子 commit 交付。覆盖客户端对接文档 §1 的 **必须项 #1-#4 + #6 (Ed25519 签名)**，可选项 #5 (列表) 和 #7 (telemetry 入库) 顺手做完。

| 子任务 | Commit | 内容 |
|---|---|---|
| P0.1 | `a7a6abb` | 迁移 `020_resource_packs` 四表 |
| P0.9 | `97772cc` | AppError 三个资源包错误码 |
| P0.2 | `b1709c9` | `store::operations::resource_packs` 存储层（6 单测） |
| P0.4 | `647115e` | SseEvent::ResourcePackAvailable 三处同步 + 顺手补 v1.0 漏的 2 个事件 |
| P0.6a | `3de239a` | Ed25519 签名模块（6 单测） |
| P0.3+P0.6b | `56046e1` | 公开端点 manifest/list/public-key + AppState 集成 |
| P0.5 | `cc33aec` | static/packs/ immutable cache 头 |
| P0.7 | `e94b4a1` | admin/resource-packs CRUD（5 单测） |
| P0.8 | `c566257` | telemetry `/api/telemetry/resource-pack-install` |
| P0.10 | `f52dd91` | 集成测试 9 个端到端场景 + manifest downloadURL serde 修正 |
| P0.11 | （本 commit） | 文档：api-endpoints §21/§22 + 本文 + CHANGELOG |

---

## 二、与 plan 的偏离

实施过程中两处主动偏离原 plan，均在 commit 注释里说明：

### 2.1 ResourcePackChannel 独立 enum

**Plan 原写**：扩展 `services::updater::Channel { Stable, Beta }` 加 `Internal` 变体。

**实施改正**：`services::updater::Channel` 是二进制自更新 release 通道，扩它会牵连 checker / cache / apply / 测试整套。资源包业务语义不同，需要独立的 Internal 内测通道。

**结果**：新增 `store::operations::resource_packs::ResourcePackChannel { Stable, Beta, Internal }`，serde `rename_all = "lowercase"`，与现有 SSE `release_available` 的 `Channel` 字段在网络层是兼容的字符串值（同 `stable` / `beta`）。

### 2.2 manifest `downloadURL` 字段名

**Plan 原假设**：serde `rename_all = "camelCase"` 自动产生 `downloadURL`。

**实施发现**（集成测试触发）：serde camelCase 把 `download_url` 转成 `downloadUrl`（U 小写），与对接文档 §2.1 约定的 `downloadURL`（全大写 URL）不一致。

**结果**：`ResourcePackManifest` 字段加 `#[serde(rename = "downloadURL")]`，注释明确指出 camelCase 默认行为不对。

---

## 三、关键复用范式（与 v1.0 架构对齐）

| 子任务 | 复用源 | 文件:行 |
|---|---|---|
| Multipart 上传 | `user_profile.rs:341 upload_avatar` 的 raw bytes body | `src/routes/admin/resource_packs.rs::upload_version` |
| Admin 路由 | `admin/updates.rs` 整体 fork | `src/routes/admin/resource_packs.rs` |
| SSE 事件三处同步 | `tests/sse_event_table.rs:1-12` 维护策略 | state.rs + sse_event_table.rs + docs §14 |
| Static 文件托管 + cache 头 | 现有 ServeDir + `static_cache_headers` 中间件的 `/assets/` 分支 | `src/routes/mod.rs:128-170` |
| 迁移文件结构 | `m017_update_audit_log` ISO 8601 TEXT 时间戳风格 | `src/store/migrate.rs` |
| AppState 字段（Arc<RwLock<Option<...>>>） | 既有 `updater` 字段 + set/getter 范式 | `src/state.rs` |
| 错误码 → 客户端 humanAPIMessage | 现有 AppError 静态构造方法集合 | `src/response.rs` |

---

## 四、与 iOS 协调

iOS 客户端 v1.1 POC 对接文档 §6 列了集成点。**注意一处协调改动**：

iOS 原文档写 `GET /api/v1/resource-packs/{packId}/manifest`，但后端 `/api/v1/*` 自 2026-05-21 起冻结 410 Gone（sunset 2027-01-01）。

**协调结果**（用户拍板）：iOS 改路径为 `/api/resource-packs/...`，与 `/api/wordbooks/*`、`/api/word-favorites/*` 主端点风格对齐。iOS 需修改：
- `EndpointServices.swift:519` 把路径前缀从 `/api/v1/resource-packs/` 改为 `/api/resource-packs/`
- 同步 `docs/backend-handoff-resource-pack-v1.1.md` 第一版示例 URL（已过时）

---

## 五、对接 checklist 完成度

对照客户端文档 §5：

### MVP（必过）

- [x] `GET /api/resource-packs/wordbook-core/manifest?appVersion=1.0.0&locale=zh-Hans` 返回 200 + 合法 JSON
- [x] `sha256` 字段是 64 字符 hex lowercase（测试 `admin_upload_computes_sha256_and_valid_signature`）
- [x] `downloadURL` 是 HTTPS（生产时由 `RESOURCE_PACK_BASE_URL` env 注入，否则从 `x-forwarded-proto` 推断）
- [x] payload 文件 SHA256 与 manifest 的 `sha256` 完全一致（上传时一次性算出，落同一个 row）
- [x] manifest 端点无鉴权（注册在 `/api/*` 而非 `/api/admin/*`，handler 无 `AuthUser` extractor）
- [x] 不存在的 packId 返回 404 + `code: "RESOURCE_PACK_NOT_FOUND"`
- [x] SSE `resource_pack_available` 事件能从 admin 切 active 触发（集成测试 `sse_broadcast_dedup_within_5_minutes`）

### 回归（建议）

- [x] 切 channel 后 SSE 立即推送送达（同步调用 `state.broadcast_to_all_sse`）
- [x] 同 packId 5 分钟内重复发布只推 1 次 SSE（`state.try_mark_pack_broadcast` dedup）
- [ ] 旧版本 payload 文件保留 30 天（GC worker 在 v1.1 P2 范围，本 RC 软删除已实现）
- [x] CDN 带 `Cache-Control: immutable`（`/packs/*` 命中 immutable 分支）

### 容错

- [x] manifest 缺 `sizeBytes` / `minAppVersion` / `channel`（全可选）客户端能正常工作（`#[serde(skip_serializing_if = "Option::is_none")]`）
- [x] payload.json 顶层非 dict 时客户端 `RemoteAsset.json` 返回 nil（**这是客户端职责，后端不影响**）

---

## 六、后续 RC 待办（不属于本 RC）

按 plan 章节顺序，下面这些放进 `feat/v1-1-event-bus`（P1）和 `feat/v1-1-tech-debt`（P2）后续 RC：

- P1：S2 records → AMAS 事件总线化
- P2.1：`cargo clippy -- -D warnings` 56 警告清零
- P2.2：14 条 migration down 设计（含本 RC 的 `020_resource_packs`）
- P2.3：rate_limit 区分匿名 / 已登录
- P2.4：SSE 上限提至 5000 + 心跳改 10s
- P2.5：`routes/operations/extras.rs` 拆分
- P2.6：前端 ErrorBoundary 接 Sentry
- P2.7：`docs/release-calendar.md` 补 v1.0 GA + v1.1 milestone
- P2.8：`deploy/nginx/sample.conf` + TLS runbook
- P2.9：admin 维护模式 UI 开关
- P2.10：`update_audit_log` 扩展到资源包 admin 操作 + 其他 admin 敏感动作
- 新增：资源包 GC worker（30 天后清盘已 deactivated 的版本目录）
